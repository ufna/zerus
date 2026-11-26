//! Narrow Linux fallback for an established ordinary Codex input. The native
//! observations supplement this send only; no saved start or readiness changes.
use super::*;
use crate::platform::{self, FileIdentity, ProcessState};
use crate::state::{
    process_index::{Birth, Process, pins::Pin},
    repositories::directory::Directory,
};
use backend::PaneField::{Dead, PaneId, PanePid, RunMarker, SessionId, SessionName};
use std::io::Read;

#[derive(PartialEq, Eq)]
pub(super) struct Witness {
    pin: Pin,
    conversation: String,
    terminal: String,
    pane: String,
    tmux_session: String,
}

/// Select the exact Codex run without refreshing the presentation projection.
/// The ordinary input gate admits both idle and working. Child rollout reads
/// only refine those two states and are unnecessary for this observation.
pub(super) fn observation_record(name: &str, run: &str, conversation: &str) -> Result<Value> {
    let candidates = labels::candidates_metadata(name)?;
    let first = candidates
        .first
        .then(|| storage::read_metadata_at(&storage::label_record_path(name), name))
        .transpose()?;
    let mut matching = first.into_iter().chain(candidates.by_id).filter(|record| {
        string(record, "run_id") == run && string(record, "conversation_id") == conversation
    });
    let record = matching.next().ok_or("session identity changed; refresh before sending")?;
    if matching.next().is_some() {
        return Err("more than one session has this run and conversation; open Terminal".into());
    }
    Ok(record)
}

/// Compare the binding from the regular input gate to a fresh, bounded read
/// of its exact owner-controlled slot. Neither a public row nor a cache pin
/// can supply the fallback. No index is repaired and no file is created here.
fn private_binding(record: &Value) -> Option<()> {
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
    if !established(&saved) {
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
    .then_some(())
}

fn established(record: &Value) -> bool {
    record["agent"] == "codex"
        && record["run_identity_version"] == 1
        && record["rename_shadow"] != true
        && record["state"] != "archived"
        && !string(record, "conversation_id").is_empty()
        && string(record, "expected_id").is_empty()
        && string(record, "archive_id").is_empty()
        && string(record, "error").is_empty()
        && record["last_event_at"].as_f64().is_some_and(|at| at > 0.0)
        && ["paused", "starting", "clearing", "resuming"]
            .iter()
            .all(|key| record[*key].is_null() || record[*key] == false)
        && [
            "pausing",
            "terminating",
            "resume_pending",
            "restore_archive_id",
            "restore_archive_cleanup",
            "session_end",
            "exited_at",
            "pending_settings",
        ]
        .iter()
        .all(|key| record[*key].is_null())
        && !resume_input::pending(record)
        && !resume_input::fork_pending(record)
        && !codex_tasks::multiple(record)
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

impl Witness {
    pub(super) fn read(record: &Value) -> Option<Self> {
        if !established(record) {
            return None;
        }
        let pin = process_index_writer::original_pin(record)?;
        private_binding(record)?;
        let pane = string(record, "pane");
        let terminal = terminal_name(record);
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
            || live_birth(&pin.child)?.parent != pin.supervisor.pid
        {
            return None;
        }
        live_birth(&pin.supervisor)?;
        Some(Self {
            pin,
            conversation: string(record, "conversation_id").into(),
            terminal: terminal.into(),
            pane: pane.into(),
            tmux_session: tmux_session.into(),
        })
    }
}
