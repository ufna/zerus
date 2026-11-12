//! The resident agent hosts that serve several sessions from one long-lived
//! process, read for the caller proof. No session's own agent is an ancestor
//! of a hosted session's shell; only the host is. These reads are bounded,
//! follow no link and repair nothing.
//!
//! DeepSeek hosts are this computer's own records: one folder per agent home
//! under `dsh/hosts`, at most [`HOST_FOLDERS`] of them, each naming the host's
//! process ID and its birth as `platform::process_identity` reads it. A
//! starting record and an older `host.pid` name a process ID without a birth.
//! A link, a file where a folder belongs, an unreadable or malformed record
//! and more folders than the bound are refusals.
//!
//! OpenCode's shared service is registered only in OpenCode's own service
//! file, which holds a process ID and no birth. It is someone else's file, read
//! as untrusted data: it can only add a refusal, and a missing, unreadable or
//! malformed file adds nothing. An OpenCode service that runs without being
//! registered there is therefore not detected.
#[cfg(unix)]
use super::caller_binding::optional_child;
use super::*;
use crate::state::provider_binding::Provider;
#[cfg(unix)]
use crate::state::{repositories::directory::Directory, storage};

/// At most this many DeepSeek host folders, one per agent home.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) const HOST_FOLDERS: usize = 64;
const UNREADABLE: &str = "Cannot verify a recorded agent host.";

/// A process that a host record names, with the birth it recorded, or none
/// when the record holds only a process ID.
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

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Hosts {
    pub(crate) deepseek: Vec<Named>,
    pub(crate) opencode: Option<Registered>,
}

#[cfg(test)]
thread_local! { static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(super) fn reads() -> usize {
    READS.with(|n| n.get())
}

pub(super) fn hosts() -> std::result::Result<Hosts, &'static str> {
    Ok(Hosts {
        deepseek: deepseek()?,
        opencode: opencode(),
    })
}

#[cfg(unix)]
fn deepseek() -> std::result::Result<Vec<Named>, &'static str> {
    let state_root = std::fs::canonicalize(storage::root()).map_err(|_| UNREADABLE)?;
    let root = Directory::open(&state_root).map_err(|_| UNREADABLE)?;
    let Some(dsh) = optional_child(&root, "dsh")? else {
        return Ok(Vec::new());
    };
    let Some(hosts) = optional_child(&dsh, "hosts")? else {
        return Ok(Vec::new());
    };
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(&hosts.path)
        .map_err(|_| UNREADABLE)?
        .take(HOST_FOLDERS + 1)
    {
        let name = entry.map_err(|_| UNREADABLE)?.file_name();
        folders.push(name.into_string().map_err(|_| UNREADABLE)?);
    }
    if folders.len() > HOST_FOLDERS {
        return Err(UNREADABLE);
    }
    hosts.verify().map_err(|_| UNREADABLE)?;
    let mut named = Vec::new();
    for folder in folders {
        named.extend(host(&hosts.child(&folder).map_err(|_| UNREADABLE)?)?);
    }
    hosts.verify().map_err(|_| UNREADABLE)?;
    Ok(named)
}

#[cfg(not(unix))]
fn deepseek() -> std::result::Result<Vec<Named>, &'static str> {
    Err(UNREADABLE)
}

/// What one host folder names: a running host with its birth; a starting
/// record's previous host with its birth and its PID file without one; an
/// older `host.pid` without one, unless it is the running host's own.
#[cfg(unix)]
fn host(folder: &Directory) -> std::result::Result<Vec<Named>, &'static str> {
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
    let mut running = None;
    if let Some(bytes) = identity {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
        match value["state"].as_str() {
            Some("running") => {
                let host = recorded(&value)?;
                running = Some(host.pid);
                named.push(host);
            }
            Some("starting") => {
                if !value["previous"].is_null() {
                    named.push(recorded(&value["previous"])?);
                }
                if !value["pid_file"].is_null() {
                    let pid = value["pid_file"][0]
                        .as_u64()
                        .and_then(|pid| u32::try_from(pid).ok())
                        .filter(|pid| valid(*pid))
                        .ok_or(UNREADABLE)?;
                    named.push(Named { pid, birth: None });
                }
            }
            _ => return Err(UNREADABLE),
        }
    }
    if let Some(pid) = legacy.filter(|pid| Some(*pid) != running) {
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
fn opencode() -> Option<Registered> {
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
