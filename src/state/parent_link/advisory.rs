//! One current child policy refusal on the verified local parent's read path.
use super::*;
use crate::platform::{self, FileIdentity};
use crate::state::codex_tasks::history::HistoryRoot;
use std::time::{Duration, Instant};
use zerus_client_core::input_acknowledgment::canonical;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(in crate::state) mod activity;

pub(super) fn observe(config: &crate::config::Config, record: &Value) -> Option<Value> {
    let parent = listed(record)?;
    let after = floor(record, &parent)?;
    let machine = session_graph::caller_machine(config).ok()?;
    if parent["machine"] != machine || !caller_matches(&parent) {
        return None;
    }
    if !live(record) {
        return None;
    }
    let history = History::open(record)?;
    let event = provider_errors::codex_policy_notice(record, &history.file)?;
    let at = event["at"].as_f64().filter(|at| *at >= after)?;
    if !history.held_by_child() {
        return None;
    }
    let observation = Observation { history, event, at };

    let until = Instant::now() + Duration::from_millis(100);
    let _guard = storage::existing_lock_until(&root().join(".lock"), until).ok()??;
    if !observation.current(record)
        || waits::completed_result_since(&parent, record, observation.at) != Some(false)
        || !caller_matches(&parent)
    {
        return None;
    }
    Some(json!({"scope":"session","child":{
        "session_id":record["session_id"],"run_id":record["run_id"],
        "conversation_id":record["conversation_id"]},"event":observation.event}))
}

struct Observation {
    history: History,
    event: Value,
    at: f64,
}

impl Observation {
    /// Called under the state writer. A changed binding or transcript voids
    /// the observation for a later admission.
    fn current(&self, record: &Value) -> bool {
        let current = (|| {
            let path = storage::own_path(record).ok()?;
            let bytes = storage::binding_bytes(&path).ok()?;
            storage::verify_metadata_at(&path, string(record, "name"), &bytes).ok()
        })();
        current.is_some_and(|current| {
            same_observation(record, &current) && live(&current) && self.history.current()
        })
    }
}

fn caller_matches(parent: &Value) -> bool {
    let id = string(parent, "session_id");
    let run = string(parent, "run_id");
    let Some(name) = nonempty_env("HGS_SESSION") else {
        return false;
    };
    if nonempty_env("HGS_SESSION_ID").as_deref() != Some(id)
        || nonempty_env("HGS_RUN_ID").as_deref() != Some(run)
    {
        return false;
    }
    session_graph::caller_record_for(&name, run, Some(id))
        .ok()
        .flatten()
        .is_some_and(|caller| {
            caller["session_id"] == id
                && caller["run_id"] == run
                && caller["conversation_id"] == parent["conversation_id"]
                && live(&caller)
        })
}

fn live(record: &Value) -> bool {
    string(record, "archive_id").is_empty()
        && matches!(string(record, "state"), "" | "running")
        && record["session_end"].is_null()
        && record["rename_shadow"] != true
        && process_alive(record)
        && !pause_active(record)
        && !terminating::pending(record)
}

fn floor(record: &Value, parent: &Value) -> Option<f64> {
    if record["agent"] != "codex"
        || record["run_identity_version"] != 1
        || record["startup_kind"] != "new"
        || !matches!(string(parent, "source"), "launch" | "adopt")
        || !canonical(string(record, "session_id"))
        || !canonical(string(record, "conversation_id"))
        || !canonical(string(parent, "session_id"))
        || string(record, "run_id").is_empty()
        || string(parent, "run_id").is_empty()
        || string(parent, "conversation_id").is_empty()
        || ["fork_parent_id", "expected_id", "requested_id"]
            .iter()
            .any(|key| !record[*key].is_null() && record[*key] != "")
    {
        return None;
    }
    let linked = parent["at"].as_i64().filter(|at| *at > 0)? as f64;
    let created = record["created"].as_i64().filter(|at| *at > 0)? as f64;
    // Both times have second precision. A resumed run may reuse old history.
    Some(linked.max(created) + 1.0)
}

const OBSERVATION_FIELDS: &[&str] = &[
    "agent",
    "name",
    "layout",
    "session_id",
    "run_id",
    "conversation_id",
    "pid",
    "process_start",
    "run_identity_version",
    "startup_kind",
    "fork_parent_id",
    "expected_id",
    "requested_id",
    "created",
    "agent_home",
    "launch_dir",
    "transcript",
    "parent",
    "state",
    "session_end",
    "archive_id",
    "rename_shadow",
    "last_main_progress_at",
    "turn_started",
];

fn same_observation(before: &Value, current: &Value) -> bool {
    OBSERVATION_FIELDS
        .iter()
        .all(|key| before[*key] == current[*key])
}

struct History {
    file: fs::File,
    account: HistoryRoot,
    path: PathBuf,
    resolved: PathBuf,
    stamp: (FileIdentity, u64),
    pid: u32,
}

impl History {
    fn open(record: &Value) -> Option<Self> {
        let account = HistoryRoot::open(record).ok()?;
        let path = PathBuf::from(string(record, "transcript"));
        let resolved = account.resolve_transcript(&path).ok()?;
        let file = Self::open_file(&path)?;
        let stamp = Self::stamp(&file)?;
        let pid = record["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())?;
        let history = Self {
            file,
            account,
            path,
            resolved,
            stamp,
            pid,
        };
        history.current().then_some(history)
    }

    fn current(&self) -> bool {
        self.account.verify().is_ok()
            && self.account.resolve_transcript(&self.path).ok().as_ref() == Some(&self.resolved)
            && Self::stamp(&self.file) == Some(self.stamp)
            && Self::open_file(&self.path).and_then(|file| Self::stamp(&file)) == Some(self.stamp)
    }

    fn held_by_child(&self) -> bool {
        platform::holds_open(self.pid, &self.resolved).ok() == Some(true)
    }

    fn open_file(path: &Path) -> Option<fs::File> {
        platform::no_follow(fs::OpenOptions::new().read(true))
            .open(path)
            .ok()
    }

    fn stamp(file: &fs::File) -> Option<(FileIdentity, u64)> {
        let (identity, meta) = FileIdentity::with_metadata(file).ok()?;
        (meta.is_file() && identity.inode != 0).then_some((identity, meta.len()))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
