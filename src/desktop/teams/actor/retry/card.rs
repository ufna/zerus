//! Kept card state and opener receipts share one policy for both gate paths.
use super::*;
pub(super) fn decisions() -> Vec<String> {
    vec!["retry".into(), "acknowledged".into()]
}
impl Runtime {
    pub(super) fn reserve_gate(
        &mut self,
        team: &str,
        name: &str,
        row: Option<Row>,
        g: &Gate,
    ) -> Result<Option<Row>> {
        if let Some(mut row) = row {
            return Ok(self.save_gate(&mut row, g)?.then_some(row));
        }
        let data = serde_json::to_value(g).map_err(crate::teams::store::storage)?;
        self.ledger.reserve(team, name, data).map(Some)
    }
    pub(super) fn save_gate(&mut self, row: &mut Row, g: &Gate) -> Result<bool> {
        row.data = serde_json::to_value(g).map_err(crate::teams::store::storage)?;
        self.ledger.save_if_current(row)
    }
    pub(super) fn current_gate_card(
        &mut self,
        store: &mut Store,
        team: &Team,
        row: &mut Row,
        g: &mut Gate,
        now: i64,
    ) -> Result<bool> {
        let Some(id) = g.card else { return Ok(true) };
        let kept = store.card(&team.id, id)?;
        if kept
            .as_ref()
            .is_some_and(|c| crate::desktop::teams::kept::stands(c, &decisions(), &["retry"]))
        {
            return Ok(true);
        }
        if kept.is_some_and(|c| c.state == "open") {
            write(
                store,
                &team.id,
                Operation::CloseCard {
                    card: id,
                    reason: "The service card's current decisions changed.".into(),
                },
                now,
            )?;
            boundary("replacement_closed");
        }
        g.card = None;
        g.notice = None;
        if !self.save_gate(row, g)? {
            return Ok(false);
        }
        boundary("replacement_reset");
        Ok(true)
    }
    pub(super) fn open_gate_card(
        &mut self,
        store: &mut Store,
        team: &Team,
        scope: (&str, &Problem),
        row: &mut Row,
        g: &mut Gate,
        now: i64,
    ) -> Result<bool> {
        let (key, error) = scope;
        if g.notice.is_none() {
            g.notice = Some(request(store, &team.id, operation(key, error))?.1);
            if !self.save_gate(row, g)? {
                return Ok(false);
            }
            boundary("notice_reserved");
        }
        self.apply_gate_notice(store, team, row, g, now)
    }
    pub(super) fn apply_gate_notice(
        &mut self,
        store: &mut Store,
        team: &Team,
        row: &mut Row,
        g: &mut Gate,
        now: i64,
    ) -> Result<bool> {
        let actor = Actor::Service {
            person: team.owner.clone(),
            team: team.id.clone(),
        };
        match ops::apply(store, &actor, g.notice.as_ref().expect("reserved"), now) {
            Ok(reply) => {
                boundary("notice_applied");
                g.card = reply.item;
            }
            Err(e) if e.code == code::STALE_REVISION => g.notice = None,
            Err(e) => return Err(e),
        }
        if !self.save_gate(row, g)? {
            return Ok(false);
        }
        boundary("notice_saved");
        Ok(true)
    }
}
fn operation(key: &str, error: &Problem) -> Operation {
    let timing = if key.starts_with("finish_refused:") {
        "The service waits for this task's observation or acceptance to change before another finish attempt. Retry checks that state on the next pass."
    } else {
        "Automatic checks use a backoff from one to fifteen minutes. Choosing Retry checks on the next service pass (60-second safety interval)."
    };
    Operation::OpenCard {
        kind: "team_actor_blocked".into(),
        node: key
            .strip_prefix("held:")
            .and_then(|s| s.rsplit(':').next())
            .and_then(|s| s.parse().ok()),
        decisions: Some(decisions()),
        text: if key.starts_with("held:") {
            cut(&error.message, 3000)
        } else {
            format!(
                "{}: {}. {}",
                cut(key, 200),
                cut(&error.message, 2000),
                timing
            )
        },
    }
}
