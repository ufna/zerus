//! Service writes whose authority is a stored decision or released identity.
use super::*;
use zerus_team_plan::model::NodeKind;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: u32,
    team: String,
    node: u32,
}
impl Input {
    fn reply(&self, state: &str, error: &str) -> Value {
        json!({"schema":1,"team":self.team,"node":self.node,"state":state,"error":error})
    }
}

pub(crate) fn execute(config: &Config, action: &str, args: &[String]) -> Result<Value> {
    if !crate::teams::enabled(&config.state) {
        return Err(fail("teams_inactive: Agent Teams preview is off."));
    }
    arguments(args, true, false)?;
    let body: Input = serde_json::from_str(&stdin_text("team service request")?).map_err(fail)?;
    if body.schema != 1 || !zerus_replica::ids::is_uuid(&body.team) || body.node == 0 {
        return Err(fail("invalid_request: Invalid team service request."));
    }
    match write(config, action, args, &body) {
        Ok(state) => Ok(body.reply(state, "")),
        Err(error) if error.message.starts_with(UNCONFIRMED_WRITE) => Err(error),
        Err(error) => Ok(body.reply(
            if error.message == BUSY {
                "cutting_over"
            } else {
                "refused"
            },
            &error.message,
        )),
    }
}

pub(crate) fn dispatch(config: &Config, action: &str, args: &[String]) -> Result<i32> {
    match execute(config, action, args) {
        Ok(reply) => {
            println!("{reply}");
            Ok(0)
        }
        Err(error) if error.message.starts_with(UNCONFIRMED_WRITE) => Err(error),
        Err(error) => {
            println!(
                "{}",
                json!({"schema":1,"state":"refused","error":error.message})
            );
            Ok(1)
        }
    }
}

fn bound(store: &Store, action: &str, body: &Input, here: &str) -> Result<(Team, Node, TaskRef)> {
    let team = store
        .team(&body.team)
        .map_err(problem)?
        .ok_or_else(|| fail("Team not found."))?;
    if team.home != here {
        return Err(fail("Team belongs to another computer."));
    }
    if action != "team-clear" && team.status != TeamStatus::Running {
        return Err(fail("Team is not Running."));
    }
    let db = store
        .db
        .as_ref()
        .ok_or_else(|| fail("Team store is unavailable."))?;
    let node = crate::teams::store::rows::node(db, &body.team, body.node)
        .map_err(problem)?
        .ok_or_else(|| fail("Node not found."))?;
    let task = if let Some(task) = node.task.clone() {
        if task.project != team.project {
            return Err(fail("Node task belongs to another project."));
        }
        let exact:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM node_identity WHERE team=?1 AND id=?2 AND task_project=?3 AND task_uuid=?4)",params![body.team,body.node,task.project,task.uuid],|r|r.get(0)).map_err(fail)?;
        if !exact {
            return Err(fail("Node task identity differs."));
        }
        task
    } else if action == "team-clear" && node.kind == NodeKind::Stage {
        let id:Option<String>=db.query_row("SELECT s.request_id FROM stage_tasks s JOIN node_identity i ON i.team=s.team AND i.id=s.node WHERE s.team=?1 AND s.node=?2 AND i.task_project=?3 AND i.task_uuid=''",params![body.team,body.node,team.project],|r|r.get(0)).optional().map_err(fail)?.flatten();
        let uuid = id
            .filter(|id| zerus_replica::ids::is_uuid(id))
            .ok_or_else(|| {
                fail("Released stage has no recorded creation identity in this project.")
            })?;
        TaskRef {
            project: team.project.clone(),
            uuid,
        }
    } else {
        return Err(fail("Node has no task in this project."));
    };
    Ok((team, node, task))
}

fn authority(
    store: &Store,
    action: &str,
    body: &Input,
    team: &Team,
    node: &Node,
    reference: &TaskRef,
) -> Result<()> {
    let db = store.db.as_ref().expect("bound store");
    let released =
        crate::teams::store::rows::released(db, &body.team, body.node).map_err(problem)?;
    if action == "team-clear" {
        let retired:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM node_archive WHERE team=?1 AND id=?2) AND NOT EXISTS(SELECT 1 FROM nodes WHERE team=?1 AND id=?2) AND NOT EXISTS(SELECT 1 FROM stage_tasks WHERE team=?1 AND node=?2 AND state IN ('wanted','reserved','created'))",params![body.team,body.node],|r|r.get(0)).map_err(fail)?;
        let rebound: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM live_tasks WHERE task_project=?1 AND task_uuid=?2)",
                params![reference.project, reference.uuid],
                |r| r.get(0),
            )
            .map_err(fail)?;
        let clearable_kind = node.kind == NodeKind::Stage
            || (node.kind == NodeKind::Task
                && matches!(team.status, TeamStatus::Stopped | TeamStatus::Archived));
        if !clearable_kind || !retired || rebound {
            return Err(fail(
                "Node is not a released stage or a stopped or archived team's retired task.",
            ));
        }
    } else {
        if released {
            return Err(fail("A released node cannot finish a task."));
        }
        let authorized:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM decisions d JOIN (SELECT generation,integration_required,integrated FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT generation,integration_required,integrated FROM node_archive WHERE team=?1 AND id=?2) n ON n.generation=d.generation WHERE d.team=?1 AND d.node=?2 AND d.kind='accept' AND (d.state='ready' OR (d.state='settled' AND d.stopped_ready=1)) AND (NOT n.integration_required OR ?3='stage' OR (n.integrated AND EXISTS(SELECT 1 FROM integration_facts f WHERE f.team=d.team AND f.node=d.node AND f.decision=d.seq AND f.commit_id IS NOT NULL AND f.conflicts='[]'))))",params![body.team,body.node,if node.kind==NodeKind::Stage{"stage"}else{"task"}],|r|r.get(0)).map_err(fail)?;
        if !authorized {
            return Err(fail(
                "No accepted generation with the required integration authority.",
            ));
        }
    }
    Ok(())
}

fn write(config: &Config, action: &str, args: &[String], body: &Input) -> Result<&'static str> {
    let deadline = Instant::now() + attempts::BUDGET;
    let here = identity(config)?;
    let teams = Store::open(&config.state).map_err(problem)?;
    let (team, node, reference) = bound(&teams, action, body, &here)?;
    authority(&teams, action, body, &team, &node, &reference)?;
    #[cfg(test)]
    super::stage::before_store(config);
    let mut scoped = args.to_vec();
    if option(args, "--project")?.is_none() {
        scoped.extend(["--project".into(), reference.project.clone()]);
    }
    let r = super::super::reference(config, &scoped, &Actor::Person)?;
    if r.project.0 != reference.project {
        return Err(fail("REF is not this node's task."));
    }
    if !super::super::store_dir(config, &reference.project).is_dir() {
        return if action == "team-clear" && r.task == reference.uuid {
            Ok("cleared")
        } else {
            Err(fail("REF is not this node's task."))
        };
    }
    let tasks = super::super::store_until(config, &reference.project, deadline)?;
    let mut admitted = tasks.admit_until(deadline).ok_or_else(|| fail(BUSY))?;
    let db = teams.db.as_ref().expect("bound store");
    db.execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| fail(BUSY))?;
    let (team, node, fresh) = bound(&teams, action, body, &here)?;
    if fresh != reference {
        return Err(fail(dispatch::CHANGED));
    }
    authority(&teams, action, body, &team, &node, &reference)?;
    if action == "team-clear" && !task_present(config, &reference)? {
        return if r.task == reference.uuid {
            Ok("cleared")
        } else {
            Err(fail("REF is not this node's task."))
        };
    }
    let mut doc = admitted.load_complete(&reference.uuid).map_err(fail)?;
    if r.task != reference.uuid && r.task != doc.id() {
        return Err(fail("REF is not this node's task."));
    }
    if action == "team-clear" && doc.team().map_err(fail)?.is_none() && !doc.team_invalid() {
        return Ok("cleared");
    }
    marker(&doc, &body.team, &here)?;
    if action != "team-clear" {
        let current = doc.view().map_err(fail)?;
        if current.cancelled_at.is_some() || current.execution.as_ref().is_some_and(|e| e.conflict)
        {
            return Err(fail("Task execution changed."));
        }
        let expected:Option<String>=db.query_row("SELECT request_id FROM effects WHERE team=?1 AND node=?2 AND generation=(SELECT generation FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT generation FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1) ORDER BY seq DESC LIMIT 1",params![body.team,body.node],|r|r.get(0)).optional().map_err(fail)?;
        if expected
            .as_ref()
            .is_some_and(|id| current.execution_id().as_ref() != Some(id))
        {
            return Err(fail("Task now belongs to another attempt."));
        }
        if current.stage == Stage::Done {
            return Ok("done");
        }
        if !matches!(current.stage, Stage::Active | Stage::Review) {
            return Err(fail("Task is no longer active or in review."));
        }
    }
    if admitted
        .project()
        .map_err(fail)?
        .cutover_hold()
        .is_some_and(|h| h.on)
    {
        return Err(fail(BUSY));
    }
    let lead = teams
        .member(&body.team, &team.lead)
        .map_err(problem)?
        .and_then(|m| m.session)
        .filter(|s| s.machine == here && !s.session_id.is_empty())
        .ok_or_else(|| fail("Lead has no local session binding."))?;
    let who = Actor::Agent(SessionKey {
        machine: lead.machine,
        session_id: lead.session_id,
        session: String::new(),
    });
    if action == "team-clear" {
        crate::teams::tasks::clear_named(&mut doc, &team.name, &who).map_err(problem)?;
    } else {
        doc.finish_team_accepted(&who, now()).map_err(fail)?;
    }
    admitted
        .save(&mut doc)
        .map_err(|e| fail(format!("{UNCONFIRMED_WRITE} {e}")))?;
    #[cfg(test)]
    if std::env::var_os("HGS_TEST_TEAM_COMPLETION_AFTER_SAVE").is_some() {
        std::process::exit(79);
    }
    Ok(if action == "team-clear" {
        "cleared"
    } else {
        "done"
    })
}
