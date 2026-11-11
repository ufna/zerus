//! Task command seams for team intake and caller guards.
use super::*;
use crate::teams::tasks::{self as bridge, Caller};
use zerus_team_plan::{model::TaskRef, Problem};

fn problem(error: Problem) -> Error {
    fail(format!("{}: {}", error.code, error.message))
}
fn active(config: &Config) -> Result<()> {
    if crate::teams::enabled(&config.state) {
        Ok(())
    } else {
        Err(fail(
            "teams_inactive: Agent Teams preview is off on this computer.",
        ))
    }
}
fn routing(args: &[String], team: bool) -> Result<()> {
    if !team && (option(args, "--role")?.is_some() || option(args, "--pipeline")?.is_some()) {
        return Err(fail(
            "--role and --pipeline require --team TEAM or target REF team TEAM",
        ));
    }
    Ok(())
}
pub(super) fn add(config: &Config, args: &[String]) -> Result<Option<String>> {
    let team = option(args, "--team")?;
    routing(args, team.is_some())?;
    if let Some(team) = team {
        active(config)?;
        if !zerus_replica::ids::is_uuid(team) {
            return Err(fail("a team is named by its UUID"));
        }
        if flag(args, "--start")
            || option(args, "--target")?.is_some()
            || option(args, "--host")?.is_some()
        {
            return Err(fail(
                "--team takes the team's queue; use it without --start, --target or --host",
            ));
        }
    }
    Ok(team.map(str::to_owned))
}
pub(super) fn target(config: &Config, action: &str, args: &[String]) -> Result<Option<String>> {
    if action != "target" {
        return Ok(None);
    }
    let team = args.get(2).filter(|value| !value.starts_with("--"));
    let form = args.get(1).is_some_and(|s| s == "team") && team.is_some();
    routing(args, form)?;
    if !form {
        return Ok(None);
    }
    active(config)?;
    let team = team.expect("team form");
    if !zerus_replica::ids::is_uuid(team) {
        return Err(fail("a team is named by its UUID"));
    }
    if option(args, "--host")?.is_some() {
        return Err(fail("target REF team TEAM uses this computer's team"));
    }
    Ok(Some(team.clone()))
}
pub(super) fn destination(
    config: &Config,
    machines: &Machines,
    who: &Actor,
    project: &str,
    team: &str,
    args: &[String],
) -> Result<()> {
    bridge::preflight(
        &Caller {
            state: &config.state,
            machine: &machines.here,
            actor: who,
        },
        project,
        team,
        option(args, "--role")?,
        option(args, "--pipeline")?,
        option(args, "--request-id")?,
    )
    .map_err(problem)
}
pub(super) fn send(
    config: &Config,
    machines: &Machines,
    who: &Actor,
    task: &TaskRef,
    team: &str,
    args: &[String],
) -> Result<Value> {
    let result = bridge::send(
        &Caller {
            state: &config.state,
            machine: &machines.here,
            actor: who,
        },
        task,
        team,
        option(args, "--role")?,
        option(args, "--pipeline")?,
    );
    let mut out = json!({"task_exists":true,"team":{"id":team,"queued":false}});
    match result {
        Ok(intake) => {
            out["team"]["queued"] = json!(intake.queued);
            if let Some(node) = intake.node {
                out["team"]["node"] = json!(node);
            }
            if let Some(error) = intake.refusal {
                out["team"]["error"] = json!(error);
            }
            if let Some(error) = intake.observation_pending {
                out["team"]["observation_pending"] = json!(true);
                out["message"] = json!(format!(
                    "Task was queued; its first observation is pending: {}",
                    error.message
                ));
            }
        }
        Err(error) => out["team"]["error"] = json!(error),
    }
    Ok(out)
}
pub(super) fn annotate(mut out: Value) -> Value {
    if out["team"]["queued"] == false {
        let message = format!(
            "Task {} exists and was not queued ({}): {}",
            out["task"]["ref"].as_str().unwrap_or(""),
            out["team"]["error"]["code"]
                .as_str()
                .unwrap_or("invalid_state"),
            out["team"]["error"]["message"]
                .as_str()
                .unwrap_or("Team intake was refused.")
        );
        if out.get("answer").is_some() {
            out["answer"] = json!(message);
        }
        out["message"] = json!(message);
    } else if out["team"]["observation_pending"] == true {
        if let Some(answer) = out["answer"].as_str() {
            out["answer"] = json!(format!("{answer} Observation is pending."));
        }
    }
    out
}
pub(super) fn guard(
    config: &Config,
    here: &str,
    who: &Actor,
    task: &TaskDoc,
    action: &str,
    args: &[String],
) -> Result<()> {
    let action = match action {
        "send" | "handoff" | "stage" | "target" | "start" | "worker" | "reopen" | "cancel"
        | "cancel-execution" | "move" | "edit" | "rename" => action,
        "message" if option(args, "--mode")? == Some("reopen") => "reopen",
        _ => return Ok(()),
    };
    bridge::guard(
        &Caller {
            state: &config.state,
            machine: here,
            actor: who,
        },
        task,
        action,
    )
    .map_err(problem)
}
pub(super) fn clear(config: &Config, task: &mut TaskDoc, who: &Actor) -> Result<()> {
    bridge::clear(&config.state, task, who).map_err(problem)
}
#[cfg(test)]
mod tests;
