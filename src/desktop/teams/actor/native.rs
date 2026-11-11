//! Original local CLI effects; missing part-C readers fail closed.
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
    fn unoccupied(&self, m: &Member) -> Result<bool> {
        let s = m
            .session
            .as_ref()
            .ok_or_else(|| problem("Member has no session"))?;
        let organization = crate::swarm::organization(&self.config, false)
            .map_err(|_| problem("Project inventory unavailable"))?;
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
            let args = [
                "task",
                "list",
                "--project",
                id,
                "--stage",
                "in_progress",
                "--limit",
                "1",
                "--session",
                &s.session_id,
                "--json",
            ]
            .map(String::from);
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
    ) -> Ended {
        let Some(task) = node.task.as_ref() else {
            return Ended::NotStarted;
        };
        let reference = format!("{}/{}", task.project, task.uuid);
        let args = ["task".into(), action.into(), reference, "--json".into()];
        let input = if let Some(m) = member {
            json!({"schema":1,"id":id,"team":team.id,"node":node.id,"member":m.id})
        } else {
            json!({"schema":1,"team":team.id,"node":node.id})
        };
        let input = input.to_string();
        child::run_guarded(
            &self.config,
            &args,
            Some(input.as_bytes()),
            Some(id),
            false,
            Instant::now() + Duration::from_secs(30),
            &self.stop,
            || self.admit(team, node, member, id),
        )
    }
    fn admit(&self, expected: &Team, node: &Node, member: Option<&Member>, id: &str) -> Result<()> {
        let store = Store::open(&self.config.state)?;
        let team = store
            .team(&expected.id)?
            .ok_or_else(|| problem("Team disappeared"))?;
        if team.home != self.machine
            || team.status != TeamStatus::Running
            || team.owner != expected.owner
            || team.project != expected.project
        {
            return Err(problem("Team effect admission changed"));
        }
        if let Some(member) = member {
            if store.member(&team.id, &member.id)?.as_ref() != Some(member) {
                return Err(problem("Member effect admission changed"));
            }
            let mut cursor = None;
            for _ in 0..4 {
                let page = store.open_effects(&team.id, 200, cursor.as_ref())?;
                if page.rows.iter().any(|e| {
                    e.request_id == id
                        && e.node == node.id
                        && e.member == member.id
                        && e.outcome.is_none()
                        && e.state == "reserved"
                }) {
                    return Ok(());
                }
                cursor = page.next;
                if cursor.is_none() {
                    break;
                }
            }
            return Err(problem("Delivery effect is not reserved"));
        }
        let current = store
            .node(&team.id, node.id)?
            .ok_or_else(|| problem("Node disappeared"))?;
        if current.task != node.task {
            return Err(problem("Finishing task binding changed"));
        }
        let mut cursor = None;
        for _ in 0..4 {
            let page = store.pending_accepts(&team.id, 200, cursor.as_ref())?;
            if page.rows.iter().any(|d| d.node == node.id) {
                return Ok(());
            }
            cursor = page.next;
            if cursor.is_none() {
                break;
            }
        }
        Err(problem("Task has no Ready acceptance"))
    }
}
impl Ports for Native {
    fn changes(
        &mut self,
        project: &str,
        generation: &str,
        offset: u64,
    ) -> Result<zerus_replica::Changes> {
        zerus_replica::ProjectStore::open_within(
            &self.config.state.join("replica"),
            project,
            Duration::from_secs(2),
        )
        .map_err(|_| problem("Task change journal is unavailable"))?
        .ok_or_else(|| problem("Task change journal lock is busy"))?
        .changes_since(generation, offset)
        .map_err(|_| problem("Task change journal is unavailable"))
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        Err(problem("Team task observation is not available yet"))
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
    fn deliver(&mut self, team: &Team, node: &Node, member: &Member, id: &str) -> HandoutOutcome {
        match self.command(team, node, Some(member), id, "team-deliver") {
            Ended::Exited { code: _, output } => decode_delivery(&output, id),
            Ended::NotStarted => HandoutOutcome::Refused {
                reason: "Delivery did not start".into(),
            },
            Ended::Unknown(_) => HandoutOutcome::Uncertain {
                reason: "The original delivery child did not settle".into(),
            },
        }
    }
}
pub(super) fn decode_delivery(raw: &[u8], id: &str) -> HandoutOutcome {
    let bad = || HandoutOutcome::Uncertain {
        reason: "The original child returned no authoritative delivery result".into(),
    };
    let Ok(v) = serde_json::from_slice::<Value>(raw) else {
        return bad();
    };
    if v.get("id").is_some_and(|value| value.as_str() != Some(id)) {
        return bad();
    }
    if v.get("schema").is_some_and(|s| s.as_u64() != Some(1)) {
        return bad();
    }
    let reason = cut(
        v["error"]
            .as_str()
            .or(v["reason"].as_str())
            .unwrap_or("The task writer did not complete delivery"),
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
    pub(super) fn done(&self, team: &Team, node: &Node, id: &str) -> Result<super::ports::Finish> {
        match self.command(team, node, None, id, "team-done") {
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
