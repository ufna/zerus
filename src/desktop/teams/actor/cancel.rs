//! A cancellation repeats an idempotent task command, never a delivery.
//! Known answers stay durable until their exact decision is settled.
use super::*;
use ports::TaskReply;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::desktop::teams) struct Pending {
    pub node: u32,
    pub generation: u32,
    pub decision: u64,
    pub task: Option<TaskRef>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::desktop::teams) struct Fact {
    pub request_id: String,
    pub revision: u64,
    pub pending: Pending,
    pub refused: bool,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    pending: Pending,
    reply: Option<TaskReply>,
    fact: Option<Fact>,
    failures: u32,
    next_ms: i64,
    reason: String,
}
impl Runtime {
    pub(super) fn cancels(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        now: i64,
    ) -> Result<()> {
        let mut budget = 4;
        let after = self
            .load::<String>(&t.id, "cancel_recovery_cursor")?
            .unwrap_or_default();
        let rows = self.ledger.prefix_page(&t.id, "cancel:", &after)?;
        let next = if rows.len() == 200 {
            rows.last().unwrap().key.clone()
        } else {
            String::new()
        };
        for row in rows {
            self.cancel_row(store, p, t, &row.key, now, &mut budget)?;
        }
        self.put(&t.id, "cancel_recovery_cursor", &next)?;
        let after = self.load::<u32>(&t.id, "cancel_cursor")?.unwrap_or(0);
        let rows = p.cancels(store, &t.id, after)?;
        let next = if rows.len() == 16 {
            rows.last().unwrap().node
        } else {
            0
        };
        for pending in rows {
            if p.stopped() {
                break;
            }
            let key = format!("cancel:{}", pending.decision);
            self.reserve_cancel(t, &key, pending)?;
            self.cancel_row(store, p, t, &key, now, &mut budget)?;
        }
        self.put(&t.id, "cancel_cursor", &next).map(|_| ())
    }
    fn reserve_cancel(&mut self, t: &Team, key: &str, pending: Pending) -> Result<()> {
        if self.ledger.get(&t.id, key)?.is_none() {
            let row = Entry {
                pending,
                reply: None,
                fact: None,
                failures: 0,
                next_ms: 0,
                reason: String::new(),
            };
            self.put(&t.id, key, &row)?;
            boundary("reserved");
        }
        Ok(())
    }
    pub(super) fn terminal_cancels(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        now: i64,
    ) -> Result<()> {
        let ids = store.keyed_teams(&self.cancel_after, 16)?;
        self.cancel_after = if ids.len() == 16 {
            ids.last().unwrap().clone()
        } else {
            String::new()
        };
        let mut fault = None;
        for id in ids {
            if p.stopped() {
                break;
            }
            let Some(t) = store.team(&id)? else { continue };
            if t.home != self.machine {
                continue;
            }
            if let Err(error) = self.cancels(store, p, &t, now) {
                fault = Some(error);
            }
        }
        fault.map_or(Ok(()), Err)
    }
    fn cancel_row(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        let Some(mut row) = self.load::<Entry>(&t.id, key)? else {
            return Ok(());
        };
        let live = matches!(t.status, TeamStatus::Running | TeamStatus::Paused);
        if p.stopped()
            || if live {
                !self.due(store, t, key, "", now)?
            } else {
                now < row.next_ms
            }
        {
            return Ok(());
        }
        if let Err(error) = self.advance_cancel(store, p, t, key, &mut row, now, budget) {
            row.failures = row.failures.saturating_add(1);
            row.next_ms = now.saturating_add(retry::delay(row.failures));
            row.reason = cut(&error.message, 2048);
            self.put(&t.id, key, &row)?;
            if live {
                self.failure(store, t, key, "", &error, now)?;
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_cancel(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        if row.reply.is_none() {
            if *budget == 0 {
                return Ok(());
            }
            let pending = p.cancels(store, &t.id, row.pending.node.saturating_sub(1))?;
            if pending.first() != Some(&row.pending) {
                return self.cancel_done(store, t, key, now);
            }
            *budget -= 1;
            let reply = if row.pending.task.is_none() {
                TaskReply::Cancelled
            } else {
                p.cancel(t, &row.pending, &uuid::Uuid::new_v4().to_string())?
            };
            boundary("effect");
            match reply {
                TaskReply::Cancelled | TaskReply::Released | TaskReply::Refused { .. } => (),
                TaskReply::CuttingOver { ref reason } => return Err(problem(reason)),
                _ => return Err(problem("Cancel answered another command")),
            }
            row.reply = Some(reply);
            self.put(&t.id, key, row)?;
            boundary("outcome");
        }
        self.settle_cancel(store, p, t, key, row, now)
    }
    fn settle_cancel(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
    ) -> Result<()> {
        if row.fact.is_none() {
            row.fact = Some(Fact {
                request_id: uuid::Uuid::new_v4().to_string(),
                revision: store
                    .team(&t.id)?
                    .ok_or_else(|| problem("Cancel team disappeared"))?
                    .revision,
                pending: row.pending.clone(),
                refused: matches!(row.reply, Some(TaskReply::Refused { .. })),
                reason: match &row.reply {
                    Some(TaskReply::Refused { reason }) => cut(reason, 2048),
                    _ => String::new(),
                },
            });
            self.put(&t.id, key, row)?;
        }
        match p.record_cancel(store, t, row.fact.as_ref().unwrap(), now) {
            Ok(()) => {
                boundary("recorded");
                self.cancel_done(store, t, key, now)
            }
            Err(error) if error.code == code::STALE_REVISION => {
                row.fact = None;
                self.put(&t.id, key, row).map(|_| ())
            }
            Err(error) => Err(error),
        }
    }
    fn cancel_done(&mut self, store: &mut Store, t: &Team, key: &str, now: i64) -> Result<()> {
        if matches!(t.status, TeamStatus::Running | TeamStatus::Paused) {
            self.recovered(store, t, key, now)?;
        }
        for name in [key.to_owned(), format!("gate:{key}")] {
            if let Some(row) = self.ledger.get(&t.id, &name)? {
                self.ledger.remove(&row)?;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
thread_local! {pub(super) static CRASH:std::cell::Cell<Option<&'static str>>=const{std::cell::Cell::new(None)};}
fn boundary(point: &str) {
    #[cfg(test)]
    CRASH.with(|s| {
        if s.get() == Some(point) {
            s.set(None);
            panic!("fixture restart at cancel {point}")
        }
    });
    #[cfg(not(test))]
    let _ = point;
}
