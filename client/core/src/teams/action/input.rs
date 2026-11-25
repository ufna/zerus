//! What an action sends on standard input and the checks it must pass before
//! anything runs: the store's own bounds for text, nodes and write paths, and
//! the confirmation a destructive action needs.
use super::types::*;
use crate::{
    source, text,
    text::{Arg, Text},
};
use serde_json::{json, Map, Value};
use zerus_team_plan::model::{Integration, NodeId};

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
            Self::SetPhaseBarrier { .. } => "set-phase-barrier",
            Self::PausePhase { .. } => "pause-phase",
            Self::ResumePhase { .. } => "resume-phase",
            Self::MovePhase { .. } => "move-phase",
            Self::SetPhaseAcceptance { .. } => "set-phase-acceptance",
            Self::SendBackPhase { .. } => "send-back-phase",
            Self::SubmitPhaseCorrections { .. } => "submit-phase-corrections",
            Self::EditPhaseCorrections { .. } => "edit-phase-corrections",
            Self::ApprovePhaseCorrections { .. } => "approve-phase-corrections",
            Self::RejectPhaseCorrections { .. } => "reject-phase-corrections",
            Self::UndoPhaseCorrectionRejection { .. } => "undo-phase-correction-rejection",
            Self::ApprovePhase { .. } => "approve-phase",
            Self::RejectPhase { .. } => "reject-phase",
            Self::UndoRejectPhase { .. } => "undo-reject-phase",
            Self::AcceptPhase { .. } => "accept-phase",
            Self::StartPlanning { .. } => "start-planning",
            Self::SubmitDraftPhase { .. } => "submit-draft-phase",
            Self::EditDraftPhase { .. } => "edit-draft-phase",
            Self::AddMember { .. } => "add-member",
            Self::SwapMember { .. } => "swap-member",
            Self::RenameTeam { .. } => "rename-team",
            Self::SetGoal { .. } => "set-goal",
            Self::Create { .. } => "create",
            Self::Pause {} => "pause",
            Self::Resume {} => "resume",
            Self::Stop {} => "stop",
            Self::Archive {} => "archive",
            Self::AdvanceBase { .. } => "advance-base",
            Self::PauseMember { .. } => "pause-member",
            Self::ResumeMember { .. } => "resume-member",
            Self::StopMember { .. } => "stop-member",
            Self::ArchiveMember { .. } => "archive-member",
            Self::Settings { .. } => "settings",
            Self::ValidationConfig { .. } => "validation-settings",
            Self::Decide { .. } => "decide",
            Self::Admit { .. } => "admit",
            Self::Route { .. } => "route",
            Self::SetDeps { .. } => "set-deps",
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
        !matches!(self, Self::Create { .. } | Self::StartPlanning { .. })
    }

    /// The fields the operation sends besides the envelope's.
    pub fn input(&self) -> Map<String, Value> {
        let value = match self {
            Self::SetPhaseBarrier { scope, barrier } => json!({"scope":scope,"barrier":barrier}),
            Self::PausePhase {
                scope,
                pause,
                reason,
            } => json!({"scope":scope,"pause":pause,"reason":reason}),
            Self::ResumePhase { scope, pause } => json!({"scope":scope,"pause":pause}),
            Self::MovePhase {
                scope,
                relative_to,
                placement,
            } => json!({"scope":scope,"relative_to":relative_to,"placement":placement}),
            Self::SetPhaseAcceptance { scope, acceptance } => {
                json!({"scope":scope,"acceptance":acceptance})
            }
            Self::SendBackPhase { .. }
            | Self::SubmitPhaseCorrections { .. }
            | Self::EditPhaseCorrections { .. }
            | Self::ApprovePhaseCorrections { .. }
            | Self::RejectPhaseCorrections { .. }
            | Self::UndoPhaseCorrectionRejection { .. } => {
                let mut value =
                    serde_json::to_value(self).expect("typed correction action serializes");
                value.as_object_mut().expect("action object").remove("kind");
                value
            }
            Self::ApprovePhase {
                phase,
                generation,
                confirm_initial,
            } => json!({"phase":phase,"generation":generation,"confirm_initial":confirm_initial}),
            Self::RejectPhase {
                phase,
                generation,
                comment,
            } => json!({"phase":phase,"generation":generation,"comment":comment}),
            Self::UndoRejectPhase {
                phase,
                generation,
                rejection,
            } => json!({"phase":phase,"generation":generation,"rejection":rejection}),
            Self::AcceptPhase { phase, publication } => {
                json!({"phase":phase,"publication":publication})
            }
            Self::SubmitDraftPhase { phase, draft } => json!({"phase":phase,"draft":draft}),
            Self::EditDraftPhase {
                phase,
                generation,
                draft,
            } => json!({"phase":phase,"generation":generation,"draft":draft}),
            Self::Create {
                project,
                name,
                goal,
                home,
                integration_branch,
                base_commit,
                expected_template,
                delegations,
                ..
            }
            | Self::StartPlanning {
                project,
                name,
                goal,
                home,
                integration_branch,
                base_commit,
                expected_template,
                delegations,
                ..
            } => json!({
                "project": project,
                "name": name,
                "goal": goal,
                "home": home,
                "integration_branch": integration_branch,
                "base_commit": base_commit,
                "expected_template": expected_template,
                "delegations": delegations,
            }),
            Self::AddMember {
                role,
                agent,
                model,
                effort,
                account,
            } => json!({"role":role,"agent":agent,"model":model,"effort":effort,"account":account}),
            Self::SwapMember {
                member,
                agent,
                model,
                effort,
                account,
            } => {
                json!({"member":member,"agent":agent,"model":model,"effort":effort,"account":account})
            }
            Self::RenameTeam { name } => json!({"name":name}),
            Self::SetGoal { goal } => json!({"goal":goal}),
            Self::Pause {} | Self::Resume {} | Self::Stop {} | Self::Archive {} => json!({}),
            Self::PauseMember { member }
            | Self::ResumeMember { member }
            | Self::StopMember { member }
            | Self::ArchiveMember { member } => json!({"member": member}),
            Self::ValidationConfig { selection } => json!({"selection":selection}),
            Self::Settings { settings } => json!({"settings": settings}),
            Self::AdvanceBase { commit } => json!({"commit": commit}),
            Self::Decide { card, decision } => json!({"card": card, "decision": decision}),
            Self::Admit { node } => json!({"node": node}),
            Self::Route {
                node,
                role,
                pipeline,
                keep_task_fields,
                deps,
                writes,
                assignee,
            } => {
                let mut input = json!({"node": node, "role": role, "pipeline": pipeline});
                if *keep_task_fields {
                    input["keep_task_fields"] = json!(true);
                } else {
                    input["deps"] = json!(deps);
                    input["writes"] = json!(writes);
                    if let Some(assignee) = assignee {
                        input["assignee"] = json!(assignee);
                    }
                }
                input
            }
            Self::SetDeps { node, deps } => json!({"node": node, "deps": deps}),
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

    /// Preserve the shared settings validator's structured refusal for the UI.
    pub(super) fn settings_problem(&self) -> Option<zerus_team_plan::Problem> {
        match self {
            Self::Settings { settings } => {
                zerus_team_plan::settings::validate_settings(settings).err()
            }
            _ => None,
        }
    }

    /// Checks what the call sends before anything runs.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::SetPhaseBarrier { .. }
            | Self::PausePhase { .. }
            | Self::ResumePhase { .. }
            | Self::MovePhase { .. }
            | Self::SetPhaseAcceptance { .. } => super::phase_controls::check(self),
            Self::SendBackPhase { .. }
            | Self::SubmitPhaseCorrections { .. }
            | Self::EditPhaseCorrections { .. }
            | Self::ApprovePhaseCorrections { .. }
            | Self::RejectPhaseCorrections { .. }
            | Self::UndoPhaseCorrectionRejection { .. } => super::rework::check(self),
            Self::ApprovePhase { .. }
            | Self::RejectPhase { .. }
            | Self::UndoRejectPhase { .. }
            | Self::AcceptPhase { .. } => super::phases::check(self),
            Self::SubmitDraftPhase { phase, draft } => super::drafts::check(*phase, draft),
            Self::EditDraftPhase {
                phase,
                generation,
                draft,
            } => {
                if *generation == 0 {
                    return Err("The draft's generation is not known; read it again.".into());
                }
                super::drafts::check(*phase, draft)
            }
            Self::AddMember { .. }
            | Self::SwapMember { .. }
            | Self::RenameTeam { .. }
            | Self::SetGoal { .. } => super::editing::check(self),
            Self::Create { .. } | Self::StartPlanning { .. } => check_create(self),
            Self::AdvanceBase { commit } => {
                if matches!(commit.len(), 40 | 64)
                    && commit
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    Ok(())
                } else {
                    Err("Name the full lowercase commit hash.".into())
                }
            }
            Self::Pause {} | Self::Resume {} | Self::Stop {} | Self::Archive {} => Ok(()),
            Self::ValidationConfig { selection } => selection.check(),
            Self::Settings { settings } => zerus_team_plan::settings::validate_settings(settings)
                .map_err(|problem| problem.message),
            Self::PauseMember { member }
            | Self::ResumeMember { member }
            | Self::StopMember { member }
            | Self::ArchiveMember { member } => word(member, "member"),
            Self::Decide { card: id, decision } => check_card(*id, decision),
            Self::Admit { node: id }
            | Self::Accept { node: id }
            | Self::Hold { node: id }
            | Self::Release { node: id } => node(*id),
            Self::SetDeps { node: id, deps } => {
                node(*id)?;
                if deps.len() > 200 {
                    return Err("Too many dependencies.".into());
                }
                for dep in deps {
                    node(dep.node)?;
                }
                Ok(())
            }
            Self::Route {
                node: id,
                role,
                pipeline,
                keep_task_fields,
                deps,
                writes,
                assignee,
            } => {
                if *keep_task_fields
                    && (!deps.is_empty() || !writes.is_empty() || assignee.is_some())
                {
                    return Err("A placement edit cannot also change the task's dependencies, write paths or assignee.".into());
                }
                check_route(*id, role, pipeline, writes, assignee.as_deref())
            }
            Self::Return {
                node: id,
                reason: why,
            } => check_return(*id, why),
            Self::Reassign {
                node: id,
                member,
                reason: why,
            } => check_reassign(*id, member, why),
            Self::Cancel {
                node: id,
                reason: why,
            } => check_cancel(*id, why),
            Self::Reply { item, text } => check_reply(*item, text),
            Self::Ask {
                node: id,
                generation: _,
                text,
            } => check_ask(*id, text),
        }
    }

    /// What the person is asked before a destructive action. The sentence is
    /// the native prompt's own (`confirmation_text`), carried as `message` so
    /// the window renders the same catalog sentence in its language; `text` is
    /// its English. The title and button are catalog sources too.
    pub fn confirmation(&self, team_name: &str) -> Option<Confirmation> {
        let name = if team_name.trim().is_empty() {
            Arg::from(text!("this team"))
        } else {
            Arg::from(team_name.trim())
        };
        let (dialog, title, action) = match self {
            Self::AdvanceBase { .. } => (
                "teamAdvanceBaseConfirm",
                source!("Advance base?"),
                source!("Advance base"),
            ),
            Self::SwapMember { .. } => (
                "teamSwapMemberConfirm",
                source!("Swap member?"),
                source!("Swap member"),
            ),
            Self::Stop {} => (
                "teamStopConfirm",
                source!("Stop team?"),
                source!("Stop team"),
            ),
            Self::Archive {} => ("teamArchiveConfirm", source!("Archive team?"), "Archive"),
            Self::StopMember { .. } => (
                "teamStopMemberConfirm",
                source!("Stop member?"),
                source!("Stop member"),
            ),
            Self::ArchiveMember { .. } => (
                "teamArchiveMemberConfirm",
                source!("Remove member?"),
                source!("Remove member"),
            ),
            Self::Cancel { .. } => (
                "teamCancelConfirm",
                source!("Cancel task?"),
                source!("Cancel task"),
            ),
            _ => return None,
        };
        let message = self.confirmation_text(name, None)?;
        Some(Confirmation {
            name: dialog.into(),
            title: title.into(),
            action: action.into(),
            text: message.english(),
            message: Box::new(message),
        })
    }

    /// Common summary for the window and the native dialog. The native
    /// caller can supply the target's label from its verified review.
    pub(crate) fn confirmation_text(&self, team: Arg, target: Option<Text>) -> Option<Text> {
        let target = |fallback| target.unwrap_or_else(|| Text::verbatim(fallback));
        Some(match self {
            Self::AdvanceBase { commit } => text!(
                "Advance the base of %1 to %2? Future hand-outs use it. Earlier hand-outs keep their assigned base. Branches are unchanged.",
                team, target(commit.clone())
            ),
            Self::SwapMember { member, .. } => text!(
                "Replace the agent for %1 of %2? The old session stays outside the team. The member keeps its ID, task history, folder and branch. Only a person-stopped member with no open or unsettled work can be swapped.",
                target(member.clone()), team
            ),
            Self::Stop {} => text!(
                "Stop %1? Every open task is cancelled and no task is handed out again. Member sessions keep running; their current turns finish. A stopped team cannot be started again; use Pause to stop it for a while.",
                team
            ),
            Self::Archive {} => text!(
                "Archive %1? Its history, tasks and reports stay. The list of teams shows it under Show archived. Archiving cannot be undone.",
                team
            ),
            Self::StopMember { member } => text!(
                "Stop %1 of %2? Its current turn finishes and it takes no new task.",
                target(member.clone()), team
            ),
            Self::ArchiveMember { member } => text!(
                "Remove %1 from %2? Its history and reports stay. Removing a member cannot be undone; add a new member to take its place.",
                target(member.clone()), team
            ),
            Self::Cancel { node, .. } => text!(
                "Cancel task #%1 of %2? Its work stops and its result is not accepted.",
                target(node.to_string()), team
            ),
            _ => return None,
        })
    }
}

fn check_create(action: &TeamAction) -> Result<(), String> {
    let (TeamAction::Create {
        template,
        project,
        name,
        goal,
        home,
        integration_branch,
        base_commit,
        integration,
        expected_template,
        ..
    }
    | TeamAction::StartPlanning {
        template,
        project,
        name,
        goal,
        home,
        integration_branch,
        base_commit,
        integration,
        expected_template,
        ..
    }) = action
    else {
        return Err("Not a create.".into());
    };
    word(template, "template")?;
    if matches!(action, TeamAction::StartPlanning { .. }) && goal.trim().is_empty() {
        return Err("Name the planning goal.".into());
    }
    // The digest is opaque: the catalog read gave it and it is sent back as
    // it came; a create never goes without one.
    word(expected_template, "template digest")?;
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
    // The store requires the integration branch only for a template that
    // integrates on a branch.
    match integration {
        Integration::Branch => {
            word(integration_branch, "branch")?;
            word(base_commit, "commit")
        }
        Integration::None => {
            for (value, what) in [(integration_branch, "branch"), (base_commit, "commit")] {
                if !value.is_empty() {
                    word(value, what)?;
                }
            }
            Ok(())
        }
    }
}

fn check_card(id: u64, decision: &str) -> Result<(), String> {
    card(id)?;
    reason(decision, "A decision")
}

fn check_return(id: NodeId, why: &str) -> Result<(), String> {
    node(id)?;
    reason(why, "A return's reason")
}

fn check_cancel(id: NodeId, why: &str) -> Result<(), String> {
    node(id)?;
    reason(why, "A reason")
}

fn check_reply(item: u64, text: &str) -> Result<(), String> {
    card(item)?;
    reason(text, "A reply")
}

fn check_ask(id: NodeId, text: &str) -> Result<(), String> {
    node(id)?;
    reason(text, "A question")
}

fn check_route(
    id: NodeId,
    role: &str,
    pipeline: &str,
    writes: &[String],
    assignee: Option<&str>,
) -> Result<(), String> {
    node(id)?;
    word(role, "role")?;
    word(pipeline, "pipeline")?;
    if let Some(assignee) = assignee {
        word(assignee, "member")?;
    }
    // The store's own write-path rule.
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

fn check_reassign(id: NodeId, member: &str, why: &str) -> Result<(), String> {
    node(id)?;
    word(member, "member")?;
    reason(why, "A reason")
}
