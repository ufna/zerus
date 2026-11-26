//! Read-only Linux proof of the exact original Codex launch and terminal.
use super::*;
use crate::platform::{self, FileIdentity, ProcessState};
use crate::state::{
    process_index::{Birth, Process, pins::Pin},
    repositories::directory::Directory,
};
use backend::PaneField::{Dead, PaneId, PanePid, RunMarker, SessionId, SessionName};
use std::fs;
use std::io::Read;
use std::time::{Duration, Instant};
use zerus_client_core::desktop::verified_send::{Readiness, VERSION};

#[derive(PartialEq, Eq)]
pub(super) struct Witness {
    pin: Pin,
    conversation: String,
    terminal: String,
    pane: String,
    tmux_session: String,
}

pub(super) fn catalog_deadline() -> Instant {
    Instant::now() + Duration::from_millis(250)
}

/// A compact row chooses only the metadata to corroborate. Its pins and
/// policy fields never substitute for the current owner-private record.
pub(super) fn catalog_readiness(hint: &Value, until: Instant) -> Option<Readiness> {
    if Instant::now() >= until || !established_identity(hint) {
        return None;
    }
    let saved = storage::read_metadata_at(&storage::own_path(hint).ok()?, string(hint, "name")).ok()?;
    if ["session_id", "run_id", "conversation_id", "pid", "pane", "terminal_name"]
        .iter().any(|key| hint[*key] != saved[*key]) || Instant::now() >= until
    {
        return None;
    }
    let (_, canonical) = Witness::read_canonical(&saved, |_| true)?;
    if Instant::now() >= until {
        return None;
    }
    let send = input::native_readiness::established(&canonical);
    Some(Readiness {
        version: VERSION,
        session_id: string(&saved, "session_id").into(),
        run_id: string(&saved, "run_id").into(),
        conversation_id: string(&saved, "conversation_id").into(),
        send,
        pause: false,
        terminate: false,
        stop: false,
    })
}

/// Compare the binding from the regular input gate to a fresh, bounded read
/// of its exact owner-controlled slot. Neither a public row nor a cache pin
/// can supply the fallback. No index is repaired and no file is created here.
fn private_binding(record: &Value) -> Option<Value> {
    let name = string(record, "name");
    let path = storage::own_path(record).ok()?;
    let directory = Directory::open(path.parent()?).ok()?;
    if !directory.owner_controls_entries().ok()? {
        return None;
    }
    let leaf = path.file_name()?.to_str()?;
    let mut file = directory.read_private(leaf).ok()?;
    let version = FileIdentity::of(&file).ok()?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(storage::ID_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > storage::ID_RECORD_BYTES
        || version != FileIdentity::of(&file).ok()?
        || version != FileIdentity::of(&directory.read_private(leaf).ok()?).ok()?
    {
        return None;
    }
    directory.verify().ok()?;
    let saved = storage::verify_metadata_at(&path, name, &bytes).ok()?;
    session_id::verify(&saved).ok()?;
    if !established_identity(&saved) {
        return None;
    }
    [
        "session_id",
        "run_id",
        "conversation_id",
        "agent",
        "pane",
        "terminal_name",
        "pid",
        "process_start",
        "supervisor",
        crate::state::process_index::pins::FIELD,
    ]
    .iter()
    .all(|key| saved[*key] == record[*key])
    .then_some(saved)
}

fn established_identity(record: &Value) -> bool {
    record["agent"] == "codex"
        && record["run_identity_version"] == 1
        && record["rename_shadow"] != true
        && record["state"] != "archived"
        && !string(record, "conversation_id").is_empty()
        && string(record, "archive_id").is_empty()
}

/// Both originals must be running, local to our PID/user namespaces, and
/// owned by our effective UID. process_identity also verifies the child still
/// belongs to its original supervisor.
fn live_birth(process: &Process) -> Option<platform::ProcessIdentity> {
    let Birth::Linux(ticks) = process.birth else {
        return None;
    };
    i32::try_from(process.pid).ok()?;
    for namespace in ["pid", "user"] {
        if fs::read_link(format!("/proc/{}/ns/{namespace}", process.pid)).ok()?
            != fs::read_link(format!("/proc/self/ns/{namespace}")).ok()?
        {
            return None;
        }
    }
    let birth = ticks.to_string();
    let identity = platform::process_identity(process.pid)?;
    (identity.birth == birth
        && platform::process_state(process.pid, Some(&birth)).ok()? == ProcessState::Running)
        .then_some(identity)
}

fn terminal_identity(pin: &Pin, pane: &str, terminal: &str) -> Option<String> {
    let backend = backend::current();
    let info = backend
        .pane_info(
            backend::Target::Pane(pane),
            &[PaneId, SessionId, SessionName, RunMarker, PanePid, Dead],
        )
        .ok()?;
    let fields: [&str; 6] = info.fields().try_into().ok()?;
    let [
        actual_pane,
        tmux_session,
        actual_terminal,
        run,
        supervisor,
        "0",
    ] = fields
    else {
        return None;
    };
    if actual_pane != pane
        || actual_terminal != terminal
        || run != pin.run
        || supervisor.parse::<u32>().ok()? != pin.supervisor.pid
        || !tmux_session
            .strip_prefix('$')
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
        || backend
            .session_variable(terminal, "HGS_SESSION_ID")
            .ok()??
            != pin.session
    {
        return None;
    }
    Some(tmux_session.to_owned())
}

impl Witness {
    /// Read immutable launch identity. This grants no input or lifecycle authority.
    #[allow(dead_code)] // Shared by the independently qualified lifecycle paths.
    pub(super) fn read(record: &Value) -> Option<Self> {
        Self::read_checked(record, |_| true)
    }

    pub(super) fn originals(&self) -> (&Process, &Process) {
        (&self.pin.child, &self.pin.supervisor)
    }

    #[allow(dead_code)] // Shared by the independently qualified lifecycle paths.
    pub(super) fn terminal_session_id(&self) -> &str {
        &self.tmux_session
    }

    /// Recheck the same terminal without assuming either process still runs.
    #[allow(dead_code)] // Shared by the independently qualified lifecycle paths.
    pub(super) fn terminal_is_current(&self) -> bool {
        terminal_identity(&self.pin, &self.pane, &self.terminal).as_deref()
            == Some(self.tmux_session.as_str())
    }

    /// Apply an operation's own predicate to the supplied and fresh canonical records.
    pub(super) fn read_checked(record: &Value, admits: impl Fn(&Value) -> bool) -> Option<Self> {
        Self::read_canonical(record, admits).map(|(witness, _)| witness)
    }

    fn read_canonical(record: &Value, admits: impl Fn(&Value) -> bool) -> Option<(Self, Value)> {
        if !established_identity(record) || !admits(record) {
            return None;
        }
        let pin = process_index_writer::original_pin(record)?;
        let canonical = private_binding(record)?;
        if !admits(&canonical) {
            return None;
        }
        let pane = string(record, "pane");
        let terminal = terminal_name(record);
        let tmux_session = terminal_identity(&pin, pane, terminal)?;
        let witness = Self {
            pin,
            conversation: string(record, "conversation_id").into(),
            terminal: terminal.into(),
            pane: pane.into(),
            tmux_session,
        };
        let (child, supervisor) = witness.originals();
        if live_birth(child)?.parent != supervisor.pid {
            return None;
        }
        live_birth(supervisor)?;
        Some((witness, canonical))
    }
}
