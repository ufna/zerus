use super::*;
#[test]
fn every_actor_key_prefix_is_pruned_with_a_finished_team_in_bounded_pages() {
    let h = Harness::new();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE teams SET status=?2 WHERE id=?1",
            rusqlite::params![TEAM, serde_json::to_string(&TeamStatus::Finished).unwrap()],
        )
        .unwrap();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let prefixes = [
        "cancel_cursor",
        "cancel_recovery_cursor",
        "cancel:",
        "delivery:",
        "delivery_cursor",
        "merge:",
        "merge_cursor",
        "gate:",
        "journal",
        "stage_cursor",
        "task_command:",
        "task_command_cursor",
        "stage_clear_cursor",
        "terminal_delivery_cursor",
        "metadata_cursor",
        "finish_hold:",
    ];
    for prefix in prefixes {
        for n in 0..19 {
            a.ledger
                .reserve(TEAM, &format!("{prefix}{n}"), json!({}))
                .unwrap();
        }
    }
    for first in b'!'..=b'~' {
        a.ledger
            .reserve(
                TEAM,
                &format!("{}:future-key", char::from(first)),
                json!({}),
            )
            .unwrap();
    }
    for _ in 0..20 {
        a.maintain(&h.store).unwrap();
    }
    assert!(
        a.ledger.page(TEAM, "").unwrap().is_empty(),
        "a cursor must not miss earlier or future prefixes"
    );
}
#[test]
fn terminal_maintenance_reaches_metadata_beyond_protected_originals() {
    let mut h = Harness::new();
    h.run(&person(), crate::teams::ops::Operation::Stop)
        .unwrap();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    for n in 0..220 {
        a.ledger
            .reserve(TEAM, &format!("delivery:{n:04}"), json!({}))
            .unwrap();
    }
    for key in [
        "!early",
        "cancel_cursor",
        "cancel_recovery_cursor",
        "z-later",
    ] {
        a.ledger.reserve(TEAM, key, json!({})).unwrap();
    }
    for _ in 0..10 {
        a.maintain(&h.store).unwrap();
    }
    for key in [
        "!early",
        "cancel_cursor",
        "cancel_recovery_cursor",
        "z-later",
    ] {
        assert!(a.ledger.get(TEAM, key).unwrap().is_none(), "{key}");
    }
    assert_eq!(
        a.ledger.prefix_page(TEAM, "delivery:", "").unwrap().len(),
        200
    );
}

#[test]
fn terminal_frontiers_survive_pending_work_and_are_reclaimed_after_it_settles() {
    use crate::teams::ops::{CancelState, KeyClearState, Operation};
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.run(&person(), Operation::Stop).unwrap();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    for key in ["cancel_cursor", "cancel_recovery_cursor"] {
        a.ledger.reserve(TEAM, key, json!("frontier")).unwrap();
    }
    a.maintain(&h.store).unwrap();
    assert!(a.ledger.get(TEAM, "cancel_cursor").unwrap().is_some());
    let pending = h.store.pending_cancels(TEAM, 1, 0).unwrap().remove(0);
    h.run(
        &service(),
        Operation::RecordCancel {
            node,
            decision: pending.decision,
            state: CancelState::Refused,
            reason: "The command refused".into(),
            attempt: pending.attempt,
            attempt_id: pending.attempt_id,
        },
    )
    .unwrap();
    h.run(
        &service(),
        Operation::RecordKeyClear {
            node,
            state: KeyClearState::Cleared,
            reason: String::new(),
        },
    )
    .unwrap();
    for _ in 0..3 {
        a.maintain(&h.store).unwrap();
    }
    assert!(a.ledger.get(TEAM, "cancel_cursor").unwrap().is_none());
    assert!(a
        .ledger
        .get(TEAM, "cancel_recovery_cursor")
        .unwrap()
        .is_none());
}
