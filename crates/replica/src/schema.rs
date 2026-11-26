//! The task schema shared by every replica.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// The layout version of task and project documents this build writes.
pub const SCHEMA: i64 = 1;
/// The longest prompt or comment: the input limit of hgs.
pub const MAX_TEXT: usize = 64 * 1024;
/// A task holding this much text (its prompt and history) accepts no further comments.
pub const MAX_DOCUMENT: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Backlog,
    Todo,
    Active,
    Review,
    Done,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::Active => "active",
            Self::Review => "review",
            Self::Done => "done",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::Todo => "To do",
            Self::Active => "In progress",
            Self::Review => "In review",
            Self::Done => "Done",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "backlog" => Self::Backlog,
            "todo" => Self::Todo,
            "active" => Self::Active,
            "review" => Self::Review,
            "done" => Self::Done,
            _ => return Err(Error::new(format!("unknown stage {value:?}"))),
        })
    }
}

/// A session on a computer. `session` is a display/routing hint; new callers
/// retain the immutable launch identity (or run identity for legacy bindings).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionKey {
    pub machine: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
}

impl PartialEq for SessionKey {
    fn eq(&self, other: &Self) -> bool {
        self.machine == other.machine
            && if self.session_id.is_empty() && other.session_id.is_empty() {
                self.session == other.session
            } else {
                !self.session_id.is_empty() && self.session_id == other.session_id
            }
    }
}
impl Eq for SessionKey {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    Any,
    Session(SessionKey),
    #[serde(rename = "new_session")]
    NewSession {
        parent: SessionKey,
    },
    Subagent {
        parent: SessionKey,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Actor {
    Person,
    Agent(SessionKey),
}

/// The person's own message whose request an agent carried out
/// (zerus-46l1q): the asking session's exact identity and the message's
/// request. It records evidence only; the writer that admitted the action
/// proved the request itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRequestRef {
    pub machine: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub request_id: String,
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub kind: String,
    pub actor: Actor,
    pub at: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionKey>,
    /// A send's durable hgs request, for matching it to the paste receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::TaskAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<TaskDelivery>,
    /// Why a report was recorded without moving its task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<crate::execution::Stale>,
    /// The requirement revision a report says it considered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub considered: Option<u64>,
    /// The immutable attempt named by a scoped report, including a stale
    /// report. This never means the newest attempt at the time of reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_attempt: Option<String>,
    /// The history entry a person's review of an unexplained execution entry
    /// names (`Execution::unexplained`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviews: Option<String>,
    /// For an execution entry an attempt's settlement wrote: the fingerprint
    /// of that exact attempt record (`Attempt::fingerprint`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    /// The person's message an agent's entry carried out (zerus-46l1q).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_request: Option<PersonRequestRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_review: Option<crate::quick_review::Decision>,
}

/// A task message's durable outbox reservation. Submission is not consumption.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDelivery {
    pub mode: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stage: String,
    #[serde(default)]
    pub stage_at: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dispatch_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub submitted_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub conversation_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// A coordinator's explicit binding to an observed native child. The parent
/// conversation and dispatch receipt scope the identity; names remain captions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWorker {
    pub parent: SessionKey,
    pub conversation_id: String,
    pub provider: String,
    pub id: String,
    pub name: String,
    pub request_id: String,
}

/// The expected review participant. This is assignment metadata, not authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOwner {
    Person,
    Technical,
}

impl ReviewOwner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Technical => "technical",
        }
    }
}

/// The review assignment and the criterion agreed before work starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAssignment {
    pub owner: ReviewOwner,
    pub criterion: String,
}

impl ReviewAssignment {
    pub fn new(owner: ReviewOwner, criterion: String) -> Result<Self> {
        let assignment = Self { owner, criterion };
        assignment.check()?;
        Ok(assignment)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.criterion.trim().is_empty()
            || self.criterion.len() > 512
            || crate::forbidden_char(&self.criterion).is_some()
        {
            return Err(Error::new(
                "the review criterion must be one line of at most 512 bytes",
            ));
        }
        Ok(())
    }
}

/// A task as callers read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskView {
    pub uuid: String,
    pub id: String,
    pub project: String,
    pub schema: i64,
    pub writable: bool,
    pub created_at: i64,
    /// When the task entered its current stage (Unix milliseconds).
    pub stage_at: i64,
    pub author: Actor,
    pub stage: Stage,
    pub target: Target,
    /// Optional routing metadata; older current-schema readers ignore it.
    #[cfg(not(feature = "test-without-team"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<crate::TaskTeam>,
    #[cfg(not(feature = "test-without-team"))]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub team_invalid: bool,
    pub rank: String,
    pub session: Option<SessionKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<TaskWorker>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_assignment: Option<ReviewAssignment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_report_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quick_review_status: Option<crate::quick_review::Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quick_review: Option<crate::quick_review::Disposition>,
    pub cancelled_at: Option<i64>,
    pub cancel_reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::TaskAttachment>,
    pub log: Vec<LogEntry>,
    /// The task's execution: its current attempt and requirement revision
    /// (`execution` module). None for a task that is not In progress and
    /// recorded no attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<crate::execution::Execution>,
}

impl TaskView {
    /// The report that put this ordinary task in Review. Two matching
    /// reports, an unsettled execution or a historical result prove no owner.
    pub(crate) fn review_report(&self) -> Option<usize> {
        use crate::execution::Phase;

        #[cfg(not(feature = "test-without-team"))]
        if self.team.is_some() || self.team_invalid {
            return None;
        }
        if self.stage != Stage::Review || !self.writable || self.cancelled_at.is_some()
            || self.summary.trim().is_empty()
        {
            return None;
        }
        let mut attempt = None;
        if let Some(execution) = &self.execution {
            let current = execution.current.as_ref()?;
            if execution.conflict || !execution.unknown.is_empty()
                || !execution.unexplained.is_empty() || current.phase != Phase::Ready
            {
                return None;
            }
            if !current.legacy {
                attempt = Some(current.id.as_str());
            }
        }
        // Legacy writers have no attempt ID. Two current reports merged
        // from different replicas still provide no single settled result.
        let opened = self.log.iter().rposition(|entry| {
            entry.kind == "resumed"
                || entry.kind == "stage" && matches!(entry.text.as_str(), "todo" | "backlog")
        }).map_or(0, |position| position + 1);
        let mut reports = self.log.iter().enumerate().skip(opened).filter(|(_, entry)| {
            entry.kind == "report" && entry.stale.is_none()
                && (entry.reported_attempt.as_deref() == attempt
                    || (entry.reported_attempt.is_none() && entry.actor == Actor::Person))
        });
        let (index, report) = reports.next()?;
        let reporter_matches = match &report.actor {
            Actor::Person => true,
            Actor::Agent(session) => self.session.as_ref() == Some(session),
        };
        (reports.next().is_none() && reporter_matches
            && report.at == self.stage_at && report.text == self.summary).then_some(index)
    }

    pub fn review_editable(&self) -> bool {
        self.review_report().is_some()
    }

    pub fn review_lane(&self) -> Option<&'static str> {
        if self.stage != Stage::Review {
            return None;
        }
        #[cfg(not(feature = "test-without-team"))]
        if self.team.is_some() && !self.team_invalid {
            return Some("team");
        }
        Some(if self.review_editable() {
            self.review_assignment.as_ref().map_or("triage", |assignment| assignment.owner.as_str())
        } else {
            "triage"
        })
    }

    /// Dispatch receipt, or an explicit direct-work acknowledgement. Names do
    /// not identify an execution. This anchor is not a distributed claim.
    pub fn execution_id(&self) -> Option<String> {
        // A recorded attempt is its own anchor: its native request, or the
        // attempt itself for a direct start.
        if let Some(current) = self
            .execution
            .as_ref()
            .and_then(|execution| execution.current.as_ref())
            .filter(|current| !current.legacy)
        {
            return Some(if current.request_id.is_empty() {
                format!("attempt:{}", current.id)
            } else {
                current.request_id.clone()
            });
        }
        self.log
            .iter()
            .rev()
            .find(|e| e.kind == "sent")
            .and_then(|e| e.request_id.clone())
            .or_else(|| {
                self.log
                    .iter()
                    .rev()
                    .find(|e| e.kind == "started" && e.session == self.session)
                    .map(|e| format!("started:{}", e.at))
            })
    }
}

pub(crate) fn check_text(text: &str, what: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::new(format!("the {what} is empty")));
    }
    if text.len() > MAX_TEXT {
        return Err(Error::new(format!("the {what} is longer than 64 KiB")));
    }
    check_controls(text, what)
}

/// hgs pastes only text: terminal control characters never reach an agent.
pub(crate) fn check_controls(text: &str, what: &str) -> Result<()> {
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(Error::new(format!(
            "the {what} contains terminal control characters"
        )));
    }
    Ok(())
}
