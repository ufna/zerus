//! Interim caller proof shared by Teams and task intake. A marked caller
//! must descend from its live saved agent. An unmarked caller is the person
//! only after a complete, bounded inspection finds no verified marked agent.
//!
//! This is not verified Person authority: a child reparented away from its
//! agent, or an agent started without session markers, is outside this rule.
//! Discovery uses ancestor environments, never a saved-session inventory.
//! The top is established from process-table owners and parentage, never a
//! program name. Its environment may be unreadable; every environment below
//! it must be inspected. An unreadable helper in the middle (for example a
//! sandbox launcher) still refuses: use an ordinary terminal in that case.
//! With a session-ID hint, ids/<ID>.json selects one verified record.
//! Without an index entry, fixed ID-layout and legacy-label paths are both
//! probed; disagreement refuses. Without a hint only the exact legacy name
//! route is used. One rename shadow is allowed; launch_label proves the old
//! name of a renamed ID record. No missing route is an absence proof: when no
//! record can be reached, or any identity contradicts the claim, admission
//! refuses. Only an established stale/archived/nonmatching process continues
//! the walk. No inventory scan, repair or compatibility fallback is used.
//! The walk is capped at 64 processes; environments and bindings at 1 MiB,
//! index entries at 4 KiB. No process environment is changed.
mod ancestry;
use ancestry::ancestors;

use crate::config::Config;
use serde_json::Value;
use zerus_team_plan::model::SessionKey;
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

pub(crate) const INTERIM_NOTE: &str = "This interim rule does not detect a child reparented away from its agent or an agent started without session markers.";

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Caller {
    Person,
    Session(SessionKey),
    Refused(Problem),
}

pub(crate) fn caller(config: &Config) -> Caller {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Caller::Refused(Problem::new(code::NOT_ALLOWED,
            "The Teams preview is not available on this platform yet: process ancestry cannot be verified."));
    }
    let session = std::env::var_os("HGS_SESSION");
    let run = std::env::var_os("HGS_RUN_ID");
    let hint = std::env::var_os("HGS_SESSION_ID");
    let marked = session.is_some() || run.is_some() || hint.is_some();
    if marked {
        let pair = session
            .as_ref()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .zip(
                run.as_ref()
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.is_empty()),
            );
        let Some((name, run)) = pair else {
            return Caller::Refused(not_allowed());
        };
        let id = match hint.as_ref() {
            Some(value) => match value.to_str().filter(|s| !s.is_empty()) {
                Some(id) => Some(id),
                None => return Caller::Refused(not_allowed()),
            },
            None => None,
        };
        return match prove(config, name, run, id) {
            Ok(Some(key)) => Caller::Session(key),
            Ok(None) => Caller::Refused(not_allowed()),
            Err(problem) => Caller::Refused(problem),
        };
    }
    match agent_above() {
        Ok(true) => Caller::Refused(not_allowed()),
        Ok(false) => Caller::Person,
        Err(problem) => Caller::Refused(problem),
    }
}

fn not_allowed() -> Problem {
    Problem::new(code::NOT_ALLOWED,
        "This command cannot act as the person or prove its claimed live session. Use the agent's own shell for session commands, or a terminal outside any session for person commands.")
}

fn unreadable() -> Problem {
    Problem::new(code::INVALID_STATE,
        "Cannot completely inspect this command's ancestry or saved session binding. An unreadable helper below the top of your user process tree prevents this interim check. Run the command from an ordinary terminal.")
}

/// A saved binding's immutable identity: its launch ID, else its run ID.
pub(crate) fn identity(record: &Value) -> String {
    for key in ["launch_id", "run_id"] {
        if let Some(id) = record[key].as_str().filter(|id| !id.is_empty()) {
            return format!("{key}:{id}");
        }
    }
    String::new()
}

fn session_key(config: &Config, record: &Value) -> Result<SessionKey, Problem> {
    let session_id = identity(record);
    if session_id.is_empty() {
        return Err(unreadable());
    }
    let machine = crate::state::session_graph::caller_machine(config).map_err(|_| unreadable())?;
    Ok(SessionKey {
        machine,
        session_id,
    })
}

fn prove(
    config: &Config,
    name: &str,
    run: &str,
    session: Option<&str>,
) -> Result<Option<SessionKey>, Problem> {
    prove_read(
        config,
        name,
        run,
        session,
        crate::state::session_graph::caller_record_for,
    )
}

fn prove_read(
    config: &Config,
    name: &str,
    run: &str,
    session: Option<&str>,
    read: impl FnOnce(&str, &str, Option<&str>) -> std::result::Result<Option<Value>, &'static str>,
) -> Result<Option<SessionKey>, Problem> {
    read(name, run, session)
        .map_err(|_| unreadable())?
        .map(|record| session_key(config, &record))
        .transpose()
}

fn agent_above() -> Result<bool, Problem> {
    let chain = ancestors()?;
    for ancestor in &chain {
        if let Some(claim) = &ancestor.claim {
            if crate::state::session_graph::caller_agent_for(
                &claim.name,
                &claim.run,
                claim.session.as_deref(),
                ancestor.pid,
                &ancestor.identity.birth,
            )
            .map_err(|_| unreadable())?
            .is_some()
            {
                return Ok(true);
            }
        }
    }
    // Recheck original edges and births before accepting a negative proof.
    ancestry::unchanged(&chain)?;
    Ok(false)
}

/// Claims are only discovery hints. The explicit saved-record proof establishes
/// ancestry and liveness; neither environment labels nor a missing file do.
fn markers(bytes: &[u8]) -> Option<(String, String)> {
    let mut session = None;
    let mut run = None;
    for entry in bytes.split(|byte| *byte == 0) {
        if let Some(value) = entry.strip_prefix(b"HGS_SESSION=") {
            session = text(value);
        } else if let Some(value) = entry.strip_prefix(b"HGS_RUN_ID=") {
            run = text(value);
        }
    }
    session.zip(run)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Claim {
    name: String,
    run: String,
    session: Option<String>,
}

fn claim(bytes: &[u8]) -> Result<Option<Claim>, Problem> {
    let has_marker = bytes.split(|byte| *byte == 0).any(|entry| {
        [
            b"HGS_SESSION=".as_slice(),
            b"HGS_RUN_ID=",
            b"HGS_SESSION_ID=",
        ]
        .iter()
        .any(|prefix| entry.starts_with(prefix))
    });
    match markers(bytes) {
        Some((name, run)) => {
            let session = bytes
                .split(|byte| *byte == 0)
                .find_map(|entry| entry.strip_prefix(b"HGS_SESSION_ID="))
                .map(|value| text(value).ok_or_else(unreadable))
                .transpose()?;
            Ok(Some(Claim { name, run, session }))
        }
        None if has_marker => Err(unreadable()),
        None => Ok(None),
    }
}

fn text(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.is_empty()).then(|| text.to_owned())
}

/// One process's environment, bounded. The agent a caller descends from runs
/// as this user, so only this user's processes are read.
#[cfg(target_os = "linux")]
fn environment(pid: u32) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(format!("/proc/{pid}/environ"))?
        .take((1 << 20) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1 << 20 {
        return Err(std::io::Error::other("environment is too large"));
    }
    Ok(bytes)
}

/// The same read on macOS, through `KERN_PROCARGS2`, whose answer holds the
/// argument count, then the executable, the arguments and finally the
/// environment.
#[cfg(target_os = "macos")]
fn environment(pid: u32) -> std::io::Result<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut bytes = vec![0u8; 1 << 20];
    let mut size = bytes.len();
    // SAFETY: the buffer and the size are valid for the call.
    let code = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    let invalid = || std::io::Error::other("environment is unavailable");
    if code != 0 || !(4..=bytes.len()).contains(&size) {
        return Err(invalid());
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes[..4].try_into().map_err(|_| invalid())?);
    if !(1..=4096).contains(&argc) {
        return Err(invalid());
    }
    let mut rest = &bytes[4..];
    rest = &rest[rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(invalid)?
        + 1..];
    while rest.first() == Some(&0) {
        rest = &rest[1..];
    }
    for _ in 0..argc {
        rest = &rest[rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(invalid)?
            + 1..];
    }
    Ok(rest.to_vec())
}

/// Unsupported platforms refuse before this read is reachable. This fallback
/// stays an error; it can never establish an unmarked person's authority.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn environment(_pid: u32) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "this computer does not expose another process's environment",
    ))
}

#[cfg(test)]
mod tests;
