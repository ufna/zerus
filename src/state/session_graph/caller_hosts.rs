//! The resident agent hosts that serve several sessions from one long-lived
//! process, read for the caller proof. No session's own agent is an ancestor
//! of a hosted session's shell; only the host is. These reads are bounded,
//! follow no link and repair nothing.
//!
//! DeepSeek hosts are this computer's own records: one folder per agent home
//! under `dsh/hosts`, each naming the host's process ID and its birth as
//! `platform::process_identity` reads it. A state folder, `dsh` or `hosts`
//! that does not exist records no host. Only directories are host folders: a
//! stray file such as `.DS_Store`, or a link, is not one. More than
//! [`HOST_FOLDERS`] directories, or more than [`HOST_ENTRIES`] entries of any
//! kind, refuse with their own sentence, since these folders are kept per
//! agent home and nothing removes them. A folder or record that exists but
//! cannot be read, or is malformed, refuses.
//!
//! A running record names its host with its birth. A starting record names
//! the previous host with its birth and a PID file without one; its launcher
//! is the `hgs` that started the attempt, not a host. An older `host.pid`
//! names a process ID without a birth. A record that carries the identity of
//! another boot names no process of this boot. A host spawned by an attempt
//! that ended unconfirmed, before its running record, is named nowhere and
//! passes as a person: the stated limit.
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
/// At most this many entries of any kind are listed in `dsh/hosts`, so that
/// stray files cannot make the listing unbounded.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) const HOST_ENTRIES: usize = 1024;
#[cfg(unix)]
const UNREADABLE: &str = "Cannot verify a recorded agent host.";

/// Why the recorded hosts were not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unread {
    /// A folder or record that exists cannot be read, or is malformed.
    Unreadable,
    /// More host folders, or entries, than the bound.
    #[cfg_attr(not(unix), allow(dead_code))]
    TooMany,
}

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

pub(super) fn hosts() -> std::result::Result<Hosts, Unread> {
    Ok(Hosts {
        deepseek: deepseek()?,
        opencode: opencode(),
    })
}

#[cfg(unix)]
fn deepseek() -> std::result::Result<Vec<Named>, Unread> {
    let state_root = match std::fs::canonicalize(storage::root()) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(Unread::Unreadable),
    };
    let root = Directory::open(&state_root).map_err(|_| Unread::Unreadable)?;
    let Some(dsh) = optional_child(&root, "dsh").map_err(|_| Unread::Unreadable)? else {
        return Ok(Vec::new());
    };
    let Some(hosts) = optional_child(&dsh, "hosts").map_err(|_| Unread::Unreadable)? else {
        return Ok(Vec::new());
    };
    let mut folders = Vec::new();
    for (count, entry) in std::fs::read_dir(&hosts.path)
        .map_err(|_| Unread::Unreadable)?
        .enumerate()
    {
        if count == HOST_ENTRIES {
            return Err(Unread::TooMany);
        }
        let entry = entry.map_err(|_| Unread::Unreadable)?;
        if !entry.file_type().map_err(|_| Unread::Unreadable)?.is_dir() {
            continue;
        }
        if folders.len() == HOST_FOLDERS {
            return Err(Unread::TooMany);
        }
        let name = entry.file_name();
        folders.push(name.into_string().map_err(|_| Unread::Unreadable)?);
    }
    hosts.verify().map_err(|_| Unread::Unreadable)?;
    let boot = boot();
    let mut named = Vec::new();
    for folder in folders {
        let folder = hosts.child(&folder).map_err(|_| Unread::Unreadable)?;
        named.extend(host(&folder, boot.as_deref()).map_err(|_| Unread::Unreadable)?);
    }
    hosts.verify().map_err(|_| Unread::Unreadable)?;
    Ok(named)
}

#[cfg(not(unix))]
fn deepseek() -> std::result::Result<Vec<Named>, Unread> {
    Err(Unread::Unreadable)
}

/// This boot's identity, read as `dsh/host_identity.rs` records it; none when
/// it cannot be read, and then no record is set aside for its boot.
#[cfg(target_os = "linux")]
fn boot() -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open("/proc/sys/kernel/random/boot_id")
        .ok()?
        .take(129)
        .read_to_string(&mut text)
        .ok()?;
    canonical_boot(text.trim())
}

#[cfg(target_os = "macos")]
fn boot() -> Option<String> {
    let mut bytes = [0u8; 129];
    let mut size = bytes.len();
    // SAFETY: the name is NUL-terminated and the buffer is valid for `size`.
    let code = unsafe {
        libc::sysctlbyname(
            c"kern.bootsessionuuid".as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if code != 0 || !(1..=128).contains(&size) {
        return None;
    }
    canonical_boot(
        std::str::from_utf8(&bytes[..size])
            .ok()?
            .trim_end_matches('\0'),
    )
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn boot() -> Option<String> {
    None
}

#[cfg(unix)]
fn canonical_boot(text: &str) -> Option<String> {
    uuid::Uuid::parse_str(text)
        .ok()
        .filter(|id| !id.is_nil())
        .map(|id| id.to_string())
}

/// This boot's identity for the tests' records.
#[cfg(all(test, unix))]
pub(super) fn current_boot() -> Option<String> {
    boot()
}

/// What one host folder names: a running host with its birth; a starting
/// record's previous host with its birth and its PID file without one; an
/// older `host.pid` without one, unless it is the running host's own. A record
/// that carries another boot's identity than `boot` names nothing.
#[cfg(unix)]
fn host(folder: &Directory, boot: Option<&str>) -> std::result::Result<Vec<Named>, &'static str> {
    let other_boot = |record: &Value| {
        boot.is_some_and(|now| {
            record["boot"]
                .as_str()
                .and_then(canonical_boot)
                .is_some_and(|then| then != now)
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
    let mut running = None;
    if let Some(bytes) = identity {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
        match value["state"].as_str() {
            Some("running") => {
                let host = recorded(&value)?;
                running = Some(host.pid);
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
