use super::*;
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
struct Gate {
    revision: String,
    failures: u32,
    at: i64,
    card: Option<u64>,
    notice: Option<Request>,
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
        self.for_row(|a| a.failure_write(store, team, key, revision, e, now))
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
        key: &str,
        revision: &str,
        e: &Problem,
        now: i64,
    ) -> Result<()> {
        let name = format!("gate:{key}");
        let mut g = self.load::<Gate>(&team.id, &name)?.unwrap_or_default();
        g.failures = g.failures.saturating_add(1);
        g.revision = revision.into();
        g.at = now.saturating_add(60_000 * (1i64 << g.failures.saturating_sub(1).min(4)).min(15));
        self.put(&team.id, &name, &g)?;
        if g.failures >= 3 && g.card.is_none() {
            if g.notice.is_none() {
                g.notice=Some(request(store,&team.id,Operation::OpenCard{kind:"team_actor_blocked".into(),node:None,text:format!("{}: {}. Automatic retry uses a backoff from one to fifteen minutes. Choosing Retry checks on the next service pass (60-second safety interval).",cut(key,200),cut(&e.message,2000))})?.1);
                self.put(&team.id, &name, &g)?;
            }
            let actor = Actor::Service {
                person: team.owner.clone(),
                team: team.id.clone(),
            };
            match ops::apply(store, &actor, g.notice.as_ref().expect("reserved"), now) {
                Ok(reply) => g.card = reply.item,
                Err(e) if e.code == code::STALE_REVISION => g.notice = None,
                Err(e) => return Err(e),
            }
            self.put(&team.id, &name, &g)?;
        }
        Ok(())
    }
    fn recover_gate(&mut self, store: &mut Store, team: &Team, key: &str, now: i64) -> Result<()> {
        let name = format!("gate:{key}");
        if let Some(row) = self.ledger.get(&team.id, &name)? {
            let g: Gate =
                serde_json::from_value(row.data.clone()).map_err(crate::teams::store::storage)?;
            if let Some(card) = g.card {
                if store
                    .card(&team.id, card)?
                    .is_some_and(|c| c.state == "open")
                {
                    write(
                        store,
                        &team.id,
                        Operation::CloseCard {
                            card,
                            reason: "The service check recovered".into(),
                        },
                        now,
                    )?;
                }
            }
            self.ledger.remove(&row)?;
        }
        Ok(())
    }
}
