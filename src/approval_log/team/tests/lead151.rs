//! Audit rows are reserved by begin_team, including on Windows.
use super::*;
#[test]
fn a_startup_brief_has_a_durable_audit_without_allowing_an_empty_resume() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(&dir.path().join("audit")).unwrap();
    let question = json!({"source":"team_resume","question_id":"brief:run-1",
        "question_hash":"ab".repeat(32),"run_id":"run-1","conversation_id":"",
        "can_answer":false,"questions":[{"id":"team_resume","question":"Read hgs team inbox","options":[]}]});
    let p = payload("claude", &question);
    assert_eq!(begin(&mut db, &p).unwrap()["eligible"], false);
    let first = begin_team(&mut db, &p, TeamKind::LaunchBrief).unwrap();
    let again = begin_team(&mut db, &p, TeamKind::LaunchBrief).unwrap();
    assert_eq!(first["fresh"], true);
    assert_eq!(again["fresh"], false);
    assert_eq!(first["request_id"], again["request_id"]);
    assert_eq!(
        list(&db, &json!({})).unwrap()["entries"][0]["categories"],
        json!(["team_launch_brief"])
    );
    assert!(begin_team(&mut db, &p, TeamKind::Resume).is_err());
    assert!(begin_team(&mut db, &p, TeamKind::ResumeBrief).is_err());
}

#[test]
fn classifying_new_briefs_keeps_an_existing_reservation_and_its_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(&dir.path().join("audit")).unwrap();
    let question = json!({"source":"team_resume","question_id":"brief:run-1",
        "question_hash":"cd".repeat(32),"run_id":"run-1","conversation_id":"",
        "can_answer":false,"questions":[{"id":"team_resume","question":"Read hgs team inbox","options":[]}]});
    let mut old = payload("codex", &question);
    old["categories"] = json!(["team_resume"]);
    // This is the same writer and wire shape used by the older category.
    let original = reserve(&mut db, &old, json!([]), true).unwrap();
    finish(
        &db,
        &json!({"id":original["id"],"request_id":original["request_id"],"status":"unknown"}),
    )
    .unwrap();
    let reopened = begin_team(&mut db, &old, TeamKind::LaunchBrief).unwrap();
    assert_eq!(reopened["fresh"], false);
    assert_eq!(reopened["request_id"], original["request_id"]);
    let rows = list(&db, &json!({})).unwrap();
    assert_eq!(rows["entries"].as_array().unwrap().len(), 1);
    assert_eq!(rows["entries"][0]["status"], "unknown");
    assert_eq!(rows["entries"][0]["categories"], json!(["team_resume"]));
}
