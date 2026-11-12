//! The one reader of a fleet row (zerus-63rru.51): the facts the actor uses,
//! read once and under the names the producers write. A live terminal row is
//! made by `state::live_row` (`src/state/mod.rs`): `process_state` and
//! `runtime_state` from `journal::normalized_activity`, the subagent count
//! from `telemetry`, `optional_questions` from `read_marks::enrich_listing`,
//! `process_work` from the process sample, and no `state` key. A DeepSeek
//! row is made by `state::dsh::snapshot_from` and carries `state` itself.
//!
//! Unknown stays unknown: a count that is missing or null is not zero, a
//! value the reader does not know is not the one it waits for, and a member
//! whose row has an unknown is not free.
use serde_json::Value;

/// Every key path [`Facts::of`] reads, and whether every producer writes it
/// (the guard test checks both against rows the producers made).
pub(super) const READ: [(&str, bool); 12] = [
    ("name", true),
    ("run_id", true),
    ("launch_id", true),
    ("conversation_id", true),
    ("process_state", true),
    ("runtime_state", true),
    ("state", false),
    ("activity", true),
    ("phase", true),
    ("process_work.count", true),
    ("subagent_active_count", true),
    ("optional_questions.count", true),
];

/// What one fleet row says about its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Facts {
    /// One rule for every agent kind: `process_state` "running",
    /// `runtime_state` "live", and `state` absent (a live terminal row) or
    /// "running" (a DeepSeek row).
    pub live: bool,
    /// The shared presentation's status is "ready": no error, recovery,
    /// question, schedule or wait the person would see.
    pub ready: bool,
    pub activity: Option<String>,
    pub phase: Option<String>,
    /// `process_work.count`: the session's own background jobs.
    pub background: Option<u64>,
    /// `subagent_active_count`: its children that work now.
    pub children: Option<u64>,
    /// `optional_questions.count`: questions that wait for an answer.
    pub questions: Option<u64>,
}

/// What presence needs, only when every part of it is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    pub live: bool,
    pub working: bool,
    pub background: bool,
    pub pending: bool,
}

impl Facts {
    pub fn of(row: &Value) -> Self {
        let text = |key: &str| row.get(key).and_then(Value::as_str);
        let state = match row.get("state") {
            None => true,
            Some(value) => value.as_str() == Some("running"),
        };
        let count = |value: Option<&Value>| value.and_then(Value::as_u64);
        Self {
            live: state
                && text("process_state") == Some("running")
                && text("runtime_state") == Some("live"),
            ready: zerus_client_core::view::row(row).status == "ready",
            activity: text("activity").map(str::to_owned),
            phase: text("phase").map(str::to_owned),
            background: count(row.get("process_work").and_then(|w| w.get("count"))),
            children: count(row.get("subagent_active_count")),
            questions: count(row.get("optional_questions").and_then(|q| q.get("count"))),
        }
    }

    /// Live, ready, idle, and known to run no job, child or question.
    pub fn free(&self) -> bool {
        self.live
            && self.ready
            && self.activity.as_deref() == Some("idle")
            && self.phase.as_deref() == Some("idle")
            && self.background == Some(0)
            && self.children == Some(0)
            && self.questions == Some(0)
    }

    /// Working, Background and a pending request, or None while any of them
    /// is unknown.
    pub fn seen(&self) -> Option<Seen> {
        let activity = self.activity.as_deref()?;
        if !matches!(activity, "idle" | "busy" | "working") {
            return None;
        }
        let phase = self.phase.as_deref()?;
        let working = activity != "idle" || self.children? > 0;
        Some(Seen {
            live: self.live,
            working,
            background: self.background? > 0,
            pending: self.questions? > 0 || (phase != "idle" && !working),
        })
    }
}

/// The identity a row names: its session's name, run, launch and
/// conversation, each empty when absent or not a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identity<'a> {
    pub name: &'a str,
    pub run: &'a str,
    pub launch: &'a str,
    pub conversation: &'a str,
}

impl<'a> Identity<'a> {
    pub fn of(row: &'a Value) -> Self {
        let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
        Self {
            name: text("name"),
            run: text("run_id"),
            launch: text("launch_id"),
            conversation: text("conversation_id"),
        }
    }
}
