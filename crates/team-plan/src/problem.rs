//! One refusal, with a stable code a caller can match on.

use serde::{Deserialize, Serialize};

/// Why an operation, a validation or a decision was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    /// One of the constants in [`code`]. Callers match on it; never on `message`.
    pub code: String,
    /// A short explanation for a person.
    pub message: String,
    /// What the problem is about: IDs, field names, a reset time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}

impl Problem {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            refs: Vec::new(),
        }
    }

    pub fn with_refs<I, S>(mut self, refs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.refs = refs.into_iter().map(Into::into).collect();
        self
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Problem {}

/// The stable error codes of Teams.
pub mod code {
    pub const TEAMS_INACTIVE: &str = "teams_inactive";
    pub const NOT_FOUND: &str = "not_found";
    pub const STALE_REVISION: &str = "stale_revision";
    pub const TEMPLATE_CHANGED: &str = "template_changed";
    pub const NOT_ALLOWED: &str = "not_allowed";
    pub const INVALID_STATE: &str = "invalid_state";
    pub const UNKNOWN_ROLE: &str = "unknown_role";
    pub const UNKNOWN_PIPELINE: &str = "unknown_pipeline";
    pub const UNKNOWN_MEMBER: &str = "unknown_member";
    pub const UNKNOWN_ACCOUNT: &str = "unknown_account";
    pub const NO_ACCOUNT_AVAILABLE: &str = "no_account_available";
    pub const NO_SEAT_FOR_STAGE: &str = "no_seat_for_stage";
    pub const DEPENDENCY_CYCLE: &str = "dependency_cycle";
    pub const OVER_CAP: &str = "over_cap";
    pub const DUPLICATE: &str = "duplicate";
    pub const UNKNOWN_FIELD: &str = "unknown_field";
    pub const INTEGRATION_CONFLICT: &str = "integration_conflict";
    pub const INTEGRATION_UNAVAILABLE: &str = "integration_unavailable";
    pub const DELIVERY_UNCERTAIN: &str = "delivery_uncertain";

    /// Every code, for tests that pin the list.
    pub const ALL: [&str; 19] = [
        TEAMS_INACTIVE,
        NOT_FOUND,
        STALE_REVISION,
        TEMPLATE_CHANGED,
        NOT_ALLOWED,
        INVALID_STATE,
        UNKNOWN_ROLE,
        UNKNOWN_PIPELINE,
        UNKNOWN_MEMBER,
        UNKNOWN_ACCOUNT,
        NO_ACCOUNT_AVAILABLE,
        NO_SEAT_FOR_STAGE,
        DEPENDENCY_CYCLE,
        OVER_CAP,
        DUPLICATE,
        UNKNOWN_FIELD,
        INTEGRATION_CONFLICT,
        INTEGRATION_UNAVAILABLE,
        DELIVERY_UNCERTAIN,
    ];
}
