//! Task command seams for team intake and the three plan-bypass guards.
use super::*;
use crate::teams::tasks::{self as bridge, Caller};
use zerus_team_plan::{model::TaskRef, Problem};

/// Single caller seam for the original package. After package 5 is merged,
/// consume its shared native proof here; never add another ancestry check.
fn caller<'a>(config: &'a Config, here: &'a str, who: &'a Actor) -> Caller<'a> {
    Caller {
        state: &config.state,
        machine: here,
        actor: who,
    }
}

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

pub(super) fn add(config: &Config, args: &[String]) -> Result<Option<String>> {
    let team = option(args, "--team")?;
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
    if action != "target" || args.get(1).is_none_or(|s| s != "team") {
        return Ok(None);
    }
    active(config)?;
    let team = args
        .get(2)
        .ok_or_else(|| fail("target REF team TEAM needs a team UUID"))?;
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
    project: &str,
    team: &str,
) -> Result<()> {
    bridge::destination(&config.state, &machines.here, project, team)
        .map(|_| ())
        .map_err(problem)
}

pub(super) fn send(
    config: &Config,
    machines: &Machines,
    who: &Actor,
    task: &TaskRef,
    team: &str,
    args: &[String],
) -> Result<()> {
    bridge::send(
        &caller(config, &machines.here, who),
        task,
        team,
        option(args, "--role")?,
        option(args, "--pipeline")?,
    )
    .map(|_| ())
    .map_err(problem)
}

pub(super) fn guard(
    config: &Config,
    here: &str,
    who: &Actor,
    task: &TaskDoc,
    action: &str,
    args: &[String],
) -> Result<()> {
    let guarded = match action {
        "send" | "handoff" => action,
        "stage" if args.get(1).is_some_and(|s| s == "done") => "done",
        _ => return Ok(()),
    };
    bridge::guard(&caller(config, here, who), task, guarded).map_err(problem)
}

pub(super) fn changed(config: &Config, project: &str, uuid: &str) {
    if let Err(error) = bridge::changed(
        &config.state,
        &TaskRef {
            project: project.into(),
            uuid: uuid.into(),
        },
    ) {
        // The task write succeeded. A projection failure must not misreport a
        // send as refused or cause the caller to repeat an effect.
        eprintln!("hgs: team observation pending: {}", error.message);
    }
}

#[cfg(test)]
mod tests;
