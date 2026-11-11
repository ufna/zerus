//! Who runs a Teams command.
//!
//! A command whose environment names a session is proved exactly as `hgs task`
//! proves it: the run is live on this computer and runs this command. Without
//! those markers the caller is the person only while no live session agent runs
//! above it. That is the interim person rule of the design, and every reply it
//! produces says so.
//!
//! The proof is shared: `hgs team` and task intake read the same caller. It
//! never turns a claim into authority: a marker-carrying ancestor is looked up
//! as one saved record through the same live-binding proof `hgs send` uses, and
//! a caller that cannot be proved is refused rather than treated as the person.
use crate::config::Config;
use serde_json::Value;
use zerus_team_plan::model::SessionKey;
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

/// How a command proved its caller before any operation was chosen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Caller {
    /// No session markers and no live agent above the command.
    Person,
    /// A live run that runs this command.
    Session(SessionKey),
    /// A session was claimed but could not be proved, or the proof could not
    /// be read.
    Refused(Problem),
}

/// The caller of the running command.
pub(crate) fn caller(config: &Config) -> Caller {
    let marked = nonempty("HGS_SESSION").zip(nonempty("HGS_RUN_ID"));
    if let Some((name, run)) = marked {
        return prove(config, &name, &run).map_or_else(
            || {
                Caller::Refused(Problem::new(
                    code::NOT_ALLOWED,
                    "this command names an agent session, but that run is not live on this computer or does not run it; run the command from the agent's own shell, or unset HGS_SESSION and HGS_RUN_ID to act as yourself",
                ))
            },
            Caller::Session,
        );
    }
    match agent_above(config) {
        Ok(Some(name)) => Caller::Refused(Problem::new(
            code::NOT_ALLOWED,
            format!(
                "the agent session {name} runs this command, so it is not a person's terminal; run hgs team from that session's own shell"
            ),
        )),
        Ok(None) => Caller::Person,
        Err(problem) => Caller::Refused(problem),
    }
}

fn nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
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
    if !(session_id.starts_with("launch_id:") || session_id.starts_with("run_id:")) {
        return Err(Problem::new(
            code::NOT_ALLOWED,
            "the calling session has no launch_id or run_id to bind",
        ));
    }
    let machine = crate::swarm::local_node_id(config).map_err(|error| {
        Problem::new(
            code::INVALID_STATE,
            format!("Cannot read this computer's identity: {error}"),
        )
    })?;
    Ok(SessionKey {
        machine,
        session_id,
    })
}

/// The session `name`/`run` while it is live and runs this command.
///
/// `caller_record` owns the proof: the saved record must be live, not archived
/// and name a process this command descends from. Only the marker pair goes
/// into the environment for the length of that read, and the previous values
/// are restored.
fn prove(config: &Config, name: &str, run: &str) -> Option<SessionKey> {
    let previous = (
        std::env::var_os("HGS_SESSION"),
        std::env::var_os("HGS_RUN_ID"),
    );
    std::env::set_var("HGS_SESSION", name);
    std::env::set_var("HGS_RUN_ID", run);
    let record = crate::state::session_graph::caller_record();
    restore("HGS_SESSION", previous.0);
    restore("HGS_RUN_ID", previous.1);
    session_key(config, &record?).ok()
}

fn restore(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

/// The session agent that runs this command, if one does.
///
/// Only this command's own ancestors, at most 64, are read: each one's
/// environment names the session it runs, as its launcher set it. That claim
/// is verified once, through [`prove`], so no saved record is read for a
/// process that carries no claim, and never more than one record per claiming
/// ancestor. An ancestor whose environment cannot be read is skipped: a
/// process of another user never runs this user's agent.
fn agent_above(config: &Config) -> Result<Option<String>, Problem> {
    let chain =
        match ancestors() {
            Walk::Chain(chain) => chain,
            // A computer that cannot relate a process to its parent cannot apply
            // the interim rule; only its marked commands are proved, exactly as
            // `hgs task` proves them.
            Walk::Unsupported => return Ok(None),
            Walk::Unreadable => return Err(Problem::new(
                code::INVALID_STATE,
                "Cannot inspect this command's process ancestry, so it cannot act as the person.",
            )),
        };
    for pid in chain {
        let Some((name, run)) = environment(pid).ok().as_deref().and_then(markers) else {
            continue;
        };
        // A claim needs the state ledger; an unreadable ledger is a refusal,
        // never a silent person.
        if let Err(reason) = crate::state::session_operations() {
            return Err(Problem::new(
                code::INVALID_STATE,
                format!("Cannot read this computer's session ledger: {reason}"),
            ));
        }
        if prove(config, &name, &run).is_some() {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// What the ancestor walk could read.
enum Walk {
    Chain(Vec<u32>),
    /// This computer does not report a process's parent.
    Unsupported,
    /// The command's own process could not be read.
    Unreadable,
}

/// This command's process ancestors, the command itself first, bounded. A
/// replaced parent or a loop ends the walk; an ancestor of another user ends
/// it without failing it.
fn ancestors() -> Walk {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Walk::Unsupported;
    }
    let own = std::process::id();
    let mut chain = vec![own];
    let mut current = own;
    for step in 0..64 {
        let Some(process) = crate::platform::process_identity(current) else {
            // This command's own process is the one read that must work; an
            // ancestor of another user or a replaced parent ends the walk.
            return if step == 0 {
                Walk::Unreadable
            } else {
                Walk::Chain(chain)
            };
        };
        if process.parent == 0 || process.parent == current || chain.contains(&process.parent) {
            break;
        }
        chain.push(process.parent);
        current = process.parent;
    }
    Walk::Chain(chain)
}

/// The session markers in a process environment, which is NUL-separated
/// `KEY=VALUE` entries.
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

/// A computer without a way to read another process's environment cannot see
/// an unmarked session agent above the command; the interim rule then only
/// applies where the computer's own session state proves it.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn environment(_pid: u32) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "this computer does not expose another process's environment",
    ))
}

#[cfg(test)]
mod tests;
