//! Team intake and service observations of ordinary replicated tasks.
//! Project locks precede team operations; no code here performs a session effect.
use super::{
    ops,
    store::{storage, Guard, Store},
};
use ops::{Actor, Operation, Request, TaskObservation, TaskStage};
use rusqlite::{params, OptionalExtension};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zerus_replica::{Actor as TaskActor, ProjectStore, Stage, TaskDoc};
use zerus_team_plan::{model::TaskRef, problem::code, Problem};

type Result<T> = std::result::Result<T, Problem>;
mod header;
mod intake;
mod policy;
pub(crate) use intake::{preflight, send};
pub(crate) use policy::{clear, guard};

/// Native caller proof is resolved by the task command before entering here.
pub(crate) struct Caller<'a> {
    pub state: &'a Path,
    pub machine: &'a str,
    pub actor: TaskActor,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
fn request(store: &Store, team: &str, operation: Operation) -> Result<Request> {
    let header = header::read(store, team)?
        .ok_or_else(|| Problem::new(code::NOT_FOUND, "Team not found."))?;
    Ok(Request {
        request_id: uuid::Uuid::new_v4().to_string(),
        team: team.into(),
        expected_revision: header.revision,
        operation,
    })
}
fn load(store: &ProjectStore, uuid: &str) -> Result<TaskDoc> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    store
        .admit_until(deadline)
        .ok_or_else(|| storage("Task store is busy."))?
        .load_complete(uuid)
        .map_err(storage)
}
fn observation(document: &TaskDoc, team: &str, home: &str) -> Result<TaskObservation> {
    let view = document.view().map_err(storage)?;
    let key = match document.team().map_err(storage)? {
        Some(key) if key.team == team && key.home == home => ops::KeyState::Ours,
        None if !document.team_invalid() => ops::KeyState::Absent,
        _ => ops::KeyState::Other,
    };
    Ok(TaskObservation {
        key,
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
fn record(store: &mut Store, team: &str, node: u32, document: &TaskDoc) -> Result<()> {
    let header = header::read(store, team)?.ok_or_else(|| Guard::ObservedTeam.problem())?;
    let mut observation = observation(document, team, &header.home)?;
    observation.previous_revision = store
        .task_observation(team, node)?
        .map_or(String::new(), |o| o.revision);
    if matches!(observation.key, ops::KeyState::Ours)
        && observation.previous_revision == observation.revision
    {
        return Ok(());
    }
    let req = Request {
        request_id: uuid::Uuid::new_v4().to_string(),
        team: team.into(),
        expected_revision: header.revision,
        operation: Operation::ObserveTask { node, observation },
    };
    ops::apply(
        store,
        &Actor::Service {
            person: header.owner,
            team: team.into(),
        },
        &req,
        now(),
    )?;
    Ok(())
}
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

/// Service entry: load and record under the same project lock. An observation
/// read before a later task write can never be carried into this function.
#[allow(dead_code)] // Called by the team service.
pub(crate) fn observe(state: &Path, task: &TaskRef) -> Result<()> {
    if !super::enabled(state) {
        return Err(Problem::new(code::TEAMS_INACTIVE, "Teams preview is off."));
    }
    let tasks = ProjectStore::open(&state.join("replica"), &task.project).map_err(storage)?;
    let document = load(&tasks, &task.uuid)?;
    #[cfg(test)]
    observe_pause();
    let mut store = Store::open(state)?;
    if let Some((team, node)) = binding(&store, task)? {
        record(&mut store, &team, node, &document)?;
    }
    Ok(())
}

#[cfg(test)]
fn observe_pause() {
    let Some(path) = std::env::var_os("HGS_TEST_TEAM_OBSERVE_PAUSE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    if std::fs::write(path.join("ready"), "loaded").is_err() {
        return;
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while !path.join("release").exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Called by retarget while its task command retains the project lock. A
/// refused release leaves this in-memory edit unsaved, including its key.
pub(crate) fn retarget(
    state: &Path,
    here: &str,
    document: &mut TaskDoc,
    actor: &TaskActor,
) -> Result<()> {
    let marked = document.team().map_err(storage)?.is_some() || document.team_invalid();
    clear(state, document, actor)?;
    if marked && super::enabled(state) {
        let mut store = Store::open(state)?;
        let reference = TaskRef {
            project: document.view().map_err(storage)?.project,
            uuid: document.uuid().into(),
        };
        if let Some((team, node)) = binding(&store, &reference)? {
            // A replaced or malformed key cannot hide this computer's live
            // binding. The stored team's home decides who releases it.
            let header =
                header::read(&store, &team)?.ok_or_else(|| Guard::RetargetedTeam.problem())?;
            if header.home == here {
                record(&mut store, &team, node, document)?;
            }
        }
    }
    Ok(())
}

/// Package 10 clears the marker only if this exact team still owns it.
#[allow(dead_code)] // Called by package 10 after terminal-team reconciliation.
pub(crate) fn clear_team_key(
    state: &Path,
    task: &TaskRef,
    expected_team: &str,
    actor: &TaskActor,
) -> Result<bool> {
    if !super::enabled(state) {
        return Err(Problem::new(code::TEAMS_INACTIVE, "Teams preview is off."));
    }
    let tasks = ProjectStore::open(&state.join("replica"), &task.project).map_err(storage)?;
    let mut document = load(&tasks, &task.uuid)?;
    if document
        .team()
        .map_err(storage)?
        .as_ref()
        .is_none_or(|link| link.team != expected_team)
    {
        return Ok(false);
    }
    let link = document.team().map_err(storage)?.expect("checked marker");
    let store = Store::open(state)?;
    let Some(header) = header::read(&store, expected_team)? else {
        return Ok(false);
    };
    if link.home != header.home {
        return Ok(false);
    }
    if header.live() {
        return Err(Problem::new(
            code::INVALID_STATE,
            "Only a terminal team's marker is cleared by this service operation.",
        ));
    }
    clear(state, &mut document, actor)?;
    tasks.save(&mut document).map_err(storage)?;
    Ok(true)
}
#[cfg(test)]
mod tests;
