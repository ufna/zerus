//! Held decisions use the existing retry rows, with their own node and text.
use super::*;
impl Runtime {
    pub(in crate::desktop::teams::actor) fn held_step(
        &mut self,
        store: &mut Store,
        team: &Team,
        target: (&str, u32, u32, u64, &str),
        ready: bool,
        now: i64,
    ) -> Result<bool> {
        let (step, node, generation, decision, kind) = target;
        let key = format!("held:{step}:{node}");
        if ready {
            self.recovered(store, team, &key, now)?;
            return Ok(true);
        }
        let observation = store.task_observation(&team.id, node)?;
        let reason = sentence(node, kind, decision, observation.as_ref());
        let gate = self.load::<Gate>(&team.id, &format!("gate:{key}"))?;
        if gate.is_some_and(|g| g.event.as_deref() != Some(reason.as_str())) {
            self.recovered(store, team, &key, now)?;
        }
        let stamp = serde_json::to_string(&(
            decision,
            generation,
            observation.as_ref().map(|o| &o.revision),
        ))
        .map_err(crate::teams::store::storage)?;
        if self.due(store, team, &key, &stamp, now)? {
            self.failure(store, team, &key, &stamp, &problem(&reason), now)?;
        }
        Ok(false)
    }
}
impl Runtime {
    pub(in crate::desktop::teams::actor) fn held_retired(
        &self,
        store: &Store,
        team: &Team,
        row: &Row,
        step: &str,
        node: u32,
        reopens: &std::collections::BTreeSet<u64>,
    ) -> Result<bool> {
        let gate: Gate =
            serde_json::from_value(row.data.clone()).map_err(crate::teams::store::storage)?;
        let (decision, generation, _): (u64, u32, Option<String>) =
            serde_json::from_str(&gate.revision).map_err(crate::teams::store::storage)?;
        let Some(n) = store.node(&team.id, node)? else {
            return Ok(true);
        };
        if store.generation(&team.id, node)? != Some(generation) {
            return Ok(true);
        }
        if step == "reopen" {
            return Ok(!reopens.contains(&decision));
        }
        if !store.ready_accept(&team.id, node, generation, decision)? {
            return Ok(true);
        }
        Ok(if step == "merge" {
            team.settings.integration != Integration::Branch || n.integrated
        } else {
            team.settings.integration == Integration::Branch
                && n.kind != NodeKind::Stage
                && !n.integrated
        })
    }
}

fn sentence(node: u32, kind: &str, decision: u64, o: Option<&ops::TaskObservation>) -> String {
    let seen = match o {
        Some(o) if o.attempt_conflict => "shows executions that conflict".into(),
        Some(o) if o.stage == ops::TaskStage::InReview => {
            "is in review under another execution than the team's".into()
        }
        Some(o) => format!("shows the task at {}", stage(o.stage)),
        None => "has no readable observation of the bound task".into(),
    };
    format!("Task {} waits for {} decision {}. Its newest observation {}. It continues automatically when this is settled on the board.", node, kind, decision, seen)
}
fn stage(stage: ops::TaskStage) -> &'static str {
    match stage {
        ops::TaskStage::Backlog => "Backlog",
        ops::TaskStage::Todo => "To do",
        ops::TaskStage::InProgress => "In progress",
        ops::TaskStage::InReview => "In review",
        ops::TaskStage::Done => "Done",
    }
}
