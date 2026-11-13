//! Per-home DeepSeek records for the hosted-call token proof, and OpenCode's
//! refusal-only service registration. The global DeepSeek ancestor check is
//! `state::dsh::hosted_ancestor`; Teams and the main caller paths share it.
//!
//! A hosted call selects only its binding's agent home. Its record names a
//! PID and birth for this boot; an older PID file without a birth proves
//! no hosted call. Reads are bounded, follow no link and repair nothing.
//! OpenCode's service file is untrusted: it can only add a refusal, and a
//! missing, unreadable or malformed registration proves nothing.
#[cfg(unix)]
use super::caller_binding::optional_child;
use super::*;
use crate::state::provider_binding::Provider;
#[cfg(unix)]
use crate::state::{repositories::directory::Directory, storage};

#[cfg(unix)]
const UNREADABLE: &str = "Cannot verify a recorded agent host.";

/// A process that a host record names, with the birth it recorded, or none
/// when the record holds only a process ID.
#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Named {
    pub(crate) pid: u32,
    pub(crate) birth: Option<String>,
}

/// The process OpenCode's service file registers, and when that file was last
/// written, in Unix seconds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Registered {
    pub(crate) pid: u32,
    pub(crate) written: f64,
}

#[cfg(test)]
thread_local! { static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(super) fn reads() -> usize {
    READS.with(|n| n.get())
}

/// What the host folder of one agent home names, by the same record rules
/// and reads as the hosted proof (zerus-ppd26 Part B): the hosted caller proof reads
/// only the host its session's binding selects. A state folder, `dsh`,
/// `hosts` or host folder that does not exist names no host; one that exists
/// but cannot be read refuses.
#[cfg(unix)]
pub(super) fn deepseek_home(agent_home: &str) -> std::result::Result<Vec<Named>, &'static str> {
    let state_root = match std::fs::canonicalize(storage::root()) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(UNREADABLE),
    };
    let root = Directory::open(&state_root).map_err(|_| UNREADABLE)?;
    let Some(dsh) = optional_child(&root, "dsh").map_err(|_| UNREADABLE)? else {
        return Ok(Vec::new());
    };
    let Some(hosts) = optional_child(&dsh, "hosts").map_err(|_| UNREADABLE)? else {
        return Ok(Vec::new());
    };
    let key = crate::state::dsh::host_key(agent_home);
    let Some(folder) = optional_child(&hosts, &key).map_err(|_| UNREADABLE)? else {
        return Ok(Vec::new());
    };
    host(&folder, boot().as_deref())
}

/// This boot's identity through the very read that stamps the records,
/// `dsh/host_identity.rs`'s own, so the reader and the writer cannot drift
/// apart. A read that fails sets nothing aside.
#[cfg(unix)]
fn boot() -> Option<String> {
    crate::state::dsh::host_identity::boot().ok()
}

/// Whether a record's boot is in the one form the writer stamps: a canonical,
/// lowercase, hyphenated UUID that is not nil. Any other form is an unknown
/// boot, and the record stays named.
#[cfg(unix)]
fn written_form(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}

/// This boot's identity as the writer reads it, for the tests' records.
#[cfg(all(test, unix))]
pub(super) fn written_boot() -> Option<String> {
    crate::state::dsh::host_identity::boot().ok()
}

/// What one host folder names, for the tests of the record rules.
#[cfg(all(test, unix))]
pub(super) fn named_in(
    folder: &Path,
    boot: Option<&str>,
) -> std::result::Result<Vec<Named>, &'static str> {
    let folder = std::fs::canonicalize(folder).map_err(|_| UNREADABLE)?;
    host(&Directory::open(&folder).map_err(|_| UNREADABLE)?, boot)
}

/// What one host folder names: a running host with its birth; a starting
/// record's previous host with its birth and its PID file without one; an
/// older `host.pid` without one, unless the record publishes that same ID
/// (the running host's, or the starting record's PID file). A record that
/// carries another boot's identity than `boot` names nothing, and so neither
/// does the `host.pid` it publishes.
#[cfg(unix)]
fn host(folder: &Directory, boot: Option<&str>) -> std::result::Result<Vec<Named>, &'static str> {
    let other_boot = |record: &Value| {
        boot.is_some_and(|now| {
            record["boot"]
                .as_str()
                .is_some_and(|then| written_form(then) && then != now)
        })
    };
    let identity = small(folder, "host.identity.json", 1024)?;
    let legacy = small(folder, "host.pid", 32)?
        .map(|bytes| {
            std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
                .filter(|pid| valid(*pid))
                .ok_or(UNREADABLE)
        })
        .transpose()?;
    let mut named = Vec::new();
    let mut published = None;
    if let Some(bytes) = identity {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
        match value["state"].as_str() {
            Some("running") => {
                let host = recorded(&value)?;
                published = Some(host.pid);
                if !other_boot(&value) {
                    named.push(host);
                }
            }
            Some("starting") => {
                let previous = (!value["previous"].is_null())
                    .then(|| recorded(&value["previous"]))
                    .transpose()?;
                let pid_file = (!value["pid_file"].is_null())
                    .then(|| {
                        value["pid_file"][0]
                            .as_u64()
                            .and_then(|pid| u32::try_from(pid).ok())
                            .filter(|pid| valid(*pid))
                            .ok_or(UNREADABLE)
                    })
                    .transpose()?;
                published = pid_file;
                if !other_boot(&value) {
                    if let Some(previous) = previous.filter(|_| !other_boot(&value["previous"])) {
                        named.push(previous);
                    }
                    if let Some(pid) = pid_file {
                        named.push(Named { pid, birth: None });
                    }
                }
            }
            _ => return Err(UNREADABLE),
        }
    }
    if let Some(pid) = legacy.filter(|pid| Some(*pid) != published) {
        named.push(Named { pid, birth: None });
    }
    Ok(named)
}

#[cfg(unix)]
fn recorded(value: &Value) -> std::result::Result<Named, &'static str> {
    let pid = value["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| valid(*pid))
        .ok_or(UNREADABLE)?;
    let birth = value["birth"]
        .as_str()
        .filter(|birth| !birth.is_empty() && birth.len() <= 64)
        .ok_or(UNREADABLE)?;
    Ok(Named {
        pid,
        birth: Some(birth.to_owned()),
    })
}

fn valid(pid: u32) -> bool {
    pid > 0 && pid <= i32::MAX as u32
}

/// A small regular file of a pinned folder, or none when it is absent.
#[cfg(unix)]
fn small(
    folder: &Directory,
    name: &str,
    limit: usize,
) -> std::result::Result<Option<Vec<u8>>, &'static str> {
    folder.verify().map_err(|_| UNREADABLE)?;
    match std::fs::symlink_metadata(folder.path.join(name)) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UNREADABLE),
    }
    let bytes = folder.read_file(name, limit).map_err(|_| UNREADABLE)?;
    #[cfg(test)]
    READS.with(|n| n.set(n.get() + 1));
    folder.verify().map_err(|_| UNREADABLE)?;
    Ok(Some(bytes))
}

/// OpenCode's own service file, where `service::registration_path` looks
/// for it. Untrusted: any failure names nothing.
pub(super) fn opencode() -> Option<Registered> {
    use std::io::Read;
    let path = nonempty_env("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("opencode/service.json");
    let file = crate::platform::no_follow(std::fs::OpenOptions::new().read(true))
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 16384 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 16384 {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let pid = value["pid"]
        .as_u64()
        .filter(|pid| *pid > 1)
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| valid(*pid))?;
    let written = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    Some(Registered { pid, written })
}

/// The backend of the binding a marked claim names, when the claim routes to
/// a session that a resident host serves (`dsh/...` or `opencode/...`). The
/// binding is read by its exact route, label or ID, and counts only when its
/// name or launch label and its run are the claim's.
pub(super) fn hosted_backend(name: &str, run: &str, session: Option<&str>) -> Option<String> {
    let provider = Provider::named(name)?;
    let record = match session {
        Some(id) => provider.read_id(id).ok()?,
        None => provider.read_label(name).ok()?,
    };
    let named = record["name"] == name || record["launch_label"] == name;
    (named && record["run_id"] == run).then(|| string(&record, "backend").to_owned())
}
