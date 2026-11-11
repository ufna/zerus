//! The Teams original is this computer's own hgs, with its own close-only
//! custody descriptor. No agent identity or presentation parent is inherited.
use crate::{
    config::Config,
    desktop::original_child::{self, Codes, Input, Output, Spec, Stdin},
};
pub(super) use original_child::Ended;
use std::{
    fs::OpenOptions,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
use zerus_client_core::run::custody::InheritedLock;
pub(super) struct Original<'a> {
    pub id: &'a str,
    pub plan: &'a super::ports::Plan,
    pub bound: bool,
}
const CUSTODY: &str = "HGS_TEAMS_CUSTODY_FD";
const CODES: Codes = Codes {
    unobserved: "team_child_unobserved",
    unreadable: "team_output_unreadable",
    signalled: "team_child_signalled",
    timed_out: "team_child_timed_out",
    retained: "team_output_retained",
};

pub(crate) fn adopt_custody() -> Result<(), String> {
    original_child::adopt(
        CUSTODY,
        "Invalid Teams custody",
        "Teams custody is unavailable",
    )
}
pub(super) fn run(
    config: &Config,
    args: &[String],
    input: Option<&[u8]>,
    original: Option<Original<'_>>,
    stderr: bool,
    deadline: Instant,
    stop: &AtomicBool,
) -> Ended {
    let Ok(program) = crate::self_exec::current() else {
        return Ended::NotStarted;
    };
    let input = input.map_or(Input::Closed, |bytes| Input::Frozen {
        stdin: Stdin::Bytes(bytes),
        unwritten: "team_input_unwritten",
    });
    let output = if stderr {
        Output::StderrPrefix(8192)
    } else {
        Output::Stdout {
            bytes: 256 << 10,
            too_large: "team_output_too_large",
        }
    };
    let spec = Spec {
        custody_env: CUSTODY,
        input,
        output,
        codes: CODES,
        retain: None,
    };
    let check = |command: &mut std::process::Command| {
        if stop.load(Ordering::Acquire) || !crate::teams::enabled(&config.state) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        if let Some(original) = &original {
            admission(config, original.plan, original.bound)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::PermissionDenied))?;
        }
        for var in [
            "HGS_SESSION",
            "HGS_SESSION_ID",
            "HGS_RUN_ID",
            "HGS_CONVERSATION_ID",
            "HGS_LAUNCH_ID",
            "HGS_LAUNCH_SESSION_ID",
            "HGS_ACCOUNT_ID",
            "HGS_PARENT",
            "TMUX",
            "TMUX_PANE",
        ] {
            command.env_remove(var);
        }
        command
            .env("HGS_CONFIG_DIR", &config.dir)
            .env("HGS_STATE_DIR", &config.state);
        Ok(())
    };
    match original {
        None => original_child::run_unheld(&program, args, spec, deadline, check),
        Some(ref original) => {
            let Some(held) = custody(config, original.id) else {
                return Ended::NotStarted;
            };
            original_child::run(&program, args, spec, &held, deadline, check)
        }
    }
}
fn custody(config: &Config, id: &str) -> Option<Arc<InheritedLock>> {
    if !uuid::Uuid::parse_str(id).is_ok_and(|u| !u.is_nil() && u.to_string() == id) {
        return None;
    }
    let dir = config.state.join("teams/custody");
    crate::platform::private_dir(&dir).ok()?;
    if !crate::platform::verify_private(&dir, crate::platform::Private::Directory).ok()? {
        return None;
    }
    let path = dir.join(format!("{id}.lock"));
    let file = crate::platform::no_follow(crate::platform::private_open(
        OpenOptions::new().create_new(true).read(true).write(true),
    ))
    .open(path)
    .ok()?;
    if !crate::platform::verify_private_file(&file).ok()? {
        return None;
    }
    file.sync_all().ok()?;
    std::fs::File::open(&dir).ok()?.sync_all().ok()?;
    InheritedLock::try_lock(file).ok()
}

pub(super) fn admission(
    config: &Config,
    plan: &super::ports::Plan,
    bound: bool,
) -> super::ports::Result<()> {
    use super::launch::invalid;
    use zerus_team_plan::model::{MemberState, TeamStatus};
    let store = crate::teams::store::Store::open(&config.state)?;
    let team = store
        .team(&plan.team)?
        .ok_or_else(|| invalid("Team disappeared"))?;
    let member = store
        .member(&plan.team, &plan.member)?
        .ok_or_else(|| invalid("Member disappeared"))?;
    let session = member.session.as_ref().is_some_and(|key| {
        key.machine == plan.machine && key.session_id == format!("launch_id:{}", plan.launch_id)
    });
    if team.status != TeamStatus::Running
        || team.home != plan.machine
        || team.project != plan.project
        || team.owner != plan.person
        || member.role != plan.role
        || member.agent != plan.agent
        || if bound {
            member.state != MemberState::Active || !session
        } else {
            member.state != MemberState::NotCreated || member.session.is_some()
        }
    {
        return Err(invalid("Member launch admission changed"));
    }
    Ok(())
}
