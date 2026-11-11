//! Original local CLI effects with exact stored admissions.
//! Occupancy is a scheduling hint read just before admission, outside the
//! process-wide spawn turn because it starts child readers. A task a person
//! assigns in the remaining race can reach the member alongside this hand-out.
//! The exact current fleet row is checked last inside the store admission.

mod tasks;
#[cfg(test)]
#[allow(unused_imports)]
pub(super) use tasks::{clear_request, stage_arguments, stage_request, task_reply};
mod admission;
use super::*;
use crate::{
    config::Config,
    desktop::teams::child::{self, Ended},
    teams::ops::HandoutOutcome,
};
use serde_json::{json, Value};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
pub(in crate::desktop::teams) struct Native {
    pub config: Config,
    pub machine: String,
    pub stop: Arc<AtomicBool>,
    pub epoch: String,
}
impl Native {
    fn read(&self, args: &[String], deadline: Instant) -> Result<Value> {
        match child::run(&self.config, args, None, None, false, deadline, &self.stop) {
            Ended::Exited { code: 0, output } => {
                serde_json::from_slice(&output).map_err(|_| problem("Invalid bounded CLI read"))
            }
            _ => Err(problem("Bounded CLI read is unavailable")),
        }
    }
    fn project_inventory(&self) -> Result<Value> {
        #[cfg(test)]
        if let Some(value) = TEST_INVENTORY.with(|s| s.borrow().clone()) {
            return Ok(value);
        }
        crate::swarm::organization(&self.config, false)
            .map_err(|_| problem("Project inventory unavailable"))
    }
    pub(super) fn unoccupied(&self, m: &Member) -> Result<bool> {
        let s = m
            .session
            .as_ref()
            .ok_or_else(|| problem("Member has no session"))?;
        let organization = self.project_inventory()?;
        if organization["node_id"] != self.machine {
            return Err(problem("Project machine changed"));
        }
        let projects = organization["organization"]["projects"]
            .as_array()
            .ok_or_else(|| problem("Project inventory unavailable"))?;
        if projects.len() > 128 {
            return Err(problem("Task occupancy project bound exceeded"));
        }
        let until = Instant::now() + Duration::from_secs(3);
        for project in projects {
            if Instant::now() >= until {
                return Err(problem("Task occupancy read deadline"));
            }
            let id = project["id"]
                .as_str()
                .ok_or_else(|| problem("Invalid project identity"))?;
            let args = occupancy_arguments(id, &s.session_id);
            let r = self.read(&args, until)?;
            if r["damaged_total"].as_u64() != Some(0) {
                return Err(problem("Task occupancy is unknown"));
            }
            let tasks = r["tasks"]
                .as_array()
                .ok_or_else(|| problem("Task occupancy is unknown"))?;
            if !tasks.is_empty() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn command(
        &self,
        team: &Team,
        node: &Node,
        member: Option<&Member>,
        id: &str,
        action: &str,
        decision: Option<&crate::teams::store::Decision>,
    ) -> Result<Ended> {
        let Some(task) = node.task.as_ref() else {
            return Ok(Ended::NotStarted);
        };
        if let Some(member) = member {
            if !self.unoccupied(member)? {
                return Err(problem("The member has another active task"));
            }
        }
        let args = task_arguments(task, action);
        let input = if let Some(m) = member {
            json!({"schema":1,"id":id,"team":team.id,"node":node.id,"member":m.id})
        } else {
            json!({"schema":1,"team":team.id,"node":node.id})
        };
        let input = input.to_string();
        let mut retry = None;
        let ended = child::run_guarded(
            &self.config,
            &args,
            Some(input.as_bytes()),
            Some(id),
            false,
            Instant::now() + Duration::from_secs(30),
            &self.stop,
            || {
                self.admit(team, node, member, id, decision)
                    .inspect_err(|e| {
                        if e.code == crate::teams::store::actor_reads::ADMISSION_BUSY {
                            retry = Some(e.clone());
                        }
                    })
            },
        );
        if let Some(error) = retry {
            return Err(error);
        }
        Ok(ended)
    }
}
impl Ports for Native {
    fn clock_ms(&self, _: i64) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
    fn stage(
        &mut self,
        team: &Team,
        effect: &crate::teams::store::StageTaskEffect,
        original: &str,
    ) -> Result<super::ports::TaskReply> {
        self.create_task(team, effect, original)
    }
    fn clear_stage(
        &mut self,
        team: &Team,
        keyed: &crate::teams::store::timers::KeyedTask,
        original: &str,
    ) -> Result<super::ports::TaskReply> {
        self.clear_task(team, keyed, original)
    }
    fn stopped(&self) -> bool {
        self.stop.load(std::sync::atomic::Ordering::Acquire)
    }
    fn changes(
        &mut self,
        project: &str,
        generation: &str,
        offset: u64,
    ) -> Result<zerus_replica::Changes> {
        Ok(zerus_replica::ProjectStore::read_changes_since(
            &self.config.state.join("replica"),
            project,
            generation,
            offset,
        ))
    }
    fn observe(&mut self, _: &mut Store, task: &TaskRef) -> Result<()> {
        crate::teams::tasks::observe(&self.config.state, task)
    }
    fn report(&mut self, task: &TaskRef) -> Result<Option<String>> {
        crate::teams::tasks::report(&self.config.state, task)
    }
    fn free(&mut self, team: &Team, m: &Member) -> Result<bool> {
        let Some(session) = m.session.as_ref() else {
            return Ok(false);
        };
        if session.machine != self.machine
            || !super::availability::free(&self.config, &self.epoch, &team.id, m)?
        {
            return Ok(false);
        }
        self.unoccupied(m)
    }
    fn repository(&mut self, team: &Team) -> Result<String> {
        self.work_repository(team)
    }
    fn reported(
        &mut self,
        team: &Team,
        member: &Member,
    ) -> Result<crate::teams::integrate::Reported> {
        self.work_reported(team, member)
    }
    fn descends(&mut self, plan: &super::ports::MergePlan) -> Result<bool> {
        self.work_descends(plan)
    }
    fn contains(&mut self, plan: &super::ports::MergePlan) -> Result<Option<String>> {
        self.work_contains(plan)
    }
    fn merge(&mut self, plan: &super::ports::MergePlan) -> super::ports::MergeResult {
        self.work_merge(plan)
    }
    fn finish(
        &mut self,
        team: &Team,
        node: &Node,
        decision: &crate::teams::store::Decision,
        id: &str,
    ) -> Result<super::ports::Finish> {
        self.done(team, node, decision, id)
    }
    fn deliver(
        &mut self,
        team: &Team,
        node: &Node,
        member: &Member,
        id: &str,
    ) -> ports::DeliveryResult {
        match self.command(team, node, Some(member), id, "team-deliver", None) {
            Ok(ended) => delivery_reply(ended, id).into(),
            Err(error) => ports::DeliveryResult::NotStarted {
                reason: error.message,
            },
        }
    }
}
pub(super) fn delivery_reply(ended: Ended, id: &str) -> HandoutOutcome {
    match ended {
        Ended::Exited { code, output } => {
            let reply = decode_delivery(&output, id);
            if code != 0 && matches!(reply, HandoutOutcome::Submitted { .. }) {
                HandoutOutcome::Uncertain {
                    reason: "A failed child cannot confirm delivery".into(),
                }
            } else {
                reply
            }
        }
        Ended::NotStarted => HandoutOutcome::Refused {
            reason: "Delivery did not start".into(),
        },
        Ended::Unknown(_) => HandoutOutcome::Uncertain {
            reason: "The original delivery child did not settle".into(),
        },
    }
}
pub(super) fn decode_delivery(raw: &[u8], id: &str) -> HandoutOutcome {
    let bad = || HandoutOutcome::Uncertain {
        reason: "The original child returned no authoritative delivery result".into(),
    };
    let Ok(v) = serde_json::from_slice::<Value>(raw) else {
        return bad();
    };
    if v["id"].as_str() != Some(id) {
        return bad();
    }
    if v["schema"].as_u64() != Some(1) {
        return bad();
    }
    let Some(error) = v["error"].as_str() else {
        return bad();
    };
    let reason = cut(
        if error.trim().is_empty() {
            "The task writer did not complete delivery"
        } else {
            error
        },
        2048,
    );
    match v["state"].as_str() {
        Some("submitted") => HandoutOutcome::Submitted { receipt: id.into() },
        Some("refused") => HandoutOutcome::Refused { reason },
        Some("uncertain") => HandoutOutcome::Uncertain { reason },
        Some("cutting_over") => HandoutOutcome::CuttingOver { reason },
        _ => bad(),
    }
}
impl Native {
    pub(super) fn done(
        &self,
        team: &Team,
        node: &Node,
        decision: &crate::teams::store::Decision,
        id: &str,
    ) -> Result<super::ports::Finish> {
        match self.command(team, node, None, id, "team-done", Some(decision))? {
            Ended::Exited { code: 0, output } => decode_finish(&output, &team.id, node.id),
            _ => Err(problem(
                "The original finishing child did not settle successfully",
            )),
        }
    }
}
pub(super) fn decode_finish(raw: &[u8], team: &str, node: u32) -> Result<super::ports::Finish> {
    use super::ports::Finish;
    let v: Value = serde_json::from_slice(raw).map_err(|_| problem("Malformed finishing reply"))?;
    if v["schema"] != 1 || v["team"] != team || v["node"].as_u64() != Some(u64::from(node)) {
        return Err(problem("Finishing reply does not echo this request"));
    }
    let reason = v["error"]
        .as_str()
        .ok_or_else(|| problem("Finishing reply has no error field"))?;
    Ok(match v["state"].as_str() {
        Some("done") => Finish::Done,
        Some("refused") => Finish::Refused(cut(reason, 2048)),
        Some("cutting_over") => Finish::CuttingOver(cut(reason, 2048)),
        _ => return Err(problem("Unknown finishing reply state")),
    })
}

/// Shared with real-command contract tests; no adapter reconstructs this vector.
pub(super) fn occupancy_arguments(project: &str, session: &str) -> Vec<String> {
    [
        "task",
        "list",
        "--project",
        project,
        "--stage",
        "active",
        "--limit",
        "1",
        "--session",
        session,
        "--json",
    ]
    .map(String::from)
    .into()
}
pub(super) fn task_arguments(task: &TaskRef, action: &str) -> Vec<String> {
    vec![
        "task".into(),
        action.into(),
        format!("{}/{}", task.project, task.uuid),
        "--json".into(),
    ]
}

#[cfg(test)]
thread_local! {pub(super) static TEST_INVENTORY: std::cell::RefCell<Option<Value>> = const {std::cell::RefCell::new(None)};}
