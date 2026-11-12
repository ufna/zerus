//! A cancellation repeats an idempotent task command, never a delivery.
//! Known answers stay durable until their exact decision is settled.
use super::*;
use ports::TaskReply;
use serde::{Deserialize, Serialize};
mod answer;
mod observe;
pub(super) mod settle;
pub(in crate::desktop::teams) use answer::{Answer, Attempt, FactOutcome};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::desktop::teams) struct Pending {
    pub node: u32,
    pub generation: u32,
    pub decision: u64,
    pub task: Option<TaskRef>,
    #[serde(default)]
    pub attempt: u64,
    #[serde(default)]
    pub attempt_id: String,
    // Older visits have no conflict snapshot and cannot prove equality.
    #[serde(default)]
    pub attempt_conflict: Option<bool>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::desktop::teams) struct Fact {
    pub request_id: String,
    pub revision: u64,
    pub pending: Pending,
    pub refused: bool,
    // Missing on older facts: preserve their already reserved cancelled
    // operation and request ID, even when that old build folded a release.
    #[serde(default)]
    pub released: bool,
    pub reason: String,
    #[serde(default)]
    pub attempt: u64,
    #[serde(default)]
    pub attempt_id: String,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    pending: Pending,
    reply: Option<TaskReply>,
    #[serde(default)]
    attempt: Option<Attempt>,
    #[serde(default)]
    deferrals: u32,
    fact: Option<Fact>,
    #[serde(default)]
    children: u8,
    #[serde(default)]
    refusal_exit: bool,
    #[serde(default)]
    halted: bool,
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
        let mut visited = std::collections::BTreeSet::new();
        for row in rows {
            visited.insert(row.key.clone());
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
            if visited.contains(&key) {
                continue;
            }
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
                attempt: None,
                deferrals: 0,
                fact: None,
                children: 0,
                refusal_exit: false,
                halted: false,
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
            match self.terminal_deliveries(store, &t, now) {
                Ok(true) => (),
                Ok(false) => continue,
                Err(error) => {
                    fault = Some(error);
                    continue;
                }
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
            let live = store
                .team(&t.id)?
                .is_some_and(|t| matches!(t.status, TeamStatus::Running | TeamStatus::Paused));
            if live {
                if row.halted {
                    self.force_failure(store, t, key, "", &error, now)?;
                } else {
                    self.failure(store, t, key, "", &error, now)?;
                }
            } else if row.halted {
                return self.for_row(|_| Err(error));
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
        if row.halted {
            if self.cancel_visit_current(store, p, t, row)? {
                return Err(problem(&row.reason));
            }
            return self.cancel_done(store, t, key, now);
        }
        if self.cancel_needs_observation(store, t, row)? {
            // Retire a cached answer from an older build. Observation is now
            // the sole writer for a live task's end; never rewrite that old
            // fact under its already reserved request ID.
            row.reply = None;
            row.fact = None;
            self.put(&t.id, key, row)?;
            return self
                .observe_cancelled(store, p, t, key, row, now)
                .map(|_| ());
        }
        if matches!(row.reply, Some(TaskReply::Cancelled | TaskReply::Released))
            && row.attempt.is_none()
        {
            // A legacy saved success has no attempt evidence. Read again;
            // never invent a pair from the pending row or record that fact.
            row.reply = None;
            row.fact = None;
            self.put(&t.id, key, row)?;
            return self.defer_cancel(store, p, t, key, row, now);
        }
        if row.reply.is_none() {
            return self.invoke_cancel(store, p, t, key, row, now, budget);
        }
        self.settle_cancel(store, p, t, key, row, now)
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke_cancel(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        if *budget == 0 {
            return Ok(());
        }
        if !self.refresh_cancel(store, p, t, key, row, now)? {
            return Ok(());
        }
        if row.children >= 3 && row.pending.task.is_some() {
            self.cancel_refused_exit(
                store,
                t,
                key,
                row,
                "Three cancel command attempts produced no recordable outcome.",
            )?;
            return self.settle_cancel(store, p, t, key, row, now);
        }
        *budget -= 1;
        let answer = self.cancel_command(p, t, key, row)?;
        boundary("effect");
        match &answer.reply {
            TaskReply::Stale => return self.defer_cancel(store, p, t, key, row, now),
            TaskReply::Cancelled | TaskReply::Released if answer.attempt.is_none() => {
                return Err(problem("Cancel returned no attempt evidence"));
            }
            TaskReply::Cancelled | TaskReply::Released | TaskReply::Refused { .. } => (),
            TaskReply::CuttingOver { reason } => return Err(problem(reason)),
            _ => return Err(problem("Cancel answered another command")),
        }
        if matches!(answer.reply, TaskReply::Cancelled)
            && self.observe_cancelled(store, p, t, key, row, now)?
        {
            return Ok(());
        }
        row.reply = Some(answer.reply);
        row.attempt = answer.attempt;
        self.put(&t.id, key, row)?;
        boundary("outcome");
        self.settle_cancel(store, p, t, key, row, now)
    }
    fn refresh_cancel(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
    ) -> Result<bool> {
        let pending = p.cancels(store, &t.id, row.pending.node.saturating_sub(1))?;
        let Some(fresh) = pending.first().filter(|f| {
            f.node == row.pending.node
                && f.generation == row.pending.generation
                && f.decision == row.pending.decision
                && f.task == row.pending.task
        }) else {
            self.cancel_done(store, t, key, now)?;
            return Ok(false);
        };
        if fresh != &row.pending {
            // No answer is cached here. A stale check observed a new pair;
            // after its backoff a fresh admission may check that same decision.
            row.pending = fresh.clone();
            self.put(&t.id, key, row)?;
        }
        Ok(true)
    }
    #[allow(clippy::too_many_arguments)]
    fn defer_cancel(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        t: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
    ) -> Result<()> {
        let team = store
            .team(&t.id)?
            .ok_or_else(|| problem("Cancel team disappeared"))?;
        if !matches!(team.status, TeamStatus::Running | TeamStatus::Paused) {
            return Err(problem("A terminal cancel returned no current answer"));
        }
        let observed = if let Some(task) = &row.pending.task {
            p.observe(store, task)
        } else {
            Ok(())
        };
        row.deferrals = row.deferrals.saturating_add(1);
        row.next_ms = now.saturating_add(retry::delay(row.deferrals));
        self.put(&t.id, key, row)?;
        if matches!(t.status, TeamStatus::Running | TeamStatus::Paused) {
            self.defer(store, t, key, "", now)?;
        }
        observed
    }
    pub(super) fn cancel_done(
        &mut self,
        store: &mut Store,
        t: &Team,
        key: &str,
        now: i64,
    ) -> Result<()> {
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
