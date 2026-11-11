use serde::{Deserialize, Serialize};
use zerus_team_plan::{catalog::TemplateBundle, model::*};

/// Construct only after the caller's native authority has been verified.
/// This type deliberately cannot be deserialized from an operation request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    Person {
        person: String,
        origin: PersonOrigin,
    },
    Session {
        person: String,
        team: String,
        member: String,
        session: SessionKey,
    },
    Other {
        person: String,
        session: SessionKey,
    },
    /// Internal observer/recorder, never an authority available over the CLI.
    Service {
        person: String,
        team: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonOrigin {
    DesktopUi,
    CliInterim,
}

impl Actor {
    pub fn person(&self) -> &str {
        match self {
            Self::Person { person, .. }
            | Self::Session { person, .. }
            | Self::Other { person, .. }
            | Self::Service { person, .. } => person,
        }
    }
}

/// The task writer's authoritative snapshot, not an independently editable
/// node state. Revisions are the task document's opaque tokens. Each update
/// compares the previously projected token so delayed observations cannot
/// overwrite a newer projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskObservation {
    pub revision: String,
    #[serde(default)]
    pub previous_revision: String,
    pub stage: TaskStage,
    #[serde(default)]
    pub cancelled: bool,
    pub rank: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStage {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum HandoutOutcome {
    Submitted { receipt: String },
    Refused { reason: String },
    CuttingOver { reason: String },
    Uncertain { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Create {
        project: String,
        name: String,
        goal: String,
        home: String,
        bundle: Box<TemplateBundle>,
        integration_branch: String,
        base_commit: String,
    },
    SetMemberSession {
        member: String,
        session: SessionKey,
        account: String,
        worktree: String,
        ownership_nonce: String,
    },
    SetMemberState {
        member: String,
        state: MemberState,
    },
    Pause,
    Resume,
    Stop,
    Archive,
    PauseMember {
        member: String,
    },
    ResumeMember {
        member: String,
    },
    StopMember {
        member: String,
    },
    ArchiveMember {
        member: String,
    },
    AddTask {
        task: TaskRef,
        title: String,
        observation: TaskObservation,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        pipeline: Option<String>,
    },
    Admit {
        node: NodeId,
    },
    Route {
        node: NodeId,
        role: String,
        pipeline: String,
        #[serde(default)]
        deps: Vec<Dep>,
        #[serde(default)]
        writes: Vec<String>,
    },
    Hold {
        node: NodeId,
    },
    Release {
        node: NodeId,
    },
    Cancel {
        node: NodeId,
        reason: String,
    },
    HandoutIntent {
        node: NodeId,
        member: String,
        effect_request_id: String,
    },
    HandoutOutcome {
        effect_request_id: String,
        outcome: HandoutOutcome,
    },
    ObserveTask {
        node: NodeId,
        observation: TaskObservation,
    },
    RecordReport {
        node: NodeId,
        generation: u32,
        observation: TaskObservation,
        text: String,
        commit: String,
    },
    Accept {
        node: NodeId,
        #[serde(default)]
        integration_commit: Option<String>,
    },
    Return {
        node: NodeId,
        reason: String,
    },
    Reassign {
        node: NodeId,
        member: String,
        reason: String,
    },
    Ask {
        node: NodeId,
        generation: u32,
        text: String,
    },
    Reply {
        item: u64,
        text: String,
    },
    OpenCard {
        kind: String,
        text: String,
        #[serde(default)]
        node: Option<NodeId>,
    },
    DecideCard {
        card: u64,
        decision: String,
    },
    ChangeSettings {
        settings: Settings,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Request {
    pub request_id: String,
    pub team: String,
    pub expected_revision: u64,
    #[serde(flatten)]
    pub operation: Operation,
}

// Splitting the envelope explicitly keeps unknown operation fields strict;
// serde's flattened fields and deny_unknown_fields cannot be combined.
impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let mut take = |key: &str| {
            fields.remove(key).ok_or_else(|| {
                D::Error::missing_field(match key {
                    "request_id" => "request_id",
                    "team" => "team",
                    _ => "expected_revision",
                })
            })
        };
        let request_id = serde_json::from_value(take("request_id")?).map_err(D::Error::custom)?;
        let team = serde_json::from_value(take("team")?).map_err(D::Error::custom)?;
        let expected_revision =
            serde_json::from_value(take("expected_revision")?).map_err(D::Error::custom)?;
        let operation: Operation =
            serde_json::from_value(serde_json::Value::Object(fields.clone()))
                .map_err(D::Error::custom)?;
        // Internally tagged unit variants otherwise ignore leftover fields.
        let known = serde_json::to_value(&operation).map_err(D::Error::custom)?;
        if let Some(unknown) = fields.keys().find(|key| known.get(*key).is_none()) {
            return Err(D::Error::custom(format!("unknown field `{unknown}`")));
        }
        Ok(Self {
            request_id,
            team,
            expected_revision,
            operation,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub ok: bool,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u64>,
}
