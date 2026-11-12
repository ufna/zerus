//! Kept card state and opener receipts share one policy for both gate paths.
use super::*;
pub(super) fn decisions() -> Vec<String> {
    vec!["retry".into(), "acknowledged".into()]
}
impl Runtime {
    pub(super) fn current_gate_card(
        &mut self,
        store: &mut Store,
        team: &Team,
        name: &str,
        g: &mut Gate,
        now: i64,
    ) -> Result<()> {
        let Some(id) = g.card else { return Ok(()) };
        let kept = store.card(&team.id, id)?;
        if kept
            .as_ref()
            .is_some_and(|c| crate::desktop::teams::kept::stands(c, &decisions(), &["retry"]))
        {
            return Ok(());
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
        self.put(&team.id, name, g)?;
        boundary("replacement_reset");
        Ok(())
    }
    pub(super) fn open_gate_card(
        &mut self,
        store: &mut Store,
        team: &Team,
        scope: (&str, &str),
        g: &mut Gate,
        error: &Problem,
        now: i64,
    ) -> Result<()> {
        let (key, name) = scope;
        if g.notice.is_none() {
            g.notice = Some(request(store, &team.id, operation(key, error))?.1);
            self.put(&team.id, name, g)?;
            boundary("notice_reserved");
        }
        self.apply_gate_notice(store, team, name, g, now)
    }
    pub(super) fn apply_gate_notice(
        &mut self,
        store: &mut Store,
        team: &Team,
        name: &str,
        g: &mut Gate,
        now: i64,
    ) -> Result<()> {
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
        self.put(&team.id, name, g)?;
        boundary("notice_saved");
        Ok(())
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
