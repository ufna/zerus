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

const PERSON: &str = "00000000-0000-4000-8000-00000000bbbb";

fn seen(h: &mut Harness, node: u32, stage: TaskStage, attempt: u64, id: &str) -> Result<Reply> {
    let mut o = h.observation(node, stage);
    o.attempt = attempt;
    o.attempt_id = id.into();
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o,
        },
    )
}
/// m1 holds node 1, handed out by the team as attempt (1, effect ID).
fn handed() -> (Harness, Actor, u32, String) {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    let id = h.intent(node, "m1");
    h.outcome(
        &id,
        HandoutOutcome::Submitted {
            receipt: "receipt-example".into(),
        },
    );
    seen(&mut h, node, TaskStage::InProgress, 1, &id).unwrap();
    (h, lead, node, id)
}
fn cancel(node: u32, decision: u64, attempt: u64, id: &str) -> Operation {
    Operation::RecordCancel {
        node,
        decision,
        state: CancelState::Cancelled,
        reason: String::new(),
        attempt,
        attempt_id: id.into(),
    }
}
fn last_event(h: &Harness) -> String {
    h.store
        .journal(TEAM, 200, None)
        .unwrap()
        .rows
        .last()
        .unwrap()
        .kind
        .clone()
}
fn bound(h: &Harness, node: u32) -> bool {
    let db = h.store.db.as_ref().unwrap();
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM live_tasks WHERE team=?1 AND node=?2)",
        rusqlite::params![TEAM, node],
        |r| r.get(0),
    )
    .unwrap()
}
/// What both orders must agree on.
fn state(h: &Harness, node: u32) -> (u32, bool, bool, String, NodeState) {
    let generation = h.store.generation(TEAM, node).unwrap().unwrap();
    let pending = !h.store.pending_cancels(TEAM, 200, 0).unwrap().is_empty();
    (
        generation,
        bound(h, node),
        pending,
        last_event(h),
        h.node(node).state,
    )
}

#[test]
fn the_reviewers_witness_ends_in_the_same_state_in_both_orders() {
    let mut ends = Vec::new();
    for observation_first in [true, false] {
        let (mut h, lead, node, id) = handed();
        h.run(
            &lead,
            Operation::Cancel {
                node,
                reason: "Superseded".into(),
            },
        )
        .unwrap();
        let decision = h
            .store
            .pending_cancels(TEAM, 1, 0)
            .unwrap()
            .remove(0)
            .decision;
        if observation_first {
            seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
            refused(
                h.run(&service(), cancel(node, decision, 1, &id)),
                code::INVALID_STATE,
            );
        } else {
            h.run(&service(), cancel(node, decision, 1, &id)).unwrap();
            seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
        }
        ends.push(state(&h, node));
    }
    assert_eq!(ends[0], ends[1]);
    assert_eq!(
        ends[0],
        (
            2,
            true,
            false,
            "person_override".into(),
            NodeState::InProgress
        )
    );
}

#[test]
fn the_nodes_own_hand_out_or_reopen_is_never_a_person_attempt() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    let id = h.intent(node, "m1");
    seen(&mut h, node, TaskStage::InProgress, 1, &id).unwrap();
    h.outcome(
        &id,
        HandoutOutcome::Submitted {
            receipt: "receipt-example".into(),
        },
    );
    seen(&mut h, node, TaskStage::InProgress, 1, &id).unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(1));
    h.report(node, "m1");
    h.run(
        &lead,
        Operation::Return {
            node,
            reason: "Fix it".into(),
        },
    )
    .unwrap();
    let reopen = h.reopen(node, "m1");
    seen(&mut h, node, TaskStage::InProgress, 2, &reopen).unwrap();
    h.reopen_outcome(
        &reopen,
        HandoutOutcome::Submitted {
            receipt: "receipt-example".into(),
        },
    );
    seen(&mut h, node, TaskStage::InProgress, 2, &reopen).unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(2));
    assert!(h
        .store
        .journal(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .all(|e| e.kind != "person_override"));
}

#[test]
fn a_persons_new_attempt_ends_the_earlier_one_whatever_is_pending() {
    for (case, paused) in [
        ("none", false),
        ("cancel", false),
        ("accept", false),
        ("none", true),
        ("accept", true),
    ] {
        let (mut h, lead, node, _) = handed();
        match case {
            "cancel" => drop(
                h.run(
                    &lead,
                    Operation::Cancel {
                        node,
                        reason: "Superseded".into(),
                    },
                )
                .unwrap(),
            ),
            "accept" => {
                h.report(node, "m1");
                h.run(&lead, Operation::Accept { node }).unwrap();
            }
            _ => (),
        }
        if paused {
            h.run(&person(), Operation::Pause).unwrap();
        }
        seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
        assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(2), "{case}");
        assert_eq!(last_event(&h), "person_override", "{case}");
        assert!(
            h.store.pending_cancels(TEAM, 200, 0).unwrap().is_empty(),
            "{case}"
        );
        assert!(
            h.store
                .effects(TEAM, 200, None)
                .unwrap()
                .rows
                .iter()
                .all(|e| !matches!(e.state.as_str(), "reserved" | "submitted")),
            "{case}"
        );
        assert!(
            h.store
                .inbox(TEAM, 200, None)
                .unwrap()
                .rows
                .iter()
                .all(|i| i.state != "open"),
            "{case}"
        );
        assert!(
            bound(&h, node),
            "{case}: the task runs again under the team"
        );
    }
}

#[test]
fn a_stopped_team_restores_nothing() {
    let (mut h, _, node, _) = handed();
    h.run(&person(), Operation::Stop).unwrap();
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(1));
    assert_ne!(last_event(&h), "person_override");
}

#[test]
fn a_repeated_attempt_changes_nothing_and_a_lower_one_is_stale() {
    let (mut h, _, node, id) = handed();
    seen(&mut h, node, TaskStage::InProgress, 1, &id).unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(1));
    refused(
        seen(&mut h, node, TaskStage::InProgress, 0, ""),
        code::STALE_REVISION,
    );
}

#[test]
fn the_current_attempt_can_change_while_the_highest_generation_stays() {
    // Stored (2, A) then observed (2, C), C no effect of the node: an override.
    let (mut h, _, node, a) = handed();
    seen(&mut h, node, TaskStage::InProgress, 2, &a).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(1),
        "(1, A) then (2, A): only the number"
    );
    assert_eq!(
        h.store
            .task_observation(TEAM, node)
            .unwrap()
            .unwrap()
            .attempt,
        2
    );
    seen(&mut h, node, TaskStage::InProgress, 2, &a).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(1),
        "(2, A) then (2, A): nothing"
    );
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(2),
        "(2, A) then (2, C): an override"
    );
    assert_eq!(last_event(&h), "person_override");
}

#[test]
fn a_conflict_keeps_the_stored_attempt_and_decides_nothing() {
    let (mut h, _, node, id) = handed();
    seen(&mut h, node, TaskStage::InProgress, 2, "").unwrap();
    let kept = h.store.task_observation(TEAM, node).unwrap().unwrap();
    assert_eq!(
        (
            kept.attempt,
            kept.attempt_id.as_str(),
            h.store.generation(TEAM, node).unwrap()
        ),
        (1, id.as_str(), Some(1))
    );
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(2),
        "decided once a current attempt shows"
    );
}

#[test]
fn a_cancelled_answer_applies_only_to_the_attempt_the_store_holds() {
    for (attempt, id, applies) in [
        (0, "", false),
        (2, "x", false),
        (1, "other", false),
        (1, "", false),
        (1, "held", true),
    ] {
        let (mut h, lead, node, held) = handed();
        h.run(
            &lead,
            Operation::Cancel {
                node,
                reason: "Superseded".into(),
            },
        )
        .unwrap();
        let decision = h
            .store
            .pending_cancels(TEAM, 1, 0)
            .unwrap()
            .remove(0)
            .decision;
        let id = if id == "held" {
            held.clone()
        } else {
            id.to_owned()
        };
        let rev = h.store.team(TEAM).unwrap().unwrap().revision;
        let result = h.run(&service(), cancel(node, decision, attempt, &id));
        assert_eq!(result.is_ok(), applies, "({attempt}, {id})");
        if !applies {
            assert_eq!(
                h.store.team(TEAM).unwrap().unwrap().revision,
                rev,
                "nothing written"
            );
            let refusal = Operation::RecordCancel {
                node,
                decision,
                state: CancelState::Refused,
                reason: "Delivery unknown".into(),
                attempt: 9,
                attempt_id: "any".into(),
            };
            h.run(&service(), refusal).unwrap();
        }
    }
}

#[test]
fn a_persons_attempt_once_taken_over_is_not_taken_over_again() {
    let (mut h, _, node, _) = handed();
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(2));
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    seen(&mut h, node, TaskStage::InProgress, 3, PERSON).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(2),
        "the stored pair names it"
    );
}

#[test]
fn a_finished_node_still_in_the_live_plan_takes_todays_restore_once() {
    let (mut h, lead, node, _) = handed();
    h.report(node, "m1");
    h.run(&lead, Operation::Accept { node }).unwrap();
    h.observe(node, TaskStage::Done);
    assert_eq!(
        h.node(node).state,
        NodeState::Reported,
        "done, waiting for its merge"
    );
    seen(&mut h, node, TaskStage::InProgress, 2, PERSON).unwrap();
    assert_eq!(
        h.store.generation(TEAM, node).unwrap(),
        Some(2),
        "one restore, not two"
    );
    assert_eq!(last_event(&h), "person_override");
}
