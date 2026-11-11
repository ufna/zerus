use super::*;
use zerus_replica::execution::{Origin, Phase};

fn acknowledgement(task: &TaskDoc, actor: &TaskActor) -> Result<bool> {
    let TaskActor::Agent(key) = actor else {
        return Ok(false);
    };
    let view = task.view().map_err(storage)?;
    if view.stage != Stage::Active {
        return Ok(false);
    }
    let same = |holder: &zerus_replica::SessionKey| {
        !key.session_id.is_empty()
            && holder.machine == key.machine
            && holder.session_id == key.session_id
    };
    // The execution projection includes legacy dispatches, and distinguishes
    // their holder from a direct claim (Origin::Start) on either fence route.
    Ok(view
        .execution
        .and_then(|e| e.current)
        .is_some_and(|attempt| {
            attempt.origin != Origin::Start
                && attempt.executor.as_ref().is_some_and(same)
                && matches!(attempt.phase, Phase::Submitted | Phase::Running)
                && (!attempt.legacy || attempt.at > 0)
        }))
}

/// An agent decides through hgs team; only the service drives a team task.
pub(crate) fn guard(caller: &Caller<'_>, task: &TaskDoc, action: &str) -> Result<()> {
    if !matches!(
        action,
        "send"
            | "handoff"
            | "stage"
            | "done"
            | "target"
            | "start"
            | "worker"
            | "reopen"
            | "cancel"
            | "cancel-execution"
            | "move"
            | "edit"
            | "rename"
    ) {
        return Ok(());
    }
    if matches!(caller.actor, TaskActor::Person) {
        return Ok(());
    }
    let link = task.team().map_err(storage)?;
    let invalid = task.team_invalid();
    if link.is_none() && !invalid {
        return Ok(());
    }
    if matches!(action, "start" | "worker") && acknowledgement(task, &caller.actor)? {
        return Ok(());
    }
    let denied = || {
        Problem::new(code::NOT_ALLOWED,format!("An agent cannot {action} a live team's task; the team service performs this action."))
    };
    if invalid {
        return Err(denied());
    }
    let link = link.expect("checked marker");
    if link.home != caller.machine {
        return Err(denied());
    }
    if !super::super::enabled(caller.state) {
        return Ok(());
    }
    let store = Store::open(caller.state)?;
    let Some(header) = header::read(&store, &link.team)? else {
        return Ok(());
    };
    if !header.live() {
        return Ok(());
    }
    let reference = TaskRef {
        project: task.view().map_err(storage)?.project,
        uuid: task.uuid().into(),
    };
    if binding(&store, &reference)?.is_some_and(|(team, _)| team == link.team)
        || (reference.project == header.project
            && super::creation::pending(&store, &reference, &link.team)?)
    {
        Err(denied())
    } else {
        Ok(())
    }
}

pub(crate) fn clear(state: &Path, task: &mut TaskDoc, actor: &TaskActor) -> Result<()> {
    let link = task.team().map_err(storage)?;
    if link.is_none() && !task.team_invalid() {
        return Ok(());
    }
    let mut name = link
        .as_ref()
        .map_or_else(|| "unknown team".into(), |link| link.team.clone());
    if super::super::enabled(state) {
        if let Some(link) = &link {
            if let Ok(store) = Store::open(state) {
                if let Ok(Some(header)) = header::read(&store, &link.team) {
                    name = header.name;
                }
            }
        }
    }
    clear_named(task, &name, actor)
}

/// Shared marker writer for an authority already checked by a service command.
pub(crate) fn clear_named(task: &mut TaskDoc, name: &str, actor: &TaskActor) -> Result<()> {
    task.set_team_named(None, name, actor, now())
        .map_err(storage)
}
