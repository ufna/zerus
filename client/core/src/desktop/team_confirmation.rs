//! Native Teams consent text. Action and identity come from the validated
//! plan. The service supplies names from the same snapshot as its review.
use super::{team_person, team_person_actions as wire};
use crate::{teams::action::TeamAction, text, text::Text};
use serde::{Deserialize, Serialize};
use zerus_team_plan::{
    model::{Acceptance, Dep, DepKind, Integration, NodeKind},
    phase_control::PhasePlacement,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub facts: String,
    pub task: Option<TaskLabel>,
    pub phase: Option<String>,
    pub relative_phase: Option<String>,
    pub member: Option<String>,
    pub role: Option<String>,
    pub pipeline: Option<String>,
    pub project: Option<String>,
    pub template: Option<String>,
    pub branch: Option<String>,
    pub computer: Option<String>,
    pub card_kind: Option<String>,
    pub kept_route: Option<KeptRoute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptRoute {
    pub deps: Vec<Dep>,
    pub writes: Vec<String>,
    pub assignment: Option<KeptAssignment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptAssignment {
    pub member: String,
    pub cleared: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLabel {
    pub number: String,
    pub title: String,
    pub stage: bool,
}

pub struct Prompt {
    pub title: Text,
    pub body: Vec<Text>,
}

impl Prompt {
    fn new(title: Text, sentence: Text) -> Self {
        Self {
            title,
            body: vec![sentence],
        }
    }

    fn with(mut self, line: Text) -> Self {
        self.body.push(line);
        self
    }

    fn reason(self, reason: &str) -> Self {
        if reason.trim().is_empty() {
            self
        } else {
            self.with(text!("Reason — “%1”", visible(reason)))
        }
    }

    fn comment(self, comment: &str) -> Self {
        if comment.trim().is_empty() {
            self
        } else {
            self.with(text!("Comment — “%1”", visible(comment)))
        }
    }
}

/// Keep reviewed text within its own line and quote delimiters. Control and
/// bidirectional formatting marks have no role in a consent caption.
fn visible(value: &str) -> String {
    let mut display = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\n' | '\r' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}' => display.push(' '),
            '\\' | '"' | '“' | '”' | '«' | '»' => {
                display.push('\\');
                display.push(c);
            }
            _ if !c.is_control()
                && !matches!(c, '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                display.push(c)
            }
            _ => {}
        }
    }
    display
}

fn name(value: Option<&str>, fallback: &str) -> String {
    visible(value.filter(|v| !v.is_empty()).unwrap_or(fallback))
}

fn task(labels: &Labels, node: Option<u32>) -> Text {
    match (&labels.task, node) {
        (Some(label), _) if label.stage => text!(
            "stage #%1 “%2”",
            label.number.as_str(),
            visible(&label.title)
        ),
        (Some(label), _) => text!(
            "task #%1 “%2”",
            label.number.as_str(),
            visible(&label.title)
        ),
        (None, Some(node)) => text!("task #%1", i64::from(node)),
        _ => text!("this item"),
    }
}

fn phase(number: u32, title: Option<&str>) -> Text {
    match title.filter(|v| !v.is_empty()) {
        Some(title) => text!("phase %1 “%2”", i64::from(number), visible(title)),
        None => text!("phase %1", i64::from(number)),
    }
}

fn member(labels: &Labels, id: &str) -> Text {
    match labels.member.as_deref().filter(|v| !v.is_empty()) {
        Some(label) if !crate::teams::action::canonical_uuid(id) => {
            text!("%1 (%2)", visible(label), visible(id))
        }
        Some(label) => Text::verbatim(visible(label)),
        None if crate::teams::action::canonical_uuid(id) => text!("the selected member"),
        None => Text::verbatim(visible(id)),
    }
}

fn acceptance(value: Acceptance) -> Text {
    match value {
        Acceptance::Lead => text!("the team lead"),
        Acceptance::Person => text!("you"),
        Acceptance::Pipeline => text!("the pipeline"),
    }
}

fn dependencies(deps: &[Dep]) -> impl Iterator<Item = Text> + '_ {
    deps.iter().map(|dep| match dep.kind {
        DepKind::Hard => text!(
            "Task #%1 must be accepted and merged first.",
            i64::from(dep.node)
        ),
        DepKind::Soft => text!("Task #%1 must start first.", i64::from(dep.node)),
    })
}

pub fn action(plan: &wire::Plan, review: &wire::Review, labels: &Labels) -> Option<Prompt> {
    if plan.reference().as_ref() != Some(&review.intent.plan)
        || review.intent.facts != labels.facts
        || !review.valid()
    {
        return None;
    }
    let team = visible(&review.team_name);
    let add = text!("Send task to team");
    let prompt = match &plan.action {
        wire::Action::Team { change } => team_action(change, &team, labels)?,
        wire::Action::NewTask { title, backlog, role, pipeline, .. } => {
            let stage = if *backlog {
                text!("Backlog")
            } else {
                text!("To do")
            };
            let mut result = Prompt::new(
                add,
                text!(
                    "Create task “%1” in %2 and send it to team “%3”.",
                    visible(title),
                    name(labels.project.as_deref(), ""),
                    team
                ),
            )
            .with(text!("The new task starts in %1.", stage));
            result.body.extend(placement(role.as_deref(), pipeline.as_deref(), labels));
            result
        }
        wire::Action::Intake { role, pipeline, .. } => {
            let label = labels.task.as_ref()?;
            let mut result = Prompt::new(
                add,
                text!(
                    "Send %1 from %2 to team “%3”.",
                    task(labels, None),
                    name(labels.project.as_deref(), ""),
                    team
                ),
            );
            if label.stage {
                return None;
            }
            result
                .body
                .extend(placement(role.as_deref(), pipeline.as_deref(), labels));
            result
        }
    };
    Some(prompt)
}

pub fn card(review: &team_person::Review, labels: &Labels) -> Option<Prompt> {
    if review.intent.facts != labels.facts || !review.valid() {
        return None;
    }
    card_action(
        &review.team_name,
        &review.card_kind,
        &review.intent.plan.decision,
        labels,
    )
}

fn card_action(team: &str, kind: &str, decision: &str, labels: &Labels) -> Option<Prompt> {
    let team = visible(team);
    let related = task(labels, None);
    let mut prompt = match (kind, decision) {
        ("integration", "approve") => Prompt::new(
            text!("Approve merge"),
            text!(
                "Approve merging the result of %1 for team “%2”.",
                related,
                team
            ),
        )
        .with(text!("A completed merge must be reverted separately.")),
        ("integration", "reject") => Prompt::new(
            text!("Reject merge"),
            text!(
                "Reject the requested merge of %1 for team “%2”.",
                related,
                team
            ),
        )
        .with(text!(
            "This decision cannot undo a merge that already completed."
        )),
        ("pipeline_acceptance", "approve") => Prompt::new(
            text!("Accept work"),
            text!("Accept the work on %1 for team “%2”.", related, team),
        ),
        ("pipeline_acceptance", "reject") => Prompt::new(
            text!("Decline acceptance"),
            text!(
                "Decline acceptance of %1 and send it back to the lead of team “%2”.",
                related,
                team
            ),
        ),
        ("launch_blocked", "retry") => Prompt::new(
            text!("Retry launch"),
            text!(
                "Retry starting %1 for team “%2”.",
                labels.member.as_deref().map_or_else(
                    || text!("the member"),
                    |label| Text::verbatim(visible(label))
                ),
                team
            ),
        ),
        (_, "approve") => Prompt::new(
            text!("Approve"),
            text!("Approve the request about %1 for team “%2”.", related, team),
        ),
        (_, "reject") => Prompt::new(
            text!("Reject"),
            text!("Reject the request about %1 for team “%2”.", related, team),
        ),
        (_, "retry") if labels.task.is_some() => Prompt::new(
            text!("Retry"),
            text!(
                "Retry the requested action for %1 in team “%2”.",
                related,
                team
            ),
        ),
        (_, "retry") => Prompt::new(
            text!("Retry"),
            text!(
                "Retry the action described by this notice for team “%1”.",
                team
            ),
        ),
        (_, "acknowledged") if labels.task.is_some() => Prompt::new(
            text!("Acknowledge"),
            text!(
                "Acknowledge and close the notice about %1 for team “%2”.",
                related,
                team
            ),
        ),
        (_, "acknowledged") if labels.member.is_some() => Prompt::new(
            text!("Acknowledge"),
            text!(
                "Acknowledge and close the notice about %1 for team “%2”.",
                name(labels.member.as_deref(), ""),
                team
            ),
        ),
        (_, "acknowledged") => Prompt::new(
            text!("Acknowledge"),
            text!("Acknowledge and close this notice for team “%1”.", team),
        ),
        _ => return None,
    };
    if kind == "integration" && decision == "approve" {
        if let Some(branch) = labels.branch.as_deref().filter(|v| !v.is_empty()) {
            prompt = prompt.with(text!("The merge target is %1.", visible(branch)));
        }
    }
    Some(prompt.with(text!(
        "This decision closes the notice and cannot be undone."
    )))
}

fn placement(role: Option<&str>, pipeline: Option<&str>, labels: &Labels) -> Vec<Text> {
    let mut result = Vec::new();
    if let Some(role) = role.filter(|v| !v.is_empty()) {
        result.push(text!("The role is %1.", name(labels.role.as_deref(), role)));
    }
    if let Some(pipeline) = pipeline.filter(|v| !v.is_empty()) {
        result.push(text!(
            "The pipeline is %1.",
            name(labels.pipeline.as_deref(), pipeline)
        ));
    }
    result
}

fn team_action(change: &TeamAction, team: &str, labels: &Labels) -> Option<Prompt> {
    use TeamAction::*;
    let p = |number| phase(number, labels.phase.as_deref());
    let t = |node| task(labels, Some(node));
    let m = |id: &str| member(labels, id);
    let shared = |target| change.confirmation_text(team.into(), target);
    let mut prompt = match change {
        Create { name: new, .. } => Prompt::new(
            text!("Create team"),
            text!("Create team “%1”.", visible(new)),
        ),
        StartPlanning { name: new, .. } => Prompt::new(
            text!("Start planning"),
            text!(
                "Create team “%1” and start planning with its lead.",
                visible(new)
            ),
        ),
        RenameTeam { name: new } => Prompt::new(
            text!("Rename team"),
            text!("Rename team “%1” to “%2”.", team, visible(new)),
        ),
        SetGoal { goal } => Prompt::new(
            text!("Change goal"),
            text!("Set the goal of team “%1” to “%2”.", team, visible(goal)),
        ),
        Pause {} => Prompt::new(
            text!("Pause team"),
            text!(
                "Pause team “%1” so it stops handing out work until resumed.",
                team
            ),
        ),
        Resume {} => Prompt::new(
            text!("Resume team"),
            text!("Resume handing out work for team “%1”.", team),
        ),
        Stop {} => Prompt::new(text!("Stop team"), shared(None)?),
        Archive {} => Prompt::new(text!("Archive team"), shared(None)?),
        AdvanceBase { commit } => Prompt::new(
            text!("Advance base"),
            shared(Some(Text::verbatim(short_commit(commit))))?,
        )
        .with(text!(
            "This advance cannot be reversed by moving the base back."
        )),
        AddMember { role, .. } => Prompt::new(
            text!("Add member"),
            text!(
                "Add a member for the %1 role in team “%2”.",
                name(labels.role.as_deref(), role),
                team
            ),
        ),
        SwapMember { member, .. } => Prompt::new(text!("Swap member"), shared(Some(m(member)))?)
            .with(text!("The replacement starts with a new conversation.")),
        PauseMember { member } => Prompt::new(
            text!("Pause member"),
            text!(
                "Pause %1 so team “%2” does not give them more work until resumed.",
                m(member),
                team
            ),
        ),
        ResumeMember { member } => Prompt::new(
            text!("Resume member"),
            text!("Let team “%2” give work to %1 again.", m(member), team),
        ),
        StopMember { member } => Prompt::new(text!("Stop member"), shared(Some(m(member)))?),
        ArchiveMember { member } => Prompt::new(text!("Remove"), shared(Some(m(member)))?)
            .with(text!("Unfinished work returns to the lead.")),
        ValidationConfig { selection } => Prompt::new(
            text!("Tests and Builds"),
            text!(
                "Save the reviewed Tests and Builds settings for team “%1”.",
                team
            ),
        )
        .with(if selection.override_config.is_some() {
            text!("Use the selected provider overrides.")
        } else {
            text!("Use the providers detected for this checkout.")
        })
        .with(text!("Saving these settings does not run tests or builds.")),
        Settings { .. } => Prompt::new(
            text!("Save team settings"),
            text!(
                "Save the reviewed settings and permissions for team “%1”.",
                team
            ),
        ),
        Admit { node } => Prompt::new(
            text!("Admit"),
            text!(
                "Admit %1 so team “%2” can assign it when it is ready.",
                t(*node),
                team
            ),
        ),
        Route {
            node,
            role,
            pipeline,
            keep_task_fields,
            ..
        } => {
            let mut result = Prompt::new(
                text!("Route"),
                text!("Change the route for %1 in team “%2”.", t(*node), team),
            );
            result
                .body
                .extend(placement(Some(role), Some(pipeline), labels));
            if *keep_task_fields {
                let kept = labels.kept_route.as_ref()?;
                result = result.with(text!("Keep the task’s dependencies and write paths."));
                result.body.extend(dependencies(&kept.deps));
                write_scope(&mut result.body, &kept.writes);
                if let Some(assignment) = &kept.assignment {
                    let target = member(&Labels::default(), &assignment.member);
                    result = result.with(if assignment.cleared {
                        text!(
                            "Clear the assignment to %1. The task will be unassigned.",
                            target
                        )
                    } else {
                        text!("Keep the assignment to %1.", target)
                    });
                }
            }
            result
        }
        SetDeps { node, deps } => {
            let mut result = Prompt::new(
                text!("Change dependencies"),
                text!(
                    "Replace the dependencies of %1 in team “%2”.",
                    t(*node),
                    team
                ),
            );
            if deps.is_empty() {
                result = result.with(text!("The task will have no dependencies."));
            }
            result.body.extend(dependencies(deps));
            result
        }
        Accept { node } => Prompt::new(
            text!("Accept"),
            text!("Accept the result of %1 for team “%2”.", t(*node), team),
        )
        .with(text!(
            "If this team merges accepted work, a completed merge must be reverted separately."
        )),
        Return { node, reason } => Prompt::new(
            text!("Return"),
            text!(
                "Return %1 for another round of work in team “%2”.",
                t(*node),
                team
            ),
        )
        .reason(reason),
        Reassign {
            node,
            member,
            reason,
        } => Prompt::new(
            text!("Reassign"),
            text!("Reassign %1 to %2 in team “%3”.", t(*node), m(member), team),
        )
        .reason(reason),
        Hold { node } => Prompt::new(
            text!("Hold"),
            text!(
                "Hold %1 so team “%2” cannot hand it out until released.",
                t(*node),
                team
            ),
        ),
        Release { node } => Prompt::new(
            text!("Release"),
            text!(
                "Release %1 so team “%2” can hand it out when ready.",
                t(*node),
                team
            ),
        ),
        Cancel { node, reason } => {
            let summary = match &labels.task {
                Some(task) if task.stage => text!("Cancel %1 in team “%2”.", t(*node), team),
                Some(task) => shared(Some(text!(
                    "%1 “%2”",
                    visible(&task.number),
                    visible(&task.title)
                )))?,
                None => shared(None)?,
            };
            Prompt::new(text!("Cancel task"), summary)
                .reason(reason)
                .with(text!("The team cannot continue the cancelled attempt."))
        }
        Reply { text: reply, .. } => Prompt::new(
            text!("Reply"),
            if labels.task.is_some() {
                text!(
                    "Answer the member’s question about %1 in team “%2”.",
                    task(labels, None),
                    team
                )
            } else {
                text!("Answer the member’s question in team “%1”.", team)
            },
        )
        .with(text!("Reply — “%1”", visible(reply)))
        .with(text!("The reply cannot be recalled after delivery.")),
        SetPhaseBarrier { scope, barrier } => {
            Prompt::new(
                text!("Change phase barrier"),
                if *barrier {
                    text!(
                        "Make %1 in team “%2” wait for all earlier phases.",
                        p(scope.phase()),
                        team
                    )
                } else {
                    text!("Remove the requirement for %1 in team “%2” to wait for all earlier phases.", p(scope.phase()), team)
                },
            )
        }
        PausePhase {
            scope,
            pause,
            reason,
        } => Prompt::new(
            text!("Pause phase"),
            if pause.is_some() {
                text!(
                    "Update the pause on %1 in team “%2”.",
                    p(scope.phase()),
                    team
                )
            } else {
                text!(
                    "Pause %1 so team “%2” stops handing out its work.",
                    p(scope.phase()),
                    team
                )
            },
        )
        .reason(reason),
        ResumePhase { scope, .. } => Prompt::new(
            text!("Resume phase"),
            text!(
                "Resume handing out work for %1 in team “%2”.",
                p(scope.phase()),
                team
            ),
        ),
        MovePhase {
            scope,
            relative_to,
            placement,
        } => {
            let other = phase(relative_to.phase(), labels.relative_phase.as_deref());
            Prompt::new(
                text!("Move phase"),
                match placement {
                    PhasePlacement::Before => text!(
                        "Move %1 before %2 in team “%3”.",
                        p(scope.phase()),
                        other,
                        team
                    ),
                    PhasePlacement::After => text!(
                        "Move %1 after %2 in team “%3”.",
                        p(scope.phase()),
                        other,
                        team
                    ),
                },
            )
        }
        SetPhaseAcceptance {
            scope,
            acceptance: who,
        } => Prompt::new(
            text!("Change phase acceptance"),
            text!(
                "For %1 in team “%2”, the result will be accepted by %3.",
                p(scope.phase()),
                team,
                acceptance(*who)
            ),
        ),
        SendBackPhase { phase, comment, .. } => Prompt::new(
            text!("Send phase back"),
            text!(
                "Send %1 back for corrections in team “%2”.",
                p(*phase),
                team
            ),
        )
        .comment(comment),
        SubmitPhaseCorrections { phase, .. } => Prompt::new(
            text!("Submit corrections"),
            text!(
                "Submit corrections to %1 for review in team “%2”.",
                p(*phase),
                team
            ),
        ),
        EditPhaseCorrections { phase, .. } => Prompt::new(
            text!("Save corrections"),
            text!(
                "Replace the proposed corrections to %1 in team “%2”.",
                p(*phase),
                team
            ),
        ),
        ApprovePhaseCorrections { phase, .. } => Prompt::new(
            text!("Approve corrections"),
            text!(
                "Approve and publish the corrections to %1 in team “%2”.",
                p(*phase),
                team
            ),
        ),
        RejectPhaseCorrections { phase, comment, .. } => Prompt::new(
            text!("Reject corrections"),
            text!(
                "Reject the corrections to %1 in team “%2”.",
                p(*phase),
                team
            ),
        )
        .comment(comment),
        UndoPhaseCorrectionRejection { phase, .. } => Prompt::new(
            text!("Undo correction rejection"),
            text!(
                "Undo the rejection of corrections to %1 in team “%2”.",
                p(*phase),
                team
            ),
        ),
        ApprovePhase {
            phase,
            confirm_initial,
            ..
        } => Prompt::new(
            text!("Approve phase"),
            if *confirm_initial {
                text!("Approve %1 and start work for team “%2”.", p(*phase), team)
            } else {
                text!("Approve and publish %1 in team “%2”.", p(*phase), team)
            },
        ),
        RejectPhase { phase, comment, .. } => Prompt::new(
            text!("Reject phase"),
            text!("Reject the draft of %1 in team “%2”.", p(*phase), team),
        )
        .comment(comment),
        UndoRejectPhase { phase, .. } => Prompt::new(
            text!("Undo phase rejection"),
            text!("Undo the rejection of %1 in team “%2”.", p(*phase), team),
        ),
        AcceptPhase { phase, .. } => Prompt::new(
            text!("Accept phase"),
            text!(
                "Accept the completed work of %1 in team “%2”.",
                p(*phase),
                team
            ),
        ),
        SubmitDraftPhase { phase, .. } => Prompt::new(
            text!("Submit phase draft"),
            text!(
                "Submit the draft of %1 for review in team “%2”.",
                p(*phase),
                team
            ),
        ),
        EditDraftPhase { phase, .. } => Prompt::new(
            text!("Save phase draft"),
            text!(
                "Replace the current draft of %1 in team “%2”.",
                p(*phase),
                team
            ),
        ),
        Decide { decision, .. } => {
            return card_action(team, labels.card_kind.as_deref()?, decision, labels)
        }
        Ask { .. } => return None,
    };

    details(&mut prompt.body, change, labels);
    Some(prompt)
}

fn selected_agent(agent: &str) -> Text {
    match agent {
        "claude" => Text::verbatim("Claude Code"),
        "codex" => Text::verbatim("Codex"),
        "dsh" => Text::verbatim("DeepSeek"),
        "kimi" => Text::verbatim("Kimi Code"),
        "opencode" => Text::verbatim("OpenCode"),
        _ => text!("the selected agent"),
    }
}

fn member_choices(
    lines: &mut Vec<Text>,
    agent: &str,
    model: &str,
    effort: &str,
    account: &zerus_team_plan::catalog::AccountChoice,
) {
    use zerus_team_plan::catalog::AccountChoice;
    lines.push(text!("Use %1.", selected_agent(agent)));
    if !model.is_empty() {
        lines.push(text!("The model is %1.", visible(model)));
    }
    if !effort.is_empty() {
        lines.push(text!("The effort is %1.", visible(effort)));
    }
    lines.push(match account {
        AccountChoice::Freest => text!("Choose the freest account."),
        AccountChoice::Named { account } => {
            let chosen =
                (!crate::teams::action::canonical_uuid(account)).then_some(account.as_str());
            match chosen {
                Some(value) => text!("Use account “%1”.", visible(value)),
                None => text!("Use the selected account."),
            }
        }
    });
}

fn permitted(allowed: bool) -> Text {
    if allowed {
        text!("permitted")
    } else {
        text!("forbidden")
    }
}

fn permission_lines(values: [bool; 5]) -> Vec<Text> {
    let [accept, answer, trust, resume, merge] = values.map(permitted);
    vec![
        text!("The lead accepting results is %1.", accept),
        text!("The lead answering members is %1.", answer),
        text!("Confirming trust for team-created folders is %1.", trust),
        text!("Resuming members that exited on their own is %1.", resume),
        text!("Merging accepted work is %1.", merge),
    ]
}

fn settings_lines(lines: &mut Vec<Text>, settings: &zerus_team_plan::model::Settings) {
    use zerus_team_plan::model::{Approval, Routing};
    let d = &settings.delegations;
    lines.extend(permission_lines([
        d.accept_results,
        d.answer_members,
        d.confirm_trust,
        d.resume_exited,
        d.merge_accepted,
    ]));
    lines.push(text!(
        "Sending tasks to the queue is %1.",
        permitted(settings.queue)
    ));
    lines.push(match settings.routing {
        Routing::Direct => text!("New tasks use the default pipeline."),
        Routing::Lead => text!("The lead routes new tasks."),
    });
    lines.push(match settings.plan_approval {
        Approval::Review => text!("You approve new plans."),
        Approval::Auto => text!("New plans are approved automatically."),
    });
    lines.push(text!(
        "Phase results are accepted by %1.",
        acceptance(settings.phase_acceptance)
    ));
    lines.push(integration(settings.integration));
    lines.push(text!(
        "Landing stages are %1.",
        permitted(settings.land_allowed)
    ));
    let c = &settings.caps;
    lines.extend([
        text!(
            "The member limit besides the lead is %1.",
            i64::from(c.max_members)
        ),
        text!(
            "The task limit per phase is %1.",
            i64::from(c.max_phase_tasks)
        ),
        text!("The open task limit is %1.", i64::from(c.max_open_tasks)),
        text!(
            "The open question limit per attempt is %1.",
            i64::from(c.max_open_questions)
        ),
    ]);
    let t = &settings.timers;
    lines.extend([
        text!(
            "Wait %1 seconds for a report after the turn ends.",
            i64::from(t.report_timeout_seconds)
        ),
        text!(
            "Wait %1 seconds for assigned work to start.",
            i64::from(t.start_timeout_seconds)
        ),
        text!(
            "Wait at least %1 seconds between lead inbox alerts.",
            i64::from(t.inbox_interval_seconds)
        ),
        text!(
            "Remind the lead after %1 minutes.",
            i64::from(t.remind_lead_minutes)
        ),
        text!(
            "Remind you after %1 minutes.",
            i64::from(t.remind_person_minutes)
        ),
        text!(
            "Notify you after the lead leaves the inbox waiting for %1 minutes.",
            i64::from(t.lead_stall_minutes)
        ),
        text!(
            "Escalate unanswered questions after %1 minutes.",
            i64::from(t.answer_escalate_minutes)
        ),
        text!(
            "Allow at most %1 automatic resumes per hour.",
            i64::from(t.resume_limit_per_hour)
        ),
    ]);
}

fn integration(value: Integration) -> Text {
    match value {
        Integration::Branch => text!("Use the team integration branch for accepted results."),
        Integration::None => text!("Keep accepted results on their task branches."),
    }
}

fn short_commit(commit: &str) -> String {
    visible(&commit.chars().take(12).collect::<String>())
}

fn write_scope(lines: &mut Vec<Text>, writes: &[String]) {
    lines.extend(
        writes
            .iter()
            .map(|path| text!("Work can change %1.", visible(path))),
    );
}

fn optional_member(lines: &mut Vec<Text>, assignee: Option<&str>) {
    if let Some(id) = assignee {
        lines.push(text!(
            "Assign the work to %1.",
            member(&Labels::default(), id)
        ));
    }
}

fn draft_lines(lines: &mut Vec<Text>, draft: &zerus_team_plan::draft::DraftInput) {
    lines.push(text!("Use the draft name “%1”.", visible(&draft.title)));
    lines.push(if draft.barrier {
        text!("This phase waits for all earlier phases.")
    } else {
        text!("This phase has no barrier for earlier phases.")
    });
    for node in &draft.nodes {
        let id = i64::from(node.id);
        let title = visible(&node.title);
        lines.push(match node.kind {
            NodeKind::Task => text!("Task #%1 “%2”", id, title),
            NodeKind::Check => text!("Check #%1 “%2”", id, title),
            NodeKind::Stage => text!("Stage #%1 “%2”", id, title),
        });
        if !node.text.trim().is_empty() {
            lines.push(text!("Task description — “%1”", visible(&node.text)));
        }
        lines.extend(
            node.criteria
                .iter()
                .map(|criterion| text!("Required result — “%1”", visible(criterion))),
        );
        lines.extend(placement(
            Some(&node.role),
            Some(&node.pipeline),
            &Labels::default(),
        ));
        optional_member(lines, node.assignee.as_deref());
        lines.extend(dependencies(&node.deps));
        write_scope(lines, &node.writes);
        if node.estimate_minutes > 0 {
            lines.push(text!(
                "The estimate is %1 minutes.",
                i64::from(node.estimate_minutes)
            ));
        }
    }
}

fn validation_lines(lines: &mut Vec<Text>, config: &zerus_team_plan::validation::Config) {
    for provider in &config.provider {
        let kind = if provider.kind == "test" {
            text!("test")
        } else {
            text!("build")
        };
        lines.push(text!(
            "Use %1 for %2 provider “%3”.",
            visible(&provider.provider),
            kind,
            visible(&provider.name)
        ));
        lines.push(if provider.disabled {
            text!("This provider is disabled.")
        } else {
            text!("This provider is enabled.")
        });
        for scope in &provider.scopes {
            lines.push(if scope == "focused" {
                text!("Supports focused checks.")
            } else {
                text!("Supports full checks.")
            });
        }
        for report in &provider.reports {
            lines.push(if report == "junit" {
                text!("Reports in JUnit format.")
            } else {
                text!("Reports in structured format.")
            });
        }
    }
}

fn details(lines: &mut Vec<Text>, change: &TeamAction, labels: &Labels) {
    use TeamAction::*;
    match change {
        Create {
            template,
            goal,
            integration_branch,
            base_commit,
            integration: mode,
            delegations,
            ..
        }
        | StartPlanning {
            template,
            goal,
            integration_branch,
            base_commit,
            integration: mode,
            delegations,
            ..
        } => {
            lines.push(text!(
                "Use template “%1”.",
                name(labels.template.as_deref(), template)
            ));
            if let Some(project) = labels.project.as_deref() {
                lines.push(text!("The project is %1.", visible(project)));
            }
            if let Some(computer) = labels.computer.as_deref() {
                lines.push(text!("The lead runs on %1.", visible(computer)));
            }
            if !goal.is_empty() {
                lines.push(text!("The goal is “%1”.", visible(goal)));
            }
            lines.push(integration(*mode));
            if !integration_branch.is_empty() {
                lines.push(text!(
                    "The integration branch is %1.",
                    visible(integration_branch)
                ));
            }
            if !base_commit.is_empty() {
                lines.push(text!("Start from commit %1.", short_commit(base_commit)));
            }
            lines.extend(permission_lines([
                delegations.accept_results,
                delegations.answer_members,
                delegations.confirm_trust,
                delegations.resume_exited,
                delegations.merge_accepted,
            ]));
        }
        AddMember {
            agent,
            model,
            effort,
            account,
            ..
        }
        | SwapMember {
            agent,
            model,
            effort,
            account,
            ..
        } => {
            member_choices(lines, agent, model, effort, account);
        }
        Settings { settings } => settings_lines(lines, settings),
        ValidationConfig { selection } => {
            if let Some(config) = &selection.override_config {
                validation_lines(lines, config);
            }
        }
        Route {
            keep_task_fields: false,
            deps,
            writes,
            assignee,
            ..
        } => {
            optional_member(lines, assignee.as_deref());
            lines.extend(dependencies(deps));
            write_scope(lines, writes);
        }
        SubmitDraftPhase { draft, .. }
        | EditDraftPhase { draft, .. }
        | SubmitPhaseCorrections { draft, .. }
        | EditPhaseCorrections { draft, .. } => draft_lines(lines, draft),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
