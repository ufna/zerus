use super::*;
use crate::{desktop::creation_clone::Scope, teams::action::TeamCall};
use serde_json::{json, Value};

const TEAM: &str = "11111111-1111-4111-8111-111111111111";
const REQUEST: &str = "22222222-2222-4222-8222-222222222222";
const HOME: &str = "33333333-3333-4333-8333-333333333333";
const ID: &str = "44444444-4444-4444-8444-444444444444";
const FACTS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn scope() -> Scope {
    Scope {
        configuration: "b".repeat(64),
        service_epoch: HOME.into(),
    }
}

fn labels() -> Labels {
    Labels {
        facts: FACTS.into(),
        task: Some(TaskLabel {
            number: "7".into(),
            title: "Build parser".into(),
            stage: false,
        }),
        phase: Some("Review code".into()),
        relative_phase: Some("Ship release".into()),
        member: Some("Developer".into()),
        role: Some("Reviewer".into()),
        pipeline: Some("Review pipeline".into()),
        project: Some("Zerus development".into()),
        template: Some("Small team".into()),
        branch: Some("integration".into()),
        card_kind: Some("integration".into()),
        kept_route: Some(KeptRoute {
            deps: Vec::new(),
            writes: Vec::new(),
            assignment: None,
        }),
        ..Default::default()
    }
}

fn call(change: TeamAction) -> TeamCall {
    TeamCall {
        team: TEAM.into(),
        team_name: "Renderer label".into(),
        action: change,
        expected_revision: Some(9),
        confirmed: false,
        request_id: REQUEST.into(),
    }
}

fn review(plan: &wire::Plan) -> wire::Review {
    wire::Review {
        intent: wire::Intent {
            scope: scope(),
            machine: HOME.into(),
            store: wire::StoreBinding::Existing {
                fingerprint: "c".repeat(64),
            },
            request: "d".repeat(64),
            facts: FACTS.into(),
            plan: plan.reference().unwrap(),
        },
        team_name: "Canonical team name".into(),
        title: "Legacy title".into(),
        subject: "Legacy operation".into(),
        details: vec![("Legacy field".into(), json!({"opaque": ID}).to_string())],
    }
}

fn rendered(prompt: &Prompt) -> String {
    prompt
        .body
        .iter()
        .map(Text::english)
        .collect::<Vec<_>>()
        .join("\n")
}

fn private_fields_stay_private(body: &str) {
    // The reviewed team ID is now deliberate identity prose; receipt and
    // other authority fields still stay private.
    for hidden in [
        REQUEST,
        HOME,
        ID,
        FACTS,
        "Renderer label",
        "Legacy",
        "opaque",
        "{",
        "}",
    ] {
        assert!(!body.contains(hidden), "visible {hidden:?} in {body}");
    }
}

#[test]
fn all_native_action_kinds_render_from_the_real_plan_encoder() {
    let draft_scope = json!({"kind":"draft","phase":1,"generation":1});
    let other_scope = json!({"kind":"draft","phase":2,"generation":1});
    let draft = json!({"title":"Reviewed draft","barrier":true,"nodes":[{
        "id":7,"title":"Build parser","text":"Read input",
        "criteria":["Accept valid input"],"role":"developer","pipeline":"reviewed",
        "deps":[{"node":6}],"writes":["src/parser.rs"],"estimate_minutes":15
    }]});
    let create = json!({
        "template":"small","project":ID,"name":"New team name","goal":"Improve parser",
        "home":HOME,"integration_branch":"integration","base_commit":"a".repeat(40),
        "expected_template":"b".repeat(64),
        "delegations":{"accept_results":true,"answer_members":false,"confirm_trust":true,
                       "resume_exited":true,"merge_accepted":false}
    });
    let mut cases: Vec<(Value, &str)> = vec![
        (
            json!({"kind":"set_phase_barrier","scope":draft_scope,"barrier":true}),
            "Change phase barrier",
        ),
        (
            json!({"kind":"pause_phase","scope":draft_scope,"reason":"Wait for input"}),
            "Pause phase",
        ),
        (
            json!({"kind":"resume_phase","scope":draft_scope}),
            "Resume phase",
        ),
        (
            json!({"kind":"move_phase","scope":draft_scope,"relative_to":other_scope,"placement":"before"}),
            "Move phase",
        ),
        (
            json!({"kind":"set_phase_acceptance","scope":draft_scope,"acceptance":"person"}),
            "Change phase acceptance",
        ),
        (
            json!({"kind":"send_back_phase","phase":1,"publication":ID,"comment":"Needs review"}),
            "Send phase back",
        ),
        (
            json!({"kind":"submit_phase_corrections","phase":1,"publication":ID,"rework":REQUEST,"draft":draft}),
            "Submit corrections",
        ),
        (
            json!({"kind":"edit_phase_corrections","phase":1,"publication":ID,"rework":REQUEST,"generation":2,"draft":draft}),
            "Save corrections",
        ),
        (
            json!({"kind":"approve_phase_corrections","phase":1,"publication":ID,"rework":REQUEST,"generation":2}),
            "Approve corrections",
        ),
        (
            json!({"kind":"reject_phase_corrections","phase":1,"publication":ID,"rework":REQUEST,"generation":2,"comment":"Needs review"}),
            "Reject corrections",
        ),
        (
            json!({"kind":"undo_phase_correction_rejection","phase":1,"publication":ID,"rework":REQUEST,"generation":2,"rejection":HOME}),
            "Undo correction rejection",
        ),
        (
            json!({"kind":"approve_phase","phase":1,"generation":1,"confirm_initial":true}),
            "Approve phase",
        ),
        (
            json!({"kind":"reject_phase","phase":1,"generation":1,"comment":"Needs review"}),
            "Reject phase",
        ),
        (
            json!({"kind":"undo_reject_phase","phase":1,"generation":1,"rejection":ID}),
            "Undo phase rejection",
        ),
        (
            json!({"kind":"accept_phase","phase":1,"publication":ID}),
            "Accept phase",
        ),
        (
            json!({"kind":"submit_draft_phase","phase":1,"draft":draft}),
            "Submit phase draft",
        ),
        (
            json!({"kind":"edit_draft_phase","phase":1,"generation":1,"draft":draft}),
            "Save phase draft",
        ),
        (
            json!({"kind":"add_member","role":"reviewer","agent":"codex","model":"gpt-test","effort":"high"}),
            "Add member",
        ),
        (
            json!({"kind":"swap_member","member":"m2","agent":"dsh"}),
            "Swap member",
        ),
        (
            json!({"kind":"rename_team","name":"New team name"}),
            "Rename team",
        ),
        (
            json!({"kind":"set_goal","goal":"Improve parser"}),
            "Change goal",
        ),
        (json!({"kind":"pause"}), "Pause team"),
        (json!({"kind":"resume"}), "Resume team"),
        (json!({"kind":"stop"}), "Stop team"),
        (json!({"kind":"archive"}), "Archive team"),
        (
            json!({"kind":"advance_base","commit":"a".repeat(40)}),
            "Advance base",
        ),
        (json!({"kind":"pause_member","member":"m2"}), "Pause member"),
        (
            json!({"kind":"resume_member","member":"m2"}),
            "Resume member",
        ),
        (json!({"kind":"stop_member","member":"m2"}), "Stop member"),
        (json!({"kind":"archive_member","member":"m2"}), "Remove"),
        (
            json!({"kind":"settings","settings":{}}),
            "Save team settings",
        ),
        (
            json!({"kind":"validation_config","selection":{"digest":"a".repeat(64),"source_digest":"b".repeat(64),"override_config":null}}),
            "Tests and Builds",
        ),
        (
            json!({"kind":"decide","card":987654,"decision":"approve"}),
            "Approve merge",
        ),
        (json!({"kind":"admit","node":7}), "Admit"),
        (
            json!({"kind":"route","node":7,"role":"reviewer","pipeline":"reviewed","keep_task_fields":true}),
            "Route",
        ),
        (
            json!({"kind":"set_deps","node":7,"deps":[{"node":4,"kind":"soft"}]}),
            "Change dependencies",
        ),
        (json!({"kind":"accept","node":7}), "Accept"),
        (
            json!({"kind":"return","node":7,"reason":"Needs tests"}),
            "Return",
        ),
        (
            json!({"kind":"reassign","node":7,"member":"m2","reason":"Availability"}),
            "Reassign",
        ),
        (json!({"kind":"hold","node":7}), "Hold"),
        (json!({"kind":"release","node":7}), "Release"),
        (
            json!({"kind":"cancel","node":7,"reason":"Obsolete"}),
            "Cancel task",
        ),
        (
            json!({"kind":"reply","item":987654,"text":"Here is the answer."}),
            "Reply",
        ),
        (json!({"kind":"ask_lead","text":"Line one\nLine two"}), "Ask the lead…"),
        (json!({"kind":"leave_lead_note","text":"Line one\nLine two"}), "Leave a note"),
    ];
    for (kind, expected) in [
        ("create", "Create team"),
        ("start_planning", "Start planning"),
    ] {
        let mut fields = create.clone();
        fields["kind"] = json!(kind);
        cases.push((fields, expected));
    }
    assert_eq!(cases.len(), 47);
    for (fields, expected) in cases {
        let change: TeamAction = serde_json::from_value(fields.clone()).unwrap();
        let op = change.op();
        let plan = wire::Plan::from_call(&call(change)).unwrap_or_else(|| panic!("{op} plan"));
        let prompt =
            action(&plan, &review(&plan), &labels()).unwrap_or_else(|| panic!("{op} presentation"));
        assert_eq!(prompt.title.english(), expected, "{op}");
        let body = rendered(&prompt);
        if matches!(op, "ask-lead" | "leave-lead-note") {
            assert!(body.ends_with("\nLine one\nLine two"), "{op}: {body}");
        }
        let team = if plan.expected_revision == 0 {
            "New team name"
        } else {
            "Canonical team name"
        };
        assert!(body.contains(team), "{op}: {body}");
        if fields.get("node").is_some() {
            assert!(body.contains("task #7 “Build parser”"), "{op}: {body}");
        }
        if fields.get("phase").is_some() || fields.get("scope").is_some() {
            assert!(body.contains("phase 1 “Review code”"), "{op}: {body}");
        }
        if fields.get("member").is_some() {
            assert!(body.contains("Developer (m2)"), "{op}: {body}");
        }
        if op == "archive-member" {
            assert!(
                body.contains("Removing a member cannot be undone"),
                "{op}: {body}"
            );
        }
        assert!(!body.contains("987654"), "{op}: {body}");
        private_fields_stay_private(&body);
    }
    assert!(wire::Plan::from_call(&call(TeamAction::Ask {
        node: 7,
        generation: 1,
        text: "Can I continue?".into(),
    }))
    .is_none());
}

#[test]
fn intake_uses_the_task_plan_producer_and_preserves_displayed_placement() {
    for action_data in [
        wire::Action::NewTask {
            project: ID.into(),
            title: "Build parser".into(),
            backlog: true,
            role: Some("reviewer".into()),
            pipeline: Some("reviewed".into()),
        },
        wire::Action::Intake {
            project: ID.into(),
            task: REQUEST.into(),
            revision: "clock-v2".into(),
            role: Some("reviewer".into()),
            pipeline: Some("reviewed".into()),
        },
    ] {
        let draft = wire::TaskDraft {
            request_id: REQUEST.into(),
            team: TEAM.into(),
            action: action_data,
        };
        let plan = draft.plan(9).unwrap();
        let body = rendered(&action(&plan, &review(&plan), &labels()).unwrap());
        for value in ["Build parser", "Zerus development", "Canonical team name"] {
            assert!(body.contains(value), "{body}");
        }
        match plan.action {
            wire::Action::NewTask { .. } => {
                assert!(body.contains("Backlog"), "{body}");
                assert!(body.contains("The role is Reviewer."), "{body}");
                assert!(body.contains("The pipeline is Review pipeline."), "{body}");
            }
            wire::Action::Intake { .. } => {
                assert!(body.contains("task #7"));
                assert!(body.contains("The role is Reviewer."));
                assert!(body.contains("The pipeline is Review pipeline."));
            }
            _ => unreachable!(),
        }
        private_fields_stay_private(&body);
    }
}

fn card_review(kind: &str, decision: &str) -> team_person::Review {
    let plan = team_person::Plan::from_call(&call(TeamAction::Decide {
        card: 987654,
        decision: decision.into(),
    }))
    .unwrap();
    team_person::Review {
        intent: team_person::Intent {
            scope: scope(),
            machine: HOME.into(),
            store: "e".repeat(64),
            facts: FACTS.into(),
            plan,
        },
        team_name: "Canonical team name".into(),
        card_kind: kind.into(),
        card_text: format!("opaque {ID}"),
        card_parameters: vec![("opaque".into(), ID.into())],
        card_notes: vec![ID.into()],
    }
}

#[test]
fn owner_card_effects_are_specific_to_the_bound_kind_and_decision() {
    for (kind, decision, title) in [
        ("integration", "approve", "Approve merge"),
        ("integration", "reject", "Reject merge"),
        ("pipeline_acceptance", "approve", "Accept work"),
        ("pipeline_acceptance", "reject", "Decline acceptance"),
        ("launch_blocked", "retry", "Retry launch"),
        ("launch_uncertain", "acknowledged", "Acknowledge"),
        ("custom_request", "approve", "Approve"),
        ("custom_request", "reject", "Reject"),
    ] {
        let review = card_review(kind, decision);
        let prompt = card(&review, &labels()).unwrap();
        assert_eq!(prompt.title.english(), title);
        let body = rendered(&prompt);
        assert!(body.contains("Canonical team name"));
        assert!(body.contains("cannot be undone"));
        if kind != "launch_blocked" {
            assert!(body.contains("task #7 “Build parser”"), "{body}");
        }
        assert!(!body.contains("987654"));
        private_fields_stay_private(&body);
    }
    assert!(card(&card_review("integration", ID), &labels()).is_none());
}

#[test]
fn review_cannot_supply_text_for_another_plan_or_facts_snapshot() {
    let old = wire::Plan::from_call(&call(TeamAction::Accept { node: 7 })).unwrap();
    let another = wire::Plan::from_call(&call(TeamAction::Accept { node: 8 })).unwrap();
    let mut labels = labels();
    assert!(action(&another, &review(&old), &labels).is_none());
    labels.facts = "f".repeat(64);
    assert!(action(&old, &review(&old), &labels).is_none());
    assert!(card(&card_review("integration", "approve"), &labels).is_none());
}

#[test]
fn renderer_reasons_and_comments_stay_inside_their_quoted_label() {
    let provided = "Duplicate.\n\nSee the previous\u{2028}report\u{2029}for “context”.";
    for (change, label) in [
        (
            TeamAction::Cancel {
                node: 7,
                reason: provided.into(),
            },
            "Reason",
        ),
        (
            TeamAction::RejectPhase {
                phase: 1,
                generation: 1,
                comment: provided.into(),
            },
            "Comment",
        ),
    ] {
        let plan = wire::Plan::from_call(&call(change)).unwrap();
        let prompt = action(&plan, &review(&plan), &labels()).unwrap();
        let line = prompt
            .body
            .iter()
            .map(Text::english)
            .find(|line| line.contains("Duplicate."))
            .unwrap();
        assert_eq!(
            line,
            format!("{label} — “Duplicate.  See the previous report for \\“context\\”.”")
        );
    }
}

#[test]
fn stored_captions_cannot_add_consent_lines() {
    let mut labels = labels();
    labels.task.as_mut().unwrap().title = "Build parser”\nSee\u{2028}report".into();
    let mut review = card_review("integration", "approve");
    review.team_name = "Orchard\u{2029}east".into();
    let prompt = card(&review, &labels).unwrap();
    let line = prompt.body[0].english();
    assert!(line.contains("Build parser\\” See report"), "{line:?}");
    assert!(line.contains("Orchard east"), "{line:?}");
}

#[test]
fn bidi_controls_do_not_reorder_reviewed_text() {
    let plan = wire::Plan::from_call(&call(TeamAction::Accept { node: 7 })).unwrap();
    let mut labels = labels();
    labels.task.as_mut().unwrap().title = "Build\u{202e} parser\u{2067}".into();
    let body = rendered(&action(&plan, &review(&plan), &labels).unwrap());
    assert!(body.contains("task #7 “Build parser”"), "{body:?}");
}

#[test]
fn the_cancel_warning_explains_the_attempt_cannot_continue() {
    let plan = wire::Plan::from_call(&call(TeamAction::Cancel {
        node: 7,
        reason: "Duplicate".into(),
    }))
    .unwrap();
    let body = rendered(&action(&plan, &review(&plan), &labels()).unwrap());
    assert!(
        body.contains("The team cannot continue the cancelled attempt."),
        "{body:?}"
    );
}

/// Review of #2558 (P2, native confirmations): each confirmation sentence is
/// defined once. The window's fallback dialog reads the native prompt's own
/// first line for the same call, so both show the same catalog sentence; a
/// member without a display label is shown by its ID in both.
#[test]
fn the_window_confirmation_is_the_native_prompts_sentence() {
    let mut labels = labels();
    labels.member = None;
    let exact = [
        TeamAction::Stop {},
        TeamAction::Archive {},
        TeamAction::ArchiveMember {
            member: "m2".into(),
        },
        TeamAction::StopMember {
            member: "m2".into(),
        },
    ];
    let same_sentence = [
        serde_json::from_value::<TeamAction>(
            json!({"kind": "swap_member", "member": "m2", "agent": "dsh"}),
        )
        .unwrap(),
        TeamAction::Cancel {
            node: 7,
            reason: "Obsolete".into(),
        },
        TeamAction::AdvanceBase {
            commit: "a".repeat(40),
        },
    ];
    for (change, whole) in exact
        .into_iter()
        .map(|change| (change, true))
        .chain(same_sentence.into_iter().map(|change| (change, false)))
    {
        let plan = wire::Plan::from_call(&call(change.clone()))
            .unwrap_or_else(|| panic!("no plan for {change:?}"));
        let review = review(&plan);
        let native = action(&plan, &review, &labels)
            .unwrap_or_else(|| panic!("no native prompt for {change:?}"));
        let window = change.confirmation(&review.team_name).unwrap();
        assert_eq!(window.message.source, native.body[0].source, "{change:?}");
        assert_eq!(window.text, window.message.english(), "{change:?}");
        if whole {
            assert_eq!(*window.message, native.body[0], "{change:?}");
        }
    }
}

/// A team without a name is "this team" in the window's own language, never an
/// English word inside a Russian sentence.
#[test]
fn an_unnamed_teams_confirmation_names_it_in_the_window_language() {
    let window = TeamAction::Stop {}.confirmation("  ").unwrap();
    assert_eq!(*window.message, text!(
        "Stop %1? Every open task is cancelled and no task is handed out again. Member sessions keep running; their current turns finish. A stopped team cannot be started again; use Pause to stop it for a while.",
        text!("this team")
    ));
    assert!(
        window.text.starts_with("Stop this team? "),
        "{}",
        window.text
    );
}

#[test]
fn same_named_teams_keep_their_reviewed_identity_in_all_confirmation_paths() {
    // The suffixes deliberately collide: the full service-reviewed UUID is
    // readable, exact, and needs no unbounded same-name inventory lookup.
    let other = "55555555-5555-4555-8555-111111111111";
    for id in [TEAM, other] {
        let mut input = call(TeamAction::RenameTeam {
            name: "Next name".into(),
        });
        input.team = id.into();
        let generic = wire::Plan::from_call(&input).unwrap();
        let task = wire::TaskDraft {
            request_id: REQUEST.into(),
            team: id.into(),
            action: wire::Action::NewTask {
                project: ID.into(),
                title: "Build parser".into(),
                backlog: false,
                role: None,
                pipeline: None,
            },
        }
        .plan(9)
        .unwrap();
        let mut owner = card_review("launch_blocked", "retry");
        owner.intent.plan.team = id.into();
        owner.team_name = "Shared team".into();
        let mut prompts = Vec::new();
        for plan in [&generic, &task] {
            let mut reviewed = review(plan);
            reviewed.team_name = "Shared team".into();
            prompts.push(action(plan, &reviewed, &labels()).unwrap());
        }
        prompts.push(card(&owner, &labels()).unwrap());
        for prompt in prompts {
            assert!(rendered(&prompt).contains("Shared team"));
            let identity = prompt
                .body
                .iter()
                .find(|line| line.source == "Team ID: %1")
                .expect("reviewed team identity");
            assert_eq!(identity.english(), format!("Team ID: {id}"));
            assert_eq!(identity.args, vec![crate::text::Arg::Verbatim(id.into())]);
            let catalog: Value = serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../src/i18n.ru.json"
            )))
            .unwrap();
            assert_eq!(catalog["Team ID: %1"], "Идентификатор команды: %1");
            assert_eq!(
                catalog["Team ID: %1"].as_str().unwrap().replace("%1", id),
                format!("Идентификатор команды: {id}")
            );
            assert!(!rendered(&prompt).contains(if id == TEAM { other } else { TEAM }));
        }
    }
}
