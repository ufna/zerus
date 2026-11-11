use super::*;
use crate::desktop::teams::confirm::{engine, policy::tests::sample};
fn row(kind: Kind) -> Record {
    let (b, _, s) = sample();
    let question = s.questions[0].clone();
    Record {
        plan: engine::candidate(&b, &s, kind, question),
        original: uuid::Uuid::new_v4().to_string(),
        epoch: "epoch-1".into(),
        state: "prepared".into(),
        audit: Some(Audit {
            id: 1,
            request: uuid::Uuid::new_v4().to_string(),
            answers: json!([{"question_id":"trust","selected_option_ids":["trust_1"],"text":""}]),
            fresh: true,
        }),
        audit_done: false,
        result: None,
        detail: String::new(),
        card_request: uuid::Uuid::new_v4().to_string(),
        card_done: false,
        at: 1234,
    }
}
fn answer_receipt(row: &Record, input: &[u8]) -> Value {
    let p = &row.plan;
    json!({"request_id":row.audit.as_ref().unwrap().request,"name":p.snapshot.name,"run_id":p.snapshot.run,"conversation_id":p.snapshot.conversation,"question_id":p.question["question_id"],"question_hash":p.question["question_hash"],"digest":digest(input),"status":"answered","transport":"claude_tui"})
}
#[test]
fn trust_receipt_must_match_every_native_identity_and_exact_input() {
    let row = row(Kind::Trust);
    let (args, input, folder) = invocation(&row).unwrap();
    assert_eq!(args[0], "answer");
    assert_eq!(folder, "question_receipts");
    let receipt = answer_receipt(&row, &input);
    assert!(matches!(
        project(&row, &input, receipt.to_string().as_bytes()),
        Outcome::Approved(None)
    ));
    for field in [
        "request_id",
        "name",
        "run_id",
        "conversation_id",
        "question_id",
        "question_hash",
        "digest",
        "status",
    ] {
        let mut changed = receipt.clone();
        changed[field] = json!("other");
        assert!(
            matches!(
                project(&row, &input, changed.to_string().as_bytes()),
                Outcome::Unknown(_)
            ),
            "{field}"
        );
    }
    let mut changed = receipt;
    changed["status"] = json!("in_progress");
    assert!(matches!(
        project(&row, &input, changed.to_string().as_bytes()),
        Outcome::Unknown(_)
    ));
}
#[test]
fn restore_uses_the_exact_clean_exit_archive_and_only_completed_receipts_publish() {
    let mut row = row(Kind::Resume);
    row.plan.snapshot.conversation = "conversation-1".into();
    row.plan.snapshot.archive = Some(uuid::Uuid::new_v4().to_string());
    let (args, input, folder) = invocation(&row).unwrap();
    let request: Value = serde_json::from_slice(&input).unwrap();
    assert_eq!(args[0], "session-action");
    assert_eq!(folder, "action_receipts");
    assert_eq!(request["action"], "restore");
    assert_eq!(request["archive_id"], json!(row.plan.snapshot.archive));
    let receipt = crate::state::lifecycle_fixture_receipt(
        &row.plan.snapshot.name,
        &input,
        "completed",
        Some("new-run"),
    );
    let Outcome::Approved(Some(target)) = project(&row, &input, receipt.to_string().as_bytes())
    else {
        panic!("Exact completed receipt refused");
    };
    assert_eq!(target.run, "new-run");
    assert_eq!(target.conversation, "conversation-1");
    assert!(target.receipt_at.is_some());
    for field in [
        "request_id",
        "name",
        "run_id",
        "conversation_id",
        "request_hash",
        "archive_id",
        "status",
    ] {
        let mut v = receipt.clone();
        v[field] = json!("other");
        assert!(
            matches!(
                project(&row, &input, v.to_string().as_bytes()),
                Outcome::Unknown(_)
            ),
            "{field}"
        );
    }
    let uncertain =
        crate::state::lifecycle_fixture_receipt(&row.plan.snapshot.name, &input, "uncertain", None);
    assert!(matches!(
        project(&row, &input, uncertain.to_string().as_bytes()),
        Outcome::Unknown(_)
    ));
}
#[test]
fn crash_resume_and_brief_are_separate_requests_bound_to_the_new_run() {
    let mut row = row(Kind::Resume);
    row.plan.snapshot.conversation = "conversation-1".into();
    let (_, input, _) = invocation(&row).unwrap();
    let request: Value = serde_json::from_slice(&input).unwrap();
    assert_eq!(request["action"], "resume");
    assert!(request.get("archive_id").is_none());
    let receipt = crate::state::lifecycle_fixture_receipt(
        &row.plan.snapshot.name,
        &input,
        "completed",
        Some("new-run"),
    );
    let Outcome::Approved(Some(target)) = project(&row, &input, receipt.to_string().as_bytes())
    else {
        panic!("Expected new run");
    };
    let plan = engine::brief(&row, &target);
    row.plan = plan;
    row.audit.as_mut().unwrap().request = uuid::Uuid::new_v4().to_string();
    let (args, input, folder) = invocation(&row).unwrap();
    let brief: Value = serde_json::from_slice(&input).unwrap();
    assert_eq!(args[0], "send");
    assert_eq!(folder, "input_receipts");
    assert_eq!(brief["expected_run_id"], "new-run");
    assert_eq!(brief["expected_conversation_id"], "conversation-1");
    assert_ne!(brief["request_id"], request["request_id"]);
    let receipt = json!({"request_id":brief["request_id"],"name":target.name,"run_id":"new-run","conversation_id":target.conversation,"digest":digest(&input),"status":"submitted","transport":"terminal"});
    assert!(matches!(
        project(&row, &input, receipt.to_string().as_bytes()),
        Outcome::Approved(None)
    ));
    let mut wrong = receipt;
    wrong["run_id"] = request["expected_run_id"].clone();
    assert!(matches!(
        project(&row, &input, wrong.to_string().as_bytes()),
        Outcome::Unknown(_)
    ));
}
#[test]
fn a_missing_or_replaced_original_store_never_finishes_as_approved() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    crate::platform::private_dir(&state).unwrap();
    let roots = Roots::open(&state, "question_receipts").unwrap();
    let row = row(Kind::Trust);
    let (_, input, _) = invocation(&row).unwrap();
    assert!(matches!(observe(&roots, &row, &input), Outcome::Unknown(_)));
    std::fs::rename(&state, temp.path().join("original")).unwrap();
    crate::platform::private_dir(&state).unwrap();
    assert!(matches!(observe(&roots, &row, &input), Outcome::Unknown(_)));
}
