use super::DecisionState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zerus_team_plan::model::TeamStatus;

pub const MAX_PAGE: u32 = 200;
pub const LIVE_JOURNAL_CAP: u64 = 10_000;

/// The bound part of one roster, without accounts, paths or member state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSession {
    pub member: String,
    pub machine: String,
    pub session_id: String,
}

/// Snapshot high-water mark excludes additions while the person turns pages.
/// The scope binds the cursor to the team, list and filter that issued it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub scope: String,
    pub ceiling: u64,
    pub after: u64,
    #[serde(default)]
    pub rank: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page<T> {
    pub rows: Vec<T>,
    pub next: Option<Cursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxItem {
    pub seq: u64,
    pub kind: String,
    pub node: u32,
    pub generation: u32,
    pub actor: Value,
    pub text: String,
    pub state: String,
    pub reply: Option<String>,
    pub reply_actor: Option<Value>,
    pub created_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    pub seq: u64,
    pub kind: String,
    pub node: Option<u32>,
    pub actor: Value,
    pub text: String,
    pub state: String,
    pub decision: Option<String>,
    pub decided_actor: Option<Value>,
    pub created_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    pub seq: u64,
    pub at_ms: i64,
    pub actor: Value,
    pub kind: String,
    pub request_id: String,
    pub text: String,
    pub refs: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectIntent {
    pub seq: u64,
    pub node: u32,
    pub member: String,
    pub generation: u32,
    pub request_id: String,
    pub actor: Value,
    pub state: String,
    pub kind: String,
    pub outcome: Option<Value>,
    pub created_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub state: DecisionState,
    pub seq: u64,
    pub node: u32,
    pub generation: u32,
    pub actor: Value,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingSuggestion {
    pub role: Option<String>,
    pub pipeline: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationFact {
    pub decision: u64,
    pub node: u32,
    pub actor: Value,
    pub commit: Option<String>,
    pub conflicts: Vec<String>,
    pub created_ms: i64,
}

/// An exact acceptance identity for a service integration attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationAccept {
    pub seq: u64,
    pub node: u32,
    pub generation: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptHolder {
    pub member: String,
    pub generation: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamRow {
    pub id: String,
    pub project: String,
    pub name: String,
    pub status: TeamStatus,
    pub lead: String,
    pub home: String,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaitingCard {
    pub team: String,
    pub card: Card,
}

/// A task creation that the service must perform once or reconcile as uncertain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageTask {
    pub node: u32,
    pub target: u32,
    /// Generation of the target report that asked for this stage.
    pub generation: u32,
    pub state: String,
    pub request_id: Option<String>,
}

/// One snapshot supplies the native stage writer's exact reserved authority
/// and content. The writer also verifies that home is this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageTaskEffect {
    pub team: String,
    pub node: u32,
    pub request_id: String,
    pub home: String,
    pub project: String,
    pub title: String,
    pub text: String,
    pub kind: zerus_team_plan::model::NodeKind,
    pub role: String,
    pub target: zerus_team_plan::model::TaskRef,
    pub target_title: String,
    pub round: u32,
    /// Generation of the target, not of the stage's own later hand-out.
    pub generation: u32,
}
