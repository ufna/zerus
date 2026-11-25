//! Policy models, not evidence of a physical person's gesture.
use super::*;
use zerus_client_core::{desktop::creation_clone::Scope, teams::action::TeamAction};

pub(super) fn call() -> TeamCall {
    TeamCall {
        team: uuid::Uuid::new_v4().to_string(),
        team_name: "Untrusted caption".into(),
        action: TeamAction::RenameTeam {
            name: "New name".into(),
        },
        expected_revision: Some(2),
        confirmed: true,
        request_id: uuid::Uuid::new_v4().to_string(),
    }
}
pub(super) fn review(call: &TeamCall) -> wire::Review {
    wire::Review {
        intent: wire::Intent {
            scope: Scope {
                configuration: "a".repeat(64),
                service_epoch: uuid::Uuid::new_v4().to_string(),
            },
            machine: uuid::Uuid::new_v4().to_string(),
            store: wire::StoreBinding::Existing {
                fingerprint: "b".repeat(64),
            },
            facts: "c".repeat(64),
            request: "d".repeat(64),
            plan: wire::Plan::from_call(call).unwrap().reference().unwrap(),
        },
        team_name: "Canonical caption".into(),
        title: "Confirm team action".into(),
        subject: "Rename".into(),
        details: vec![("Name".into(), "New name".into())],
    }
}

#[test]
fn only_a_valid_live_pong_selects_a_prepare_format() {
    let id = "00000000-0000-4000-8000-000000000001";
    let epoch = "00000000-0000-4000-8000-000000000002";
    let mut pong = json!({"request_id":id,"accepted":true,"durable":false,"op":"pong"});
    let negotiation = |pong: &Value| {
        review_labels(&serde_json::to_vec(pong).unwrap(), id, epoch)
    };
    assert_eq!(negotiation(&pong), Some(false));
    pong[wire::REVIEW_LABELS_CAPABILITY] =
        json!({"version":wire::REVIEW_LABELS_VERSION,"epoch":epoch});
    assert_eq!(negotiation(&pong), Some(true));
    pong[wire::REVIEW_LABELS_CAPABILITY]["epoch"] = json!("stale epoch");
    assert_eq!(negotiation(&pong), None);
    pong[wire::REVIEW_LABELS_CAPABILITY] = json!(true);
    assert_eq!(negotiation(&pong), None);
    pong.as_object_mut().unwrap().remove(wire::REVIEW_LABELS_CAPABILITY);
    pong["request_id"] = json!("other request");
    assert_eq!(negotiation(&pong), None);
    assert_eq!(negotiation(&json!({"accepted":false})), None);
}

#[test]
fn prepare_requests_only_send_the_label_field_for_the_separate_capability() {
    for op in [
        "team_person_action_prepare",
        "team_person_task_prepare",
        "team_person_prepare",
    ] {
        let legacy = json!({"op":op,"scope":{"configuration":"example"}});
        assert_eq!(review_query(legacy.clone(), false), legacy);
        let mut expected = legacy.clone();
        expected["with_labels"] = json!(true);
        assert_eq!(review_query(legacy, true), expected);
    }
}

#[test]
fn review_reply_uses_only_the_negotiated_format() {
    let call = call();
    let review = review(&call);
    let mut reply = json!({"ok":true,"review":review});
    let legacy = serde_json::to_vec(&reply).unwrap();
    let (parsed, labels) = parse_review(&legacy, false).unwrap();
    assert_eq!(parsed["review"], json!(review));
    assert!(labels.is_none());
    assert!(parse_review(&legacy, true).is_none());

    reply["labels"] = json!({"facts":review.intent.facts});
    let labeled = serde_json::to_vec(&reply).unwrap();
    let (_, labels) = parse_review(&labeled, true).unwrap();
    assert_eq!(labels.unwrap().facts, review.intent.facts);

    reply["labels"]["unexpected"] = json!(true);
    assert!(parse_review(&serde_json::to_vec(&reply).unwrap(), true).is_none());
    reply["ok"] = json!(false);
    assert!(parse_review(&serde_json::to_vec(&reply).unwrap(), false).is_none());
}

#[test]
fn legacy_confirmation_keeps_the_reviewed_identity_and_values() {
    let call = call();
    let review = review(&call);
    let body = confirmation_body(&legacy_action_prompt(&review).body);
    assert!(body.contains(&review.team_name));
    assert!(body.contains(&review.intent.plan.team));
    assert!(body.contains(&review.intent.machine));
    assert!(body.contains("Name:\nNew name"));
    assert!(!body.contains(&call.team_name));
}

#[test]
fn native_confirmation_keeps_a_percent_sign_in_a_bound_team_name_literal() {
    let mut call = call();
    call.action = TeamAction::RenameTeam {
        name: "Launch at 50%".into(),
    };
    let plan = wire::Plan::from_call(&call).unwrap();
    let review = review(&call);
    let labels = Labels {
        facts: review.intent.facts.clone(),
        ..Default::default()
    };
    let prompt = team_confirmation::action(&plan, &review, &labels).unwrap();
    let body = confirmation_body(&prompt.body);
    if cfg!(target_os = "linux") {
        assert!(body.contains("Launch at 50%%”"));
    } else {
        assert!(body.contains("Launch at 50%"));
        assert!(!body.contains("50%%"));
    }
}
#[test]
fn renderer_confirmed_never_substitutes_for_native_acceptance() {
    let call = call();
    assert!(call.confirmed);
    assert!(Confirmed::after_dialog(review(&call), false).is_none());
    let review = review(&call);
    let expected = review.intent.clone();
    assert_eq!(Confirmed::after_dialog(review, true).unwrap().0, expected);
}
#[test]
fn lost_results_only_read_the_exact_original_and_never_adopt() {
    let call = call();
    let plan = wire::Plan::from_call(&call).unwrap();
    let actions = Actions::default();
    assert!(matches!(actions.path(&plan, true), Path::Unknown));
    let review = review(&call);
    assert!(actions.reserve(&review.intent));
    assert!(!actions.reserve(&review.intent));
    assert!(matches!(actions.path(&plan,true),Path::Read(intent) if *intent==review.intent));
    let mut changed = plan.clone();
    changed.expected_revision += 1;
    assert!(matches!(actions.path(&changed, true), Path::Unknown));
    actions.dismiss(&plan.request_id);
    assert!(matches!(actions.path(&plan, true), Path::Unknown));
}
#[test]
fn successful_receipts_bind_identity_and_storage_failure_stays_unknown() {
    let call = call();
    let intent = review(&call).intent;
    let mut value = json!({"intent":intent,"reply":{"ok":true,"request_id":call.request_id,"team":call.team,"revision":3,"actor":{"kind":"person","person":"local","origin":"desktop_ui"}}});
    assert!(matches!(
        outcome(&call, &intent, &serde_json::to_vec(&value).unwrap()),
        TeamOutcome::Done { .. }
    ));
    value["reply"]["actor"]["origin"] = json!("cli_interim");
    assert!(matches!(
        outcome(&call, &intent, &serde_json::to_vec(&value).unwrap()),
        TeamOutcome::Uncertain { .. }
    ));
    value["reply"] = json!({"ok":false,"request_id":call.request_id,"code":"storage_failed","message":"Unknown"});
    assert!(matches!(
        outcome(&call, &intent, &serde_json::to_vec(&value).unwrap()),
        TeamOutcome::Uncertain { .. }
    ));
}
