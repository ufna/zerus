use super::harness::*;
use crate::teams::cli::actor::Caller;
use serde_json::json;

pub(super) fn failed_write() -> serde_json::Value {
    let h = Harness::new();
    h.create();
    h.store().db.as_ref().unwrap().execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON journal BEGIN SELECT RAISE(ABORT,'fixture CLI write failure'); END;").unwrap();
    let input =
        json!({"request_id":uid(0xf001),"team":TEAM,"expected_revision":h.revision()}).to_string();
    let (reply, exit) = h.cli(&Caller::Person, "pause", &input);
    assert_eq!(exit, 1, "{reply}");
    reply
}

#[test]
fn a_command_reports_unknown_storage_outcome_and_can_repeat_the_original_request() {
    let h = Harness::new();
    h.create();
    h.store().db.as_ref().unwrap().execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON journal BEGIN SELECT RAISE(ABORT,'fixture CLI write failure'); END;").unwrap();
    let id = uid(0xf002);
    let input = json!({"request_id":id,"team":TEAM,"expected_revision":h.revision()}).to_string();
    let (reply, exit) = h.cli(&Caller::Person, "pause", &input);
    assert_eq!(exit, 1);
    assert_eq!(code(&reply), "storage_failed");
    assert_eq!(
        reply["error"]["message"],
        "The result is not known; repeat the same request with the same request ID."
    );
    let store = h.store();
    let db = store.db.as_ref().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_write").unwrap();
    let (first, exit) = h.cli(&Caller::Person, "pause", &input);
    assert_eq!(exit, 0, "{first}");
    let (replay, exit) = h.cli(&Caller::Person, "pause", &input);
    assert_eq!(exit, 0, "{replay}");
    assert_eq!(first, replay);
    assert_eq!(h.store().journal(TEAM, 200, None).unwrap().rows.len(), 2);
}

#[test]
fn storage_reads_and_cli_conversion_errors_do_not_claim_a_logical_refusal() {
    let h = Harness::new();
    h.create();
    h.store()
        .db
        .as_ref()
        .unwrap()
        .execute_batch("DROP TABLE requests")
        .unwrap();
    let input =
        json!({"request_id":uid(0xf003),"team":TEAM,"expected_revision":h.revision()}).to_string();
    let (reply, exit) = h.cli(&Caller::Person, "pause", &input);
    assert_eq!((code(&reply), exit), ("storage_failed", 1));
    let (reply, exit) =
        crate::teams::cli::refusal(crate::teams::cli::storage("fixture conversion failure"));
    assert_eq!((code(&reply), exit), ("storage_failed", 1));
    assert_eq!(
        reply["error"]["message"],
        "The result is not known; repeat the same request with the same request ID."
    );
}
