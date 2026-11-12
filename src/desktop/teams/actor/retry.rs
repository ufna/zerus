mod card;
mod held;
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
struct Gate {
    revision: String,
    failures: u32,
    #[serde(default)]
    deferrals: u32,
    at: i64,
    card: Option<u64>,
    notice: Option<Request>,
    #[serde(default)]
    event: Option<String>,
}
impl Runtime {
    pub(super) fn due(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        now: i64,
    ) -> Result<bool> {
        self.for_row(|a| a.due_read(store, team, key, revision, now))
    }
    pub(super) fn failure(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        e: &Problem,
        now: i64,
    ) -> Result<()> {
        self.for_row(|a| a.failure_write(store, team, (key, revision, None), e, now, false))
    }
    pub(super) fn recovered(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        now: i64,
    ) -> Result<()> {
        self.for_row(|a| a.recover_gate(store, team, key, now))
    }

    pub(super) fn force_failure(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        e: &Problem,
        now: i64,
    ) -> Result<()> {
        self.for_row(|a| a.failure_write(store, team, (key, revision, None), e, now, true))
    }
    pub(super) fn recorded_failure(
        &mut self,
        store: &mut Store,
        team: &Team,
        scope: (&str, &str),
        event: &str,
        e: &Problem,
        now: i64,
    ) -> Result<()> {
        self.for_row(|a| {
            a.failure_write(store, team, (scope.0, scope.1, Some(event)), e, now, false)
        })
    }
    pub(super) fn cooldown(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        at: i64,
    ) -> Result<()> {
        self.recovered(store, team, key, at.saturating_sub(60_000))?;
        self.for_row(|a| {
            a.put(
                &team.id,
                &format!("gate:{key}"),
                &Gate {
                    revision: revision.into(),
                    at,
                    ..Default::default()
                },
            )?;
            Ok(())
        })
    }
    /// A stale task is an answer, not a fault. Observe before calling this;
    /// keep the normal bounded backoff without an error count or a new card.
    pub(super) fn defer(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        now: i64,
    ) -> Result<()> {
        self.for_row(|a| {
            let deferrals = a
                .load::<Gate>(&team.id, &format!("gate:{key}"))?
                .map_or(1, |g| g.deferrals.saturating_add(1));
            a.recover_gate(store, team, key, now)?;
            a.put(
                &team.id,
                &format!("gate:{key}"),
                &Gate {
                    revision: revision.into(),
                    deferrals,
                    at: now.saturating_add(delay(deferrals)),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
    }
    fn due_read(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        revision: &str,
        now: i64,
    ) -> Result<bool> {
        let Some(mut g) = self.load::<Gate>(&team.id, &format!("gate:{key}"))? else {
            return Ok(true);
        };
        if let Some(id) = g.card {
            if store
                .card(&team.id, id)?
                .is_some_and(|c| c.state != "open" && c.decision.as_deref() == Some("retry"))
            {
                g.card = None;
                g.notice = None;
                g.at = now;
                self.put(&team.id, &format!("gate:{key}"), &g)?;
                return Ok(true);
            }
        }
        Ok(now >= g.at || !revision.is_empty() && revision != g.revision)
    }
    fn failure_write(
        &mut self,
        store: &mut Store,
        team: &Team,
        scope: (&str, &str, Option<&str>),
        e: &Problem,
        now: i64,
        force: bool,
    ) -> Result<()> {
        let (key, revision, event) = scope;
        let name = format!("gate:{key}");
        let mut g = self.load::<Gate>(&team.id, &name)?.unwrap_or_default();
        if event.is_none() || g.event.as_deref() != event {
            g.failures = g.failures.saturating_add(1);
            g.revision = revision.into();
            g.at = now.saturating_add(delay(g.failures));
            g.event = if key.starts_with("held:") {
                Some(e.message.clone())
            } else {
                event.map(String::from)
            };
        }
        self.put(&team.id, &name, &g)?;
        boundary("gate_saved");
        // A pending pre-upgrade opener must first replay its own receipt;
        // the second iteration can replace its now-outdated decision list.
        for _ in 0..2 {
            self.current_gate_card(store, team, &name, &mut g, now)?;
            if !(force || g.failures >= 3) || g.card.is_some() {
                return Ok(());
            }
            self.open_gate_card(store, team, (key, &name), &mut g, e, now)?;
        }
        Ok(())
    }
    fn recover_gate(&mut self, store: &mut Store, team: &Team, key: &str, now: i64) -> Result<()> {
        let name = format!("gate:{key}");
        if let Some(row) = self.ledger.get(&team.id, &name)? {
            let mut g: Gate =
                serde_json::from_value(row.data.clone()).map_err(crate::teams::store::storage)?;
            // Settle an interrupted opener before forgetting its receipt.
            if g.card.is_none() && g.notice.is_some() {
                self.apply_gate_notice(store, team, &name, &mut g, now)?;
            }
            if let Some(card) = g.card {
                if store
                    .card(&team.id, card)?
                    .is_some_and(|c| c.state == "open")
                {
                    boundary("recovery_before_close");
                    write(
                        store,
                        &team.id,
                        Operation::CloseCard {
                            card,
                            reason: "The service check recovered".into(),
                        },
                        now,
                    )?;
                    boundary("recovery_closed");
                }
            }
            // Applying a saved notice can have advanced the ledger version.
            if let Some(current) = self.ledger.get(&team.id, &name)? {
                self.ledger.remove(&current)?;
                boundary("recovery_removed");
            }
        }
        Ok(())
    }
}

pub(super) fn delay(failures: u32) -> i64 {
    60_000 * (1i64 << failures.saturating_sub(1).min(4)).min(15)
}

#[cfg(test)]
thread_local! {
    pub(super) static CRASH: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}
fn boundary(point: &str) {
    #[cfg(test)]
    CRASH.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            panic!("fixture card restart at {point}");
        }
    });
    #[cfg(not(test))]
    let _ = point;
}
