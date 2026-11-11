use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_PAGE: u32 = 200;
pub const LIVE_JOURNAL_CAP: u64 = 10_000;

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
    pub outcome: Option<Value>,
    pub created_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub seq: u64,
    pub node: u32,
    pub generation: u32,
    pub actor: Value,
    pub kind: String,
    pub text: String,
}
