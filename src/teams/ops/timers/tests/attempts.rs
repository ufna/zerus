//! The store follows the task's attempts: an observation carries the
//! highest attempt generation and the current attempt's ID, kept beside it.
use super::*;

fn stored(h: &Harness, node: u32) -> (u64, String) {
    let db = h.store.db.as_ref().unwrap();
    db.query_row(
        "SELECT attempt,attempt_id FROM task_observations WHERE team=?1 AND node=?2",
        rusqlite::params![TEAM, node],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

#[test]
fn an_observation_without_attempts_reads_as_none() {
    let old = r#"{"revision":"token-1","stage":"todo","rank":"a1","title":"Task"}"#;
    let o: TaskObservation = serde_json::from_str(old).unwrap();
    assert_eq!((o.attempt, o.attempt_id.as_str()), (0, ""));
    let cancel: Operation =
        serde_json::from_str(r#"{"op":"record_cancel","node":1,"decision":2,"state":"cancelled"}"#)
            .unwrap();
    assert!(matches!(cancel, Operation::RecordCancel { attempt: 0, .. }));
}

#[test]
fn an_observation_keeps_its_attempt_beside_its_data() {
    let mut h = Harness::new();
    let node = h.enqueue(&person(), 1);
    let mut o = h.observation(node, TaskStage::Todo);
    o.attempt = 3;
    o.attempt_id = "00000000-0000-4000-8000-00000000aaaa".into();
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o.clone(),
        },
    )
    .unwrap();
    assert_eq!(stored(&h, node), (3, o.attempt_id.clone()));
    let read = h.store.task_observation(TEAM, node).unwrap().unwrap();
    assert_eq!((read.attempt, read.attempt_id), (3, o.attempt_id));
}

#[test]
fn an_attempt_id_over_256_bytes_is_refused() {
    let mut h = Harness::new();
    let node = h.enqueue(&person(), 1);
    let mut o = h.observation(node, TaskStage::Todo);
    o.attempt_id = "a".repeat(257);
    refused(
        h.run(
            &service(),
            Operation::ObserveTask {
                node,
                observation: o,
            },
        ),
        code::OVER_CAP,
    );
}

#[test]
fn an_older_store_gains_the_attempt_columns_through_the_transition() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    let db = h.store.db.as_ref().unwrap();
    db.execute_batch("ALTER TABLE task_observations DROP COLUMN attempt; ALTER TABLE task_observations DROP COLUMN attempt_id; DROP TABLE timers_extension;").unwrap();
    h.store = Store::open(h.temp.path()).unwrap();
    assert_eq!(stored(&h, node), (0, String::new()));
}

#[test]
fn a_pending_cancel_names_the_attempt_the_store_holds() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let node = h.add(&person(), 1);
    let mut o = h.observation(node, TaskStage::Todo);
    o.attempt = 2;
    o.attempt_id = "00000000-0000-4000-8000-00000000aaaa".into();
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o.clone(),
        },
    )
    .unwrap();
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    )
    .unwrap();
    let row = h.store.pending_cancels(TEAM, 1, 0).unwrap().remove(0);
    assert_eq!((row.attempt, row.attempt_id), (2, o.attempt_id));
    let decoded: Operation = serde_json::from_str(
        r#"{"op":"record_cancel","node":1,"decision":2,"state":"cancelled","attempt":2}"#,
    )
    .unwrap();
    assert!(
        matches!(decoded, Operation::RecordCancel { attempt: 2, ref attempt_id, .. } if attempt_id.is_empty())
    );
    let long = Operation::RecordCancel {
        node,
        decision: row.decision,
        state: CancelState::Cancelled,
        reason: String::new(),
        attempt: 2,
        attempt_id: "a".repeat(257),
    };
    refused(h.run(&service(), long), code::OVER_CAP);
}
