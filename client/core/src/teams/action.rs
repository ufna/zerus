//! The team actions the desktop offers, as guarded `hgs team <op> --json`
//! calls with a request identity.
//!
//! Every call carries the team, the action, the revision the view was read at
//! and a canonical UUID request id. The store keeps the receipt of every id,
//! so a call sent again with the same id answers its stored receipt instead of
//! applying the change twice.
//!
//! [`classify`] turns a call's exit into a [`TeamOutcome`]:
//!
//! - `done`: hgs took the change and gave a receipt;
//! - `failed`: hgs refused it, or it never ran; nothing was applied, and a new
//!   attempt may get a new request id;
//! - `uncertain`: the call timed out, ended abnormally or gave no valid
//!   receipt, so the change may have been applied. It is never sent again by
//!   the desktop by itself: the view reads the team first, and only the person
//!   may send it again, with the same request id, which the store answers with
//!   its receipt if it had applied it;
//! - `confirm`: a destructive action was asked for without `confirmed`;
//!   nothing ran, and the confirmation names what the person is asked.
use crate::actions::{Exit, Invocation};
use crate::run::run;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::time::Duration;
use zerus_team_plan::model::{Dep, NodeId, Settings};

/// The longest reason, comment or reply text an action sends: the store's own
/// limit for every one of them.
pub const TEXT_CHARS: usize = 16_384;
/// The longest name an action sends.
pub const NAME_CHARS: usize = 200;
/// The longest stderr a failure keeps.
pub const DETAIL_CHARS: usize = 2000;
/// How long an action waits for `hgs` (it is this computer's hgs).
const TIMEOUT: Duration = Duration::from_secs(15);

/// What the person is asked before a destructive action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Confirmation {
    /// The dialog's stable name, for the frontend's labels.
    pub name: String,
    pub title: String,
    pub text: String,
    /// The confirming button's text.
    pub action: String,
}

/// One team action, as the webview asks for it (`{"kind": "...", ...}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeamAction {
    /// Creates a team from a catalog template; the CLI resolves the bundle,
    /// and `team` is the new team's id.
    Create {
        template: String,
        project: String,
        name: String,
        #[serde(default)]
        goal: String,
        /// The computer the lead runs on (its UUID).
        home: String,
        integration_branch: String,
        base_commit: String,
    },
    /// Pauses a team: it hands nothing out until resumed.
    Pause,
    /// Resumes a team the person paused.
    Resume,
    /// Cancels every open task and hands nothing out again.
    Stop,
    /// Hides a done or stopped team, keeping its history.
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
    /// Changes the team's settings; the whole object, with its bounds.
    Settings {
        settings: Settings,
    },
    /// Answers an owner card: the person's decision.
    Decide {
        card: u64,
        decision: String,
    },
    /// Admits a waiting queue node.
    Admit {
        node: NodeId,
    },
    /// Routes a node to a role and a pipeline.
    Route {
        node: NodeId,
        role: String,
        pipeline: String,
        #[serde(default)]
        deps: Vec<Dep>,
        #[serde(default)]
        writes: Vec<String>,
        #[serde(default)]
        assignee: Option<String>,
    },
    /// Accepts a node's report.
    Accept {
        node: NodeId,
    },
    /// Returns a node to its implementer with the reason.
    Return {
        node: NodeId,
        reason: String,
    },
    /// Gives a node to another member.
    Reassign {
        node: NodeId,
        member: String,
        #[serde(default)]
        reason: String,
    },
    Hold {
        node: NodeId,
    },
    Release {
        node: NodeId,
    },
    Cancel {
        node: NodeId,
        #[serde(default)]
        reason: String,
    },
    /// Answers a lead inbox item (`hgs team reply`).
    Reply {
        item: u64,
        text: String,
    },
    /// A member's question to its lead (`hgs team ask`).
    Ask {
        node: NodeId,
        generation: u32,
        text: String,
    },
}

/// One call from the view: the team (empty only for Create), its name for the
/// confirmation text, the action, the revision the view was read at, whether
/// the person confirmed a destructive action, and the request id of an earlier
/// attempt the person sends again (None: a new request).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamCall {
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub team_name: String,
    pub action: TeamAction,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub request_id: Option<String>,
}

/// How a call ended for the view.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum TeamOutcome {
    /// hgs applied it; `receipt` is its answer.
    Done { receipt: Value, request_id: String },
    /// Nothing was applied: hgs refused it (its words and stable code) or the
    /// call never ran.
    Failed {
        detail: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        code: String,
        request_id: String,
    },
    /// It may have been applied. Never sent again by the desktop by itself:
    /// read the team, then only the person may send it again with this
    /// `request_id`.
    Uncertain { detail: String, request_id: String },
    /// Not sent: the person confirms first.
    Confirm { confirmation: Confirmation },
}

impl TeamOutcome {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Done { request_id, .. }
            | Self::Failed { request_id, .. }
            | Self::Uncertain { request_id, .. } => Some(request_id),
            Self::Confirm { .. } => None,
        }
    }

    /// Whether the desktop may send the same call again by itself. Never for
    /// an uncertain one: its effect may have happened. Only the person repeats
    /// it, with the same request id.
    pub fn repeatable_by_desktop(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Confirm { .. })
    }
}

/// A canonical UUID as the team store requires a request id: 8-4-4-4-12
/// lowercase hexadecimal digits.
pub fn canonical_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

/// A new request identity.
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn plain(text: &str, what: &str, chars: usize) -> Result<(), String> {
    if text.chars().count() > chars {
        return Err(format!("{what} is too long."));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(format!("{what} contains control characters."));
    }
    Ok(())
}

/// The store's rule for a reason, comment, reply or question: text within its
/// bound, and not blank.
fn reason(text: &str, what: &str) -> Result<(), String> {
    plain(text, what, TEXT_CHARS)?;
    if text.trim().is_empty() {
        return Err(format!("{what} is required."));
    }
    Ok(())
}

fn word(text: &str, what: &str) -> Result<(), String> {
    if text.is_empty()
        || text.len() > 128
        || text.starts_with('-')
        || text.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(format!("Not a {what}."));
    }
    Ok(())
}

fn node(node: NodeId) -> Result<(), String> {
    if node == 0 {
        return Err("Not a node number.".into());
    }
    Ok(())
}

fn card(card: u64) -> Result<(), String> {
    if card == 0 {
        return Err("Not a card number.".into());
    }
    Ok(())
}

impl TeamAction {
    /// The operation: `hgs team <op> --json`.
    pub fn op(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Archive => "archive",
            Self::PauseMember { .. } => "pause-member",
            Self::ResumeMember { .. } => "resume-member",
            Self::StopMember { .. } => "stop-member",
            Self::ArchiveMember { .. } => "archive-member",
            Self::Settings { .. } => "settings",
            Self::Decide { .. } => "decide",
            Self::Admit { .. } => "admit",
            Self::Route { .. } => "route",
            Self::Accept { .. } => "accept",
            Self::Return { .. } => "return",
            Self::Reassign { .. } => "reassign",
            Self::Hold { .. } => "hold",
            Self::Release { .. } => "release",
            Self::Cancel { .. } => "cancel",
            Self::Reply { .. } => "reply",
            Self::Ask { .. } => "ask",
        }
    }

    /// Whether the call names a team. Only Create does not.
    pub fn names_team(&self) -> bool {
        !matches!(self, Self::Create { .. })
    }

    /// The fields the operation sends besides the envelope's.
    pub fn input(&self) -> Map<String, Value> {
        let value = match self {
            Self::Create {
                project,
                name,
                goal,
                home,
                integration_branch,
                base_commit,
                ..
            } => json!({
                "project": project,
                "name": name,
                "goal": goal,
                "home": home,
                "integration_branch": integration_branch,
                "base_commit": base_commit,
            }),
            Self::Pause | Self::Resume | Self::Stop | Self::Archive => json!({}),
            Self::PauseMember { member }
            | Self::ResumeMember { member }
            | Self::StopMember { member }
            | Self::ArchiveMember { member } => json!({"member": member}),
            Self::Settings { settings } => json!({"settings": settings}),
            Self::Decide { card, decision } => json!({"card": card, "decision": decision}),
            Self::Admit { node } => json!({"node": node}),
            Self::Route {
                node,
                role,
                pipeline,
                deps,
                writes,
                assignee,
            } => {
                let mut input = json!({"node": node, "role": role, "pipeline": pipeline, "deps": deps, "writes": writes});
                if let Some(assignee) = assignee {
                    input["assignee"] = json!(assignee);
                }
                input
            }
            Self::Accept { node } => json!({"node": node}),
            Self::Return { node, reason } => json!({"node": node, "reason": reason}),
            Self::Reassign {
                node,
                member,
                reason,
            } => json!({"node": node, "member": member, "reason": reason}),
            Self::Hold { node } | Self::Release { node } => json!({"node": node}),
            Self::Cancel { node, reason } => json!({"node": node, "reason": reason}),
            Self::Reply { item, text } => json!({"item": item, "text": text}),
            Self::Ask {
                node,
                generation,
                text,
            } => json!({"node": node, "generation": generation, "text": text}),
        };
        match value {
            Value::Object(map) => map,
            _ => unreachable!("an action's input is an object"),
        }
    }

    /// Checks what the call sends before anything runs.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Create {
                template,
                project,
                name,
                goal,
                home,
                integration_branch,
                base_commit,
            } => {
                word(template, "template")?;
                plain(project, "A project", 256)?;
                if project.trim().is_empty() {
                    return Err("Name the project.".into());
                }
                if name.trim().is_empty() {
                    return Err("Name the team.".into());
                }
                plain(name, "A team name", NAME_CHARS)?;
                plain(goal, "A goal", TEXT_CHARS)?;
                word(home, "computer id")?;
                word(integration_branch, "branch")?;
                word(base_commit, "commit")?;
                Ok(())
            }
            Self::Pause | Self::Resume | Self::Stop | Self::Archive => Ok(()),
            Self::PauseMember { member }
            | Self::ResumeMember { member }
            | Self::StopMember { member }
            | Self::ArchiveMember { member } => word(member, "member"),
            Self::Settings { .. } => Ok(()),
            Self::Decide { card: id, decision } => {
                card(*id)?;
                reason(decision, "A decision")
            }
            Self::Admit { node: id } => node(*id),
            Self::Route {
                node: id,
                role,
                pipeline,
                writes,
                assignee,
                ..
            } => {
                node(*id)?;
                word(role, "role")?;
                word(pipeline, "pipeline")?;
                if let Some(assignee) = assignee {
                    word(assignee, "member")?;
                }
                if writes.len() > 200 {
                    return Err("Too many write paths.".into());
                }
                for path in writes {
                    plain(path, "A write path", 4096)?;
                    let normalized = path.replace('\\', "/");
                    let bytes = normalized.as_bytes();
                    if normalized.starts_with('/')
                        || (bytes.len() > 1 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
                    {
                        return Err("A write path is relative to the repository.".into());
                    }
                }
                Ok(())
            }
            Self::Accept { node: id } | Self::Hold { node: id } | Self::Release { node: id } => {
                node(*id)
            }
            Self::Return {
                node: id,
                reason: why,
            } => {
                node(*id)?;
                reason(why, "A return's reason")
            }
            Self::Reassign {
                node: id,
                member,
                reason: why,
            } => {
                node(*id)?;
                word(member, "member")?;
                reason(why, "A reason")
            }
            Self::Cancel {
                node: id,
                reason: why,
            } => {
                node(*id)?;
                reason(why, "A reason")
            }
            Self::Reply { item, text } => {
                card(*item)?;
                reason(text, "A reply")
            }
            Self::Ask {
                node: id,
                generation: _,
                text,
            } => {
                node(*id)?;
                reason(text, "A question")
            }
        }
    }

    /// What the person is asked before a destructive action.
    pub fn confirmation(&self, team_name: &str) -> Option<Confirmation> {
        let name = if team_name.trim().is_empty() {
            "this team"
        } else {
            team_name.trim()
        };
        let ask = |dialog: &str, title: &str, text: String, action: &str| Confirmation {
            name: dialog.to_owned(),
            title: title.to_owned(),
            text,
            action: action.to_owned(),
        };
        match self {
            Self::Stop => Some(ask(
                "teamStopConfirm",
                "Stop team?",
                format!(
                    "Stop {name}? Every open task is cancelled and no task is handed out again. Member sessions keep running; their current turns finish."
                ),
                "Stop team",
            )),
            Self::Archive => Some(ask(
                "teamArchiveConfirm",
                "Archive team?",
                format!(
                    "Archive {name}? Its history, tasks and reports stay; the team leaves the list of teams."
                ),
                "Archive",
            )),
            Self::StopMember { member } => Some(ask(
                "teamStopMemberConfirm",
                "Stop member?",
                format!("Stop {member} of {name}? Its current turn finishes and it takes no new task."),
                "Stop member",
            )),
            Self::ArchiveMember { member } => Some(ask(
                "teamArchiveMemberConfirm",
                "Remove member?",
                format!("Remove {member} from {name}? Its history and reports stay."),
                "Remove member",
            )),
            Self::Cancel { node, .. } => Some(ask(
                "teamCancelConfirm",
                "Cancel task?",
                format!("Cancel task #{node} of {name}? Its work stops and its result is not accepted."),
                "Cancel task",
            )),
            _ => None,
        }
    }
}

fn outcome_failed(detail: impl Into<String>, request_id: &str) -> TeamOutcome {
    TeamOutcome::Failed {
        detail: detail.into(),
        code: String::new(),
        request_id: request_id.to_owned(),
    }
}

fn outcome_uncertain(detail: impl Into<String>, request_id: &str) -> TeamOutcome {
    TeamOutcome::Uncertain {
        detail: detail.into(),
        request_id: request_id.to_owned(),
    }
}

/// The checked call: its command and its request id. A call that is not sent
/// gives the outcome instead: `confirm` without the person's confirmation,
/// `failed` for a request that cannot be made.
pub fn prepare(call: &TeamCall) -> Result<(Invocation, String), TeamOutcome> {
    let request_id = match &call.request_id {
        Some(id) => id.clone(),
        None => new_request_id(),
    };
    if !canonical_uuid(&request_id) {
        return Err(outcome_failed("Not a request id.", &request_id));
    }
    // A create names the team it makes; every other call names the team it
    // changes.
    if !crate::teams::read::valid_team(&call.team) {
        return Err(outcome_failed("Not a team id.", &request_id));
    }
    if let Err(detail) = call.action.check() {
        return Err(outcome_failed(detail, &request_id));
    }
    if !call.confirmed {
        if let Some(confirmation) = call.action.confirmation(&call.team_name) {
            return Err(TeamOutcome::Confirm { confirmation });
        }
    }
    let revision = if matches!(call.action, TeamAction::Create { .. }) {
        // A new team has no revision to compare: the store checks zero.
        0
    } else {
        match call.expected_revision {
            Some(revision) => revision,
            None => {
                return Err(outcome_failed(
                    "The team's revision is not known; read the team first.",
                    &request_id,
                ))
            }
        }
    };
    let mut input = call.action.input();
    input.insert("team".into(), json!(call.team));
    input.insert("expected_revision".into(), json!(revision));
    input.insert("request_id".into(), json!(request_id));
    let mut args = vec!["team".to_owned(), call.action.op().to_owned()];
    if let TeamAction::Create { template, .. } = &call.action {
        args.push("--template".to_owned());
        args.push(template.clone());
    }
    args.push("--json".to_owned());
    Ok((
        Invocation {
            args,
            stdin: Some(Value::Object(input).to_string().into_bytes()),
            timeout: TIMEOUT,
            request_id: Some(request_id.clone()),
        },
        request_id,
    ))
}

fn stderr_text(stderr: &str) -> Option<String> {
    let text: String = stderr.trim().chars().take(DETAIL_CHARS).collect();
    (!text.is_empty()).then_some(text)
}

/// The refusal an `hgs` reply carries, when it carries one.
fn refusal(stdout: &[u8]) -> Option<(String, String)> {
    let reply: Value = serde_json::from_slice(stdout).ok()?;
    if reply.get("ok").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    let error = reply.get("error")?;
    let code = error.get("code").and_then(Value::as_str)?.to_owned();
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(DETAIL_CHARS)
        .collect();
    Some((code, message))
}

const READ_FIRST: &str = "Read the team before sending it again.";

/// The outcome of a call's exit. Only a refusal (an exit code) or a call that
/// never started is `failed`; everything that may have reached the store
/// without a valid receipt is `uncertain`.
pub fn classify(request_id: &str, exit: Exit) -> TeamOutcome {
    match exit {
        Exit::StartFailed(error) => {
            outcome_failed(format!("hgs did not start: {error}"), request_id)
        }
        Exit::TimedOut => outcome_uncertain(
            format!("hgs did not answer in time; the change may have been made. {READ_FIRST}"),
            request_id,
        ),
        Exit::WriteFailed => outcome_uncertain(
            format!(
                "The request did not reach hgs whole; the change may have been made. {READ_FIRST}"
            ),
            request_id,
        ),
        Exit::Abnormal { stderr } => outcome_uncertain(
            stderr_text(&stderr).map_or_else(
                || format!("hgs ended abnormally. {READ_FIRST}"),
                |text| format!("{text} {READ_FIRST}"),
            ),
            request_id,
        ),
        Exit::Code {
            code,
            stdout,
            stderr,
        } if code != 0 => match refusal(&stdout) {
            Some((code, message)) => TeamOutcome::Failed {
                detail: message,
                code,
                request_id: request_id.to_owned(),
            },
            None => outcome_failed(
                stderr_text(&stderr)
                    .or_else(|| stderr_text(&String::from_utf8_lossy(&stdout)))
                    .unwrap_or_else(|| format!("hgs exited with code {code}")),
                request_id,
            ),
        },
        Exit::Code { stdout, .. } => {
            if let Some((code, message)) = refusal(&stdout) {
                return TeamOutcome::Failed {
                    detail: message,
                    code,
                    request_id: request_id.to_owned(),
                };
            }
            let receipt: Value = serde_json::from_slice(&stdout).unwrap_or(Value::Null);
            if receipt.get("ok").and_then(Value::as_bool) != Some(true) {
                return outcome_uncertain(format!("hgs gave no receipt. {READ_FIRST}"), request_id);
            }
            TeamOutcome::Done {
                receipt,
                request_id: request_id.to_owned(),
            }
        }
    }
}

/// Runs one call through `hgs`: blocks for at most its timeout plus a few
/// seconds; callers run it off the UI thread. Never sends it twice.
pub fn run_action(hgs: &Path, call: &TeamCall) -> TeamOutcome {
    let (invocation, request_id) = match prepare(call) {
        Ok(prepared) => prepared,
        Err(outcome) => return outcome,
    };
    classify(&request_id, run(hgs, &invocation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_call(team: &str, revision: u64, action: TeamAction) -> TeamCall {
        TeamCall {
            team: team.to_owned(),
            team_name: "Fixes".into(),
            action,
            expected_revision: Some(revision),
            confirmed: false,
            request_id: Some("11111111-2222-4333-8444-555555555555".into()),
        }
    }

    fn stdin_of(invocation: &Invocation) -> Value {
        serde_json::from_slice(invocation.stdin.as_deref().unwrap()).unwrap()
    }

    const TEAM: &str = "00000000-0000-4000-8000-00000000beef";

    #[test]
    fn every_action_names_its_operation() {
        let cases: Vec<(TeamAction, &str)> = vec![
            (
                TeamAction::Create {
                    template: "bugfix".into(),
                    project: "p".into(),
                    name: "Fixes".into(),
                    goal: String::new(),
                    home: "00000000-0000-4000-8000-0000000000aa".into(),
                    integration_branch: "zerus/team/fixes".into(),
                    base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
                },
                "create",
            ),
            (TeamAction::Pause, "pause"),
            (TeamAction::Resume, "resume"),
            (TeamAction::Stop, "stop"),
            (TeamAction::Archive, "archive"),
            (
                TeamAction::PauseMember {
                    member: "m1".into(),
                },
                "pause-member",
            ),
            (
                TeamAction::ResumeMember {
                    member: "m1".into(),
                },
                "resume-member",
            ),
            (
                TeamAction::StopMember {
                    member: "m1".into(),
                },
                "stop-member",
            ),
            (
                TeamAction::ArchiveMember {
                    member: "m1".into(),
                },
                "archive-member",
            ),
            (
                TeamAction::Settings {
                    settings: Settings::default(),
                },
                "settings",
            ),
            (
                TeamAction::Decide {
                    card: 3,
                    decision: "accept".into(),
                },
                "decide",
            ),
            (TeamAction::Admit { node: 2 }, "admit"),
            (
                TeamAction::Route {
                    node: 2,
                    role: "developer".into(),
                    pipeline: "reviewed".into(),
                    deps: Vec::new(),
                    writes: Vec::new(),
                    assignee: None,
                },
                "route",
            ),
            (TeamAction::Accept { node: 2 }, "accept"),
            (
                TeamAction::Return {
                    node: 2,
                    reason: "Missing a test".into(),
                },
                "return",
            ),
            (
                TeamAction::Reassign {
                    node: 2,
                    member: "m2".into(),
                    reason: "M2 has the context".into(),
                },
                "reassign",
            ),
            (TeamAction::Hold { node: 2 }, "hold"),
            (TeamAction::Release { node: 2 }, "release"),
            (
                TeamAction::Cancel {
                    node: 2,
                    reason: "No longer needed".into(),
                },
                "cancel",
            ),
            (
                TeamAction::Reply {
                    item: 4,
                    text: "Go ahead".into(),
                },
                "reply",
            ),
            (
                TeamAction::Ask {
                    node: 2,
                    generation: 1,
                    text: "Which branch?".into(),
                },
                "ask",
            ),
        ];
        // `src/teams/cli/commands.rs` at package 5's fa46dcc08, plus the four
        // member commands its fix round adds.
        const COMMANDS: [&str; 21] = [
            "settings",
            "pause",
            "resume",
            "stop",
            "archive",
            "decide",
            "admit",
            "route",
            "accept",
            "return",
            "reassign",
            "hold",
            "release",
            "cancel",
            "reply",
            "ask",
            "create",
            "pause-member",
            "resume-member",
            "stop-member",
            "archive-member",
        ];
        for (action, op) in cases {
            assert_eq!(action.op(), op);
            assert!(COMMANDS.contains(&op), "{op} has no hgs team command");
            assert!(action.check().is_ok(), "{op}: {:?}", action.check());
        }
        assert!(
            TeamAction::Pause.names_team(),
            "a team action names its team"
        );
        assert!(
            !TeamAction::Create {
                template: "bugfix".into(),
                project: "p".into(),
                name: "Fixes".into(),
                goal: String::new(),
                home: "00000000-0000-4000-8000-0000000000aa".into(),
                integration_branch: "zerus/team/fixes".into(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
            }
            .names_team(),
            "a new team has no id yet"
        );
    }

    #[test]
    fn a_call_carries_its_identity_and_revision() {
        let call = a_call(TEAM, 42, TeamAction::Accept { node: 7 });
        let (invocation, request_id) = prepare(&call).unwrap();
        assert_eq!(request_id, "11111111-2222-4333-8444-555555555555");
        assert_eq!(invocation.args, ["team", "accept", "--json"]);
        assert_eq!(invocation.request_id.as_deref(), Some(request_id.as_str()));
        let stdin = stdin_of(&invocation);
        assert_eq!(stdin["request_id"], request_id);
        assert_eq!(stdin["team"], TEAM);
        assert_eq!(stdin["expected_revision"], 42);
        assert_eq!(stdin["node"], 7);
        assert!(
            stdin.get("by").is_none(),
            "the request never names its actor"
        );
    }

    #[test]
    fn a_new_call_gets_a_fresh_request_id() {
        let mut call = a_call(TEAM, 42, TeamAction::Hold { node: 1 });
        call.request_id = None;
        let (_, first) = prepare(&call).unwrap();
        let (_, second) = prepare(&call).unwrap();
        assert!(canonical_uuid(&first));
        assert!(canonical_uuid(&second));
        assert_ne!(first, second);
        call.request_id = Some("not-a-uuid".into());
        assert!(matches!(prepare(&call), Err(TeamOutcome::Failed { .. })));
    }

    #[test]
    fn a_person_confirms_a_destructive_action_first() {
        for action in [
            TeamAction::Stop,
            TeamAction::Archive,
            TeamAction::StopMember {
                member: "m1".into(),
            },
            TeamAction::ArchiveMember {
                member: "m1".into(),
            },
            TeamAction::Cancel {
                node: 3,
                reason: "Not needed".into(),
            },
        ] {
            let asked = prepare(&a_call(TEAM, 42, action.clone())).unwrap_err();
            let TeamOutcome::Confirm { confirmation } = asked else {
                panic!("{action:?} asks first");
            };
            assert!(!confirmation.text.is_empty());
            assert!(!confirmation.action.is_empty());
            assert!(confirmation.text.contains("Fixes"));
            let mut confirmed = a_call(TEAM, 42, action.clone());
            confirmed.confirmed = true;
            assert!(prepare(&confirmed).is_ok(), "{action:?} runs confirmed");
        }
        // An ordinary change asks nothing.
        assert!(prepare(&a_call(TEAM, 42, TeamAction::Pause)).is_ok());
    }

    #[test]
    fn a_change_needs_the_revision_it_was_read_at() {
        let mut call = a_call(TEAM, 42, TeamAction::Accept { node: 7 });
        call.expected_revision = None;
        let refused = prepare(&call).unwrap_err();
        assert!(matches!(refused, TeamOutcome::Failed { .. }));
        // A new team names its own id and compares revision zero.
        let new_team = "00000000-0000-4000-8000-00000000cafe";
        let create = TeamCall {
            team: new_team.into(),
            team_name: String::new(),
            action: TeamAction::Create {
                template: "bugfix".into(),
                project: "p".into(),
                name: "Fixes".into(),
                goal: "Ship it".into(),
                home: "00000000-0000-4000-8000-0000000000aa".into(),
                integration_branch: "zerus/team/fixes".into(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
            },
            expected_revision: None,
            confirmed: false,
            request_id: None,
        };
        let (invocation, _) = prepare(&create).unwrap();
        assert_eq!(
            invocation.args,
            ["team", "create", "--template", "bugfix", "--json"]
        );
        let stdin = stdin_of(&invocation);
        assert_eq!(stdin["team"], new_team);
        assert_eq!(stdin["expected_revision"], 0);
        assert_eq!(stdin["project"], "p");
        assert_eq!(stdin["name"], "Fixes");
        assert_eq!(stdin["goal"], "Ship it");
        assert_eq!(stdin["home"], "00000000-0000-4000-8000-0000000000aa");
        assert_eq!(stdin["integration_branch"], "zerus/team/fixes");
        assert_eq!(
            stdin["base_commit"],
            "0123456789abcdef0123456789abcdef01234567"
        );
    }

    #[test]
    fn a_reason_or_a_reply_must_say_something() {
        // The store refuses blank text for every one of these.
        for action in [
            TeamAction::Return {
                node: 1,
                reason: "   ".into(),
            },
            TeamAction::Cancel {
                node: 1,
                reason: String::new(),
            },
            TeamAction::Reassign {
                node: 1,
                member: "m1".into(),
                reason: String::new(),
            },
            TeamAction::Reply {
                item: 1,
                text: "\n".into(),
            },
            TeamAction::Ask {
                node: 1,
                generation: 1,
                text: "  ".into(),
            },
            TeamAction::Decide {
                card: 1,
                decision: String::new(),
            },
        ] {
            assert!(action.check().is_err(), "{action:?} must say something");
        }
        // The store's own text bound.
        let long = "x".repeat(TEXT_CHARS + 1);
        assert!(TeamAction::Reply {
            item: 1,
            text: long
        }
        .check()
        .is_err());
        assert!(TeamAction::Reply {
            item: 1,
            text: "x".repeat(TEXT_CHARS)
        }
        .check()
        .is_ok());
    }

    #[test]
    fn a_request_that_cannot_be_made_never_runs() {
        let mut call = a_call(TEAM, 42, TeamAction::Accept { node: 0 });
        call.request_id = Some("11111111-2222-4333-8444-555555555555".into());
        assert!(matches!(prepare(&call), Err(TeamOutcome::Failed { .. })));
        let mut call = a_call("has space", 42, TeamAction::Accept { node: 1 });
        call.request_id = Some("11111111-2222-4333-8444-555555555555".into());
        assert!(matches!(prepare(&call), Err(TeamOutcome::Failed { .. })));
        let mut call = a_call(
            TEAM,
            42,
            TeamAction::Return {
                node: 1,
                reason: "   ".into(),
            },
        );
        call.request_id = Some("11111111-2222-4333-8444-555555555555".into());
        assert!(matches!(prepare(&call), Err(TeamOutcome::Failed { .. })));
        let mut call = a_call(
            TEAM,
            42,
            TeamAction::Route {
                node: 1,
                role: "developer".into(),
                pipeline: "reviewed".into(),
                deps: Vec::new(),
                writes: vec!["/absolute/path".into()],
                assignee: None,
            },
        );
        call.request_id = Some("11111111-2222-4333-8444-555555555555".into());
        assert!(
            matches!(prepare(&call), Err(TeamOutcome::Failed { .. })),
            "a write set is relative to the repository"
        );
    }

    #[test]
    fn an_outcome_of_each_kind() {
        let id = "11111111-2222-4333-8444-555555555555";
        let done = classify(
            id,
            Exit::Code {
                code: 0,
                stdout: br#"{"ok":true,"revision":43,"node":7}"#.to_vec(),
                stderr: String::new(),
            },
        );
        assert_eq!(
            done,
            TeamOutcome::Done {
                receipt: json!({"ok": true, "revision": 43, "node": 7}),
                request_id: id.into(),
            }
        );
        assert_eq!(done.request_id(), Some(id));

        let failed = classify(
            id,
            Exit::Code {
                code: 1,
                stdout:
                    br#"{"ok":false,"error":{"code":"stale_revision","message":"The team moved."}}"#
                        .to_vec(),
                stderr: String::new(),
            },
        );
        assert_eq!(
            failed,
            TeamOutcome::Failed {
                detail: "The team moved.".into(),
                code: "stale_revision".into(),
                request_id: id.into(),
            }
        );

        let uncertain = classify(id, Exit::TimedOut);
        assert!(matches!(uncertain, TeamOutcome::Uncertain { .. }));
        assert!(!uncertain.repeatable_by_desktop());
        assert!(failed.repeatable_by_desktop());
        let abnormal = classify(
            id,
            Exit::Abnormal {
                stderr: "boom".into(),
            },
        );
        assert!(matches!(abnormal, TeamOutcome::Uncertain { .. }));
        let no_receipt = classify(
            id,
            Exit::Code {
                code: 0,
                stdout: b"{}".to_vec(),
                stderr: String::new(),
            },
        );
        assert!(matches!(no_receipt, TeamOutcome::Uncertain { .. }));
        let start_failed = classify(id, Exit::StartFailed("no hgs".into()));
        assert!(matches!(start_failed, TeamOutcome::Failed { .. }));
        assert!(start_failed.repeatable_by_desktop());
        let worded = classify(
            id,
            Exit::Code {
                code: 2,
                stdout: Vec::new(),
                stderr: "hgs is not installed".into(),
            },
        );
        assert_eq!(
            worded,
            TeamOutcome::Failed {
                detail: "hgs is not installed".into(),
                code: String::new(),
                request_id: id.into(),
            }
        );

        let confirmed = prepare(&a_call(TEAM, 42, TeamAction::Stop)).unwrap_err();
        assert!(matches!(confirmed, TeamOutcome::Confirm { .. }));
        assert_eq!(confirmed.request_id(), None);
        assert!(confirmed.repeatable_by_desktop());
    }

    #[test]
    fn an_uncertain_call_is_never_repeated_by_the_desktop() {
        let id = "11111111-2222-4333-8444-555555555555";
        for exit in [
            Exit::TimedOut,
            Exit::WriteFailed,
            Exit::Abnormal {
                stderr: String::new(),
            },
            Exit::Code {
                code: 0,
                stdout: b"not json".to_vec(),
                stderr: String::new(),
            },
        ] {
            let outcome = classify(id, exit);
            assert!(matches!(outcome, TeamOutcome::Uncertain { .. }));
            assert!(
                !outcome.repeatable_by_desktop(),
                "only the person repeats an uncertain call"
            );
            assert_eq!(
                outcome.request_id(),
                Some(id),
                "the person repeats it with the same id"
            );
        }
    }

    #[test]
    fn a_call_is_deserialized_without_naming_its_actor() {
        let parsed: TeamCall = serde_json::from_value(json!({
            "team": TEAM,
            "team_name": "Fixes",
            "expected_revision": 42,
            "confirmed": false,
            "action": {"kind": "return", "node": 2, "reason": "Add a test"},
        }))
        .unwrap();
        assert_eq!(
            parsed.action,
            TeamAction::Return {
                node: 2,
                reason: "Add a test".into()
            }
        );
        assert_eq!(parsed.expected_revision, Some(42));
        assert!(serde_json::from_value::<TeamCall>(json!({
            "team": TEAM,
            "action": {"kind": "accept", "node": 2},
            "actor": {"kind": "service"},
        }))
        .is_err());
    }
}
