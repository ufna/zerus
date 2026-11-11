//! Compact view models of the Teams reads and their projections. Each reply is
//! projected once in the core and serialized as it is to the webview, which
//! never parses an `hgs` reply. Texts and lists are bounded; a view that left
//! something out says so (`truncated`) and names the next page's cursor
//! (`next`).
//!
//! Adapted from the fork's `client/core/src/teams/view.rs` (WhiteWh/zerus at
//! `ca4cc4481`), reduced to the views of M1 and to the reply shapes package 5
//! pins: the team list ([`list`]), one team (its bar, its members and its
//! queue as the board's groups in [`board`]) and the open owner cards
//! ([`waiting`]).
pub mod board;
pub mod list;

pub use board::team;
pub use list::{list, waiting};

use serde::Serialize;
use serde_json::Value;

/// The longest name, title or short field kept.
pub const NAME_CHARS: usize = 200;
/// The longest free text kept.
pub const TEXT_CHARS: usize = 8000;
/// The longest line kept (a task title, a waiting text).
pub const LINE_CHARS: usize = 600;
/// Rows one page read asks for and one view keeps: the store's page bound.
pub const PAGE: usize = 200;
/// Teams one list keeps.
pub const LIST_KEPT: usize = PAGE;
/// Members one team keeps.
pub const MEMBERS_KEPT: usize = 256;
/// Stages one card keeps.
pub const STAGES_KEPT: usize = 16;
/// The board's groups, in their order; every board carries them all so the
/// view never reshuffles its columns.
pub const GROUPS: [&str; 7] = [
    "draft",
    "waiting",
    "pending",
    "in_progress",
    "reported",
    "accepted",
    "cancelled",
];

// ---------------------------------------------------------------- helpers

pub(crate) fn cut(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => text[..at].to_owned(),
        None => text.to_owned(),
    }
}

pub(crate) fn text(value: &Value, key: &str, chars: usize) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| cut(text, chars))
        .unwrap_or_default()
}

pub(crate) fn name(value: &Value, key: &str) -> String {
    text(value, key, NAME_CHARS)
}

/// A string field whose absence means the type's default (`not_created` for a
/// member, `draft` for a node): the CLI leaves defaults out.
pub(crate) fn or_default(value: &Value, key: &str, default: &str) -> String {
    match value.get(key).and_then(Value::as_str) {
        Some(found) if !found.is_empty() => cut(found, NAME_CHARS),
        _ => default.to_owned(),
    }
}

pub(crate) fn count(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|found| {
            found
                .as_u64()
                .or_else(|| found.as_f64().map(|n| n.max(0.0) as u64))
        })
        .unwrap_or(0)
}

/// A string field that may be absent: empty and missing both mean no value.
pub(crate) fn optional_name(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|found| cut(found, NAME_CHARS))
        .filter(|found| !found.is_empty())
}

/// A numeric field that may be absent.
pub(crate) fn optional_count(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|found| {
        found
            .as_u64()
            .or_else(|| found.as_f64().map(|n| n.max(0.0) as u64))
    })
}

/// A queue row's own kind: `task` or `stage`; absent means a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Task,
    Stage,
}

pub(crate) fn row_kind(value: &Value) -> Option<RowKind> {
    match value.get("kind").and_then(Value::as_str) {
        Some("task") => Some(RowKind::Task),
        Some("stage") => Some(RowKind::Stage),
        _ => None,
    }
}

/// A stage's kind: `review`, `test` or `land`. Any other word is kept out as
/// absent, never an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    Review,
    Test,
    Land,
}

pub(crate) fn stage_kind(value: &Value) -> Option<StageKind> {
    match value.get("kind").and_then(Value::as_str) {
        Some("review") => Some(StageKind::Review),
        Some("test") => Some(StageKind::Test),
        Some("land") => Some(StageKind::Land),
        _ => None,
    }
}

pub(crate) fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The cursor of the reply's next page, when it names one.
pub(crate) fn next_cursor(value: &Value) -> Option<String> {
    value
        .get("next")
        .and_then(Value::as_str)
        .filter(|next| !next.is_empty())
        .map(|next| cut(next, crate::teams::read::CURSOR_BYTES))
}

// ---------------------------------------------------------------- the list

/// One team of `hgs team list`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamSummary {
    pub id: String,
    pub project: String,
    pub name: String,
    pub status: String,
    /// The lead's member id.
    pub lead: String,
    /// The computer the lead runs on.
    pub home: String,
    pub revision: u64,
    /// The lead's bound session: how the desktop finds a session's team.
    pub lead_session: Option<SessionRef>,
}

impl TeamSummary {
    pub fn draft(&self) -> bool {
        self.status == "draft"
    }
}

/// The teams of this computer, as the store lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamList {
    pub teams: Vec<TeamSummary>,
    pub truncated: bool,
    /// The cursor of the next page, when the reply names one.
    pub next: Option<String>,
}

// --------------------------------------------------------------- one team

/// A member's session: the computer UUID and the session identity
/// (`launch_id:<uuid>` or `run_id:<id>`). A name is never an identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SessionRef {
    pub machine: String,
    pub session_id: String,
}

/// The lead's bound session of a list row; absent when the row has none.
pub(crate) fn lead_session(row: &Value) -> Option<SessionRef> {
    let found = row.get("lead_session").filter(|lead| lead.is_object())?;
    let view = SessionRef {
        machine: name(found, "machine"),
        session_id: name(found, "session_id"),
    };
    (!view.session_id.is_empty()).then_some(view)
}

pub(crate) fn session(value: &Value) -> Option<SessionRef> {
    let found = value.get("session").filter(|session| session.is_object())?;
    let view = SessionRef {
        machine: name(found, "machine"),
        session_id: name(found, "session_id"),
    };
    (!view.session_id.is_empty()).then_some(view)
}

/// One member of a team, as the Team tab's members column shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemberCard {
    pub id: String,
    pub role: String,
    pub agent: String,
    pub account: String,
    /// `not_created`, `active`, `paused`, `removed`.
    pub state: String,
    pub lead: bool,
    pub session: Option<SessionRef>,
    /// The model its role names, when it names one.
    pub model: Option<String>,
}

/// The member counts of `team status`'s `counts.members`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemberCounts {
    pub total: u64,
    pub not_created: u64,
    pub active: u64,
    pub paused: u64,
    pub removed: u64,
}

/// The node counts of `team status`'s `counts.nodes`, by state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NodeCounts {
    pub waiting: u64,
    pub pending: u64,
    pub in_progress: u64,
    pub reported: u64,
    pub accepted: u64,
    pub cancelled: u64,
}

/// The numbers the team bar shows: `team status`'s `counts`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamCounts {
    pub members: MemberCounts,
    pub nodes: NodeCounts,
    /// The lead's open inbox items.
    pub inbox: u64,
    /// The person's open cards.
    pub cards: u64,
}

/// One stage of a card: the review or test node its pipeline created.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StageCard {
    pub node: u64,
    /// The position in the pipeline.
    pub index: u64,
    pub role: String,
    /// The node state (`waiting`, `pending`, `in_progress`, `reported`,
    /// `accepted`, `cancelled`).
    pub state: String,
    /// The member the stage is assigned to: who does the stage.
    pub assignee: String,
    /// The member who implemented the task the stage checks (the plan's
    /// binding decision). Never the stage's own worker.
    pub implementer: String,
    /// `review`, `test` or `land`; absent when the row has no `stage` object or
    /// names another kind.
    pub kind: Option<StageKind>,
    /// The node this stage checks.
    pub target: Option<u64>,
    /// The target's round this stage checks.
    pub round: Option<u64>,
}

/// The task a card stands for: the replicated task, never the node alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TaskRefView {
    pub project: String,
    pub uuid: String,
}

pub(crate) fn task_ref(node: &Value) -> Option<TaskRefView> {
    let task = node.get("task").filter(|task| task.is_object())?;
    let view = TaskRefView {
        project: name(task, "project"),
        uuid: name(task, "uuid"),
    };
    (!view.uuid.is_empty()).then_some(view)
}

/// One board card: a node of the plan or the queue and, when a pipeline runs,
/// its stages. A card is never a stage node of another card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QueueCard {
    pub node: u64,
    pub title: String,
    pub state: String,
    pub task: Option<TaskRefView>,
    pub role: String,
    pub pipeline: String,
    pub assignee: String,
    pub implementer: String,
    /// The task's board rank: the queue's order.
    pub rank: String,
    pub round: u64,
    /// The queue row's own kind; absent means a task.
    pub kind: Option<RowKind>,
    /// The pinned pipeline's rounds, when the row names them.
    pub rounds: Option<u64>,
    pub held: bool,
    pub admitted: bool,
    pub integrated: bool,
    pub decided_by: String,
    pub stages: Vec<StageCard>,
}

/// One group of the board: its state and its cards.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BoardGroup {
    pub state: String,
    pub cards: Vec<QueueCard>,
}

/// The team's bar: what the tab shows above the board.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamBar {
    pub id: String,
    pub project: String,
    pub name: String,
    pub status: String,
    pub goal: String,
    pub lead: String,
    pub integration_branch: String,
    pub base_commit: String,
    pub revision: u64,
    pub counts: TeamCounts,
}

/// `hgs team status` and `hgs team queue` for the Team tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamView {
    pub bar: TeamBar,
    pub members: Vec<MemberCard>,
    pub groups: Vec<BoardGroup>,
    /// Cards or stages over a bound were left out.
    pub truncated: bool,
    /// The cursor of the queue's next page, when the reply names one.
    pub next: Option<String>,
    pub revision: u64,
}

// ----------------------------------------------------------------- waiting

/// One thing that waits for the person: an open owner card.
///
/// A request that only the person can answer inside a team *session* is not
/// here: the desktop's session attention data already carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingItem {
    /// Team and card sequence, for the decision that answers it.
    pub id: String,
    pub team: String,
    pub card: u64,
    pub kind: String,
    pub node: Option<u64>,
    pub text: String,
    pub state: String,
    /// The person the card is for, from the record's actor.
    pub by: String,
    pub created_ms: i64,
}

/// `hgs team waiting`: the open owner cards of every team, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingView {
    pub items: Vec<WaitingItem>,
    /// The cursor of the next page, when the reply names one.
    pub next: Option<String>,
}

#[cfg(test)]
mod tests;
