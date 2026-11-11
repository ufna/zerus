//! Team intake and observations of the ordinary replicated task document.
//!
//! Lock order: resolve catalog/caller identity first, then open the project
//! task store, then apply short team-store operations. Never open a project
//! store while a team transaction is held. No session effect occurs here.
use super::{
    ops,
    store::{storage, Store},
};
use ops::{Actor, Operation, PersonOrigin, Reply, Request, TaskObservation, TaskStage};
use rusqlite::{params, OptionalExtension};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zerus_replica::{Actor as TaskActor, ProjectStore, Stage, Target, TaskDoc};
use zerus_team_plan::{
    model::{Member, MemberState, SessionKey, TaskRef, Team, TeamStatus},
    problem::code,
    Problem,
};

type Result<T> = std::result::Result<T, Problem>;

mod replica_link;
pub(crate) mod context;

/// The ordinary task command already verified this caller's live run and
/// process ancestry. Neither an operation nor stdin can construct it.
pub(crate) struct Caller<'a> {
    pub state: &'a Path,
    pub machine: &'a str,
    pub actor: &'a TaskActor,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn request(store: &Store, team: &str, operation: Operation) -> Result<Request> {
    let team = store
        .team(team)?
        .ok_or_else(|| Problem::new(code::NOT_FOUND, "Team not found."))?;
    Ok(Request {
        request_id: uuid::Uuid::new_v4().to_string(),
        team: team.id,
        expected_revision: team.revision,
        operation,
    })
}

pub(crate) fn destination(state: &Path, machine: &str, project: &str, id: &str) -> Result<Team> {
    let store = Store::open(state)?;
    let team = store
        .team(id)?
        .ok_or_else(|| Problem::new(code::NOT_FOUND, "Team not found."))?;
    if team.home != machine || team.project != project || !live(&team) || !team.settings.queue {
        return Err(Problem::new(
            code::INVALID_STATE,
            "Choose an active queue team of this project on this computer.",
        ));
    }
    Ok(team)
}

fn live(team: &Team) -> bool {
    matches!(team.status, TeamStatus::Running | TeamStatus::Paused)
}

fn actor(store: &Store, team: &Team, who: &TaskActor) -> Result<Actor> {
    let TaskActor::Agent(key) = who else {
        return Ok(Actor::Person {
            person: team.owner.clone(),
            origin: PersonOrigin::CliInterim,
        });
    };
    let session = SessionKey {
        machine: key.machine.clone(),
        session_id: key.session_id.clone(),
    };
    let member: Option<Member> = if let Some(db) = &store.db {
        db.query_row(
            "SELECT data FROM members WHERE team=?1 AND machine=?2 AND session_id=?3 LIMIT 1",
            params![team.id, key.machine, key.session_id],
            |row| super::store::sql_json(row, 0),
        )
        .optional()
        .map_err(storage)?
    } else {
        None
    };
    Ok(
        match member.filter(|m| m.state != MemberState::Removed && !key.session_id.is_empty()) {
            Some(member) => Actor::Session {
                person: team.owner.clone(),
                team: team.id.clone(),
                member: member.id,
                session,
            },
            None => Actor::Other {
                person: team.owner.clone(),
                session,
            },
        },
    )
}

/// Admission is the caller's operation. The following fact is constructed
/// only from the task document this code loads under its project lock.
pub(crate) fn send(
    caller: &Caller<'_>,
    task: &TaskRef,
    team: &str,
    role: Option<&str>,
    pipeline: Option<&str>,
) -> Result<Reply> {
    let selected = destination(caller.state, caller.machine, &task.project, team)?;
    let tasks =
        ProjectStore::open(&caller.state.join("replica"), &task.project).map_err(storage)?;
    let mut document = load(&tasks, &task.uuid)?;
    let mut store = Store::open(caller.state)?;
    let existing = binding(&store, task)?;
    let view = document.view().map_err(storage)?;
    if existing.as_ref().is_none_or(|(id, _)| id != team)
        && (!matches!(view.stage, Stage::Backlog | Stage::Todo) || view.cancelled_at.is_some())
    {
        return Err(Problem::new(
            code::INVALID_STATE,
            "Send a task in Backlog or To do to a team.",
        ));
    }
    link(&mut document, &selected, caller.actor)?;
    let who = actor(&store, &selected, caller.actor)?;
    let add = request(
        &store,
        team,
        Operation::AddTask {
            task: task.clone(),
            role: role.map(str::to_owned),
            pipeline: pipeline.map(str::to_owned),
        },
    )?;
    let reply = ops::apply(&mut store, &who, &add, now())?;
    tasks.save(&mut document).map_err(storage)?;
    record(
        &mut store,
        team,
        reply.node.expect("AddTask returns its node"),
        &document,
    )?;
    Ok(Reply {
        revision: store.team(team)?.expect("recorded team").revision,
        ..reply
    })
}

/// Prepare on the in-memory document before admission so validation failures
/// write neither store. An already running duplicate keeps its executor.
fn link(document: &mut TaskDoc, team: &Team, who: &TaskActor) -> Result<()> {
    if document.view().map_err(storage)?.target != Target::Any
        && matches!(
            document.stage().map_err(storage)?,
            Stage::Backlog | Stage::Todo
        )
    {
        document
            .set_target(&Target::Any, who, now())
            .map_err(storage)?;
    }
    replica_link::write(document, team, who, now())
}

fn load(store: &ProjectStore, uuid: &str) -> Result<TaskDoc> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    store
        .admit_until(deadline)
        .ok_or_else(|| storage("Task store is busy."))?
        .load_complete(uuid)
        .map_err(storage)
}

fn observation(document: &TaskDoc) -> Result<TaskObservation> {
    let view = document.view().map_err(storage)?;
    Ok(TaskObservation {
        revision: document.revision(),
        previous_revision: String::new(),
        stage: match view.stage {
            Stage::Backlog => TaskStage::Backlog,
            Stage::Todo => TaskStage::Todo,
            Stage::Active => TaskStage::InProgress,
            Stage::Review => TaskStage::InReview,
            Stage::Done => TaskStage::Done,
        },
        cancelled: view.cancelled_at.is_some(),
        rank: view.rank,
        title: view.title,
    })
}

/// The service reads by UUID, never accepting a state or revision from a caller.
#[allow(dead_code)] // Also consumed by the following teams actor package.
pub(crate) fn observe(state: &Path, project: &str, uuid: &str) -> Result<TaskObservation> {
    let tasks = ProjectStore::open(&state.join("replica"), project).map_err(storage)?;
    observation(&load(&tasks, uuid)?)
}

fn record(store: &mut Store, team: &str, node: u32, document: &TaskDoc) -> Result<()> {
    let mut observation = observation(document)?;
    observation.previous_revision = store
        .task_observation(team, node)?
        .map_or(String::new(), |o| o.revision);
    if observation.previous_revision == observation.revision {
        return Ok(());
    }
    let req = request(store, team, Operation::ObserveTask { node, observation })?;
    let owner = store.team(team)?.expect("observed team").owner;
    ops::apply(
        store,
        &Actor::Service {
            person: owner,
            team: team.into(),
        },
        &req,
        now(),
    )?;
    Ok(())
}

/// Indexed lookup supports local teams even when no replica key was written.
fn binding(store: &Store, task: &TaskRef) -> Result<Option<(String, u32)>> {
    store.db.as_ref().map_or(Ok(None), |db| {
        db.query_row(
            "SELECT team,node FROM live_tasks WHERE task_project=?1 AND task_uuid=?2",
            params![task.project, task.uuid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)
    })
}

pub(crate) fn guard(caller: &Caller<'_>, task: &TaskDoc, action: &str) -> Result<()> {
    if matches!(caller.actor, TaskActor::Person) {
        return Ok(());
    }
    let denied = || {
        Problem::new(
            code::NOT_ALLOWED,
            format!("Only this team's home lead or a person may {action} a team task."),
        )
    };
    let link = replica_link::read(task)?;
    if link
        .as_ref()
        .is_some_and(|link| link.home != caller.machine)
    {
        return Err(denied());
    }
    if !super::enabled(caller.state) {
        return Ok(());
    }
    let store = Store::open(caller.state)?;
    let reference = TaskRef {
        project: task.view().map_err(storage)?.project,
        uuid: task.uuid().into(),
    };
    let id = binding(&store, &reference)?
        .map(|(id, _)| id)
        .or_else(|| link.map(|l| l.team));
    let Some(team) = id
        .map(|id| store.team(&id))
        .transpose()?
        .flatten()
        .filter(live)
    else {
        return Ok(());
    };
    let who = actor(&store, &team, caller.actor)?;
    if team.home == caller.machine
        && matches!(who, Actor::Session { member, session, .. } if member == team.lead && session.machine == caller.machine)
    {
        Ok(())
    } else {
        Err(denied())
    }
}

/// Observe a successful ordinary task edit, including a person's override.
/// The caller releases its project store first; this code reads it afresh.
pub(crate) fn changed(state: &Path, task: &TaskRef) -> Result<()> {
    if !super::enabled(state) {
        return Ok(());
    }
    let tasks = ProjectStore::open(&state.join("replica"), &task.project).map_err(storage)?;
    let mut store = Store::open(state)?;
    let found = binding(&store, task)?;
    let found = match (found, &store.db) {
        (Some(binding), _) => Some(binding),
        (None, Some(db)) => db.query_row("SELECT i.team,i.id FROM node_identity i JOIN teams t ON t.id=i.team WHERE i.task_project=?1 AND i.task_uuid=?2 ORDER BY t.rowid DESC,i.id DESC LIMIT 1",
            params![task.project,task.uuid], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?,
        _ => None,
    };
    if let Some((team, node)) = found {
        record(&mut store, &team, node, &load(&tasks, &task.uuid)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
