use super::*;
use crate::teams::ops::tests::{Harness, TEAM};
#[test]
fn blocking_a_row_releases_its_live_admission_immediately() {
    let h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut r = Runtime::open(h.temp.path(), &t.home, "epoch").unwrap();
    let mut row = Row::new(&t, "lead", "epoch", State::WorktreeIntent);
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    r.ledger.as_mut().unwrap().insert(&row, 1).unwrap();
    r.admitted = Some((t.id, row.original.clone()));
    r.block(&mut row, uncertain("Synthetic interrupted effect"), 2)
        .unwrap();
    assert!(!r.busy());
}
#[test]
fn owner_card_text_is_bounded_by_bytes_at_a_utf8_boundary() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut r = Runtime::open(h.temp.path(), &t.home, "epoch").unwrap();
    let mut row = Row::new(&t, "lead", "epoch", State::Blocked);
    row.problem =
        Some(Problem::new(code::INVALID_STATE, "⚙".repeat(5000)).with_refs(["⏳".repeat(3000)]));
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    r.ledger.as_mut().unwrap().insert(&row, 1).unwrap();
    r.card(&mut h.store, &mut row, 2).unwrap();
    let card = h.store.card(TEAM, row.card_id.unwrap()).unwrap().unwrap();
    assert!(card.text.len() <= 3072);
    assert!(std::str::from_utf8(card.text.as_bytes()).is_ok());
}

/// zerus-63rru.91: the worker's SetMemberSession write keeps one request ID,
/// so a commit whose answer was lost is replayed, never repeated.
fn session_write(home: &str) -> (Operation, zerus_team_plan::model::SessionKey) {
    let key = zerus_team_plan::model::SessionKey {
        machine: home.into(),
        session_id: "launch_id:00000000-0000-4000-8000-000000000009".to_string(),
    };
    (
        Operation::SetMemberSession {
            member: "m2".into(),
            session: key.clone(),
            account: "dev".into(),
            worktree: "/tmp/claude-1000/launch-ids".into(),
            ownership_nonce: "nonce".into(),
        },
        key,
    )
}

#[test]
fn a_lost_answer_is_replayed_under_the_same_id_and_writes_once() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let mut h = Harness::new();
    h.bind("m1");
    let t = h.store.team(TEAM).unwrap().unwrap();
    let (op, key) = session_write(&t.home);
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    // The commit's result is lost: the ops answers storage_failed, the
    // reservation stays, and the same ID replays the write once.
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    h.store.db.as_ref().unwrap().commit_hook(Some(move || {
        observed.fetch_add(1, Ordering::SeqCst);
        true
    }));
    let failed = apply(&mut h.store, &t, op.clone(), &mut row.session_request, 1000).unwrap_err();
    h.store
        .db
        .as_ref()
        .unwrap()
        .commit_hook(None::<fn() -> bool>);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(failed.code, "storage_failed");
    let id = row.session_request.as_ref().unwrap().request_id.clone();
    apply(&mut h.store, &t, op, &mut row.session_request, 1001).unwrap();
    let saved: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, 1, "the write was recorded twice");
    assert_eq!(
        h.store.member(TEAM, "m2").unwrap().unwrap().session,
        Some(key)
    );
}

#[test]
fn storage_failed_keeps_the_id_and_the_retry_writes_once() {
    let mut h = Harness::new();
    h.bind("m1");
    let t = h.store.team(TEAM).unwrap().unwrap();
    let (op, _) = session_write(&t.home);
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END;")
        .unwrap();
    let failed = apply(&mut h.store, &t, op.clone(), &mut row.session_request, 1000).unwrap_err();
    assert_eq!(failed.code, "storage_failed");
    assert!(
        row.session_request.is_some(),
        "a storage failure dropped the reserved request; the retry would take a new ID"
    );
    let id = row.session_request.as_ref().unwrap().request_id.clone();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_write")
        .unwrap();
    apply(&mut h.store, &t, op, &mut row.session_request, 1001).unwrap();
    let saved: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, 1);
}

#[test]
fn a_restart_between_the_intent_and_the_write_replays_the_same_id() {
    let mut h = Harness::new();
    h.bind("m1");
    let t = h.store.team(TEAM).unwrap().unwrap();
    let (op, _) = session_write(&t.home);
    let machine = t.home.clone();
    let mut r = Runtime::open(h.temp.path(), &machine, "epoch").unwrap();
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    // The first pass reserves the ID against a store that fails the write.
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END;")
        .unwrap();
    let _ = apply(&mut h.store, &t, op.clone(), &mut row.session_request, 1000).unwrap_err();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_write")
        .unwrap();
    let id = row.session_request.as_ref().unwrap().request_id.clone();
    r.ledger.as_mut().unwrap().insert(&row, 1).unwrap();
    drop(r);
    // The restart reads the row and replays the persisted request.
    let mut r = Runtime::open(h.temp.path(), &machine, "epoch").unwrap();
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    let mut loaded = r.ledger.as_mut().unwrap().get(TEAM, "m2").unwrap().unwrap();
    apply(&mut h.store, &t, op, &mut loaded.session_request, 1001).unwrap();
    let replayed: String = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT id FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(replayed, id, "the restart wrote under a new ID");
    assert!(loaded.session_request.is_none(), "a success kept the reservation");
    let saved: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, 1);
}

/// Round 2: a successful write clears the reservation, so the row's next
/// intent gets its own ID.
fn open_card(h: &mut Harness, n: u32) -> u64 {
    let t = h.store.team(TEAM).unwrap().unwrap();
    h.run(
        &Actor::Service {
            person: t.owner,
            team: TEAM.into(),
        },
        Operation::OpenCard {
            kind: "launch_blocked".into(),
            text: format!("card {n}").into(),
            node: None,
            decisions: Some(vec!["retry".into(), "acknowledged".into()]),
        },
    )
    .unwrap()
    .item
    .unwrap()
}

#[test]
fn two_cards_closed_one_after_the_other_on_one_row_are_both_closed() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let first = open_card(&mut h, 1);
    let second = open_card(&mut h, 2);
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    let close = |card: u64| Operation::CloseCard {
        card,
        reason: "the launch's state changed".into(),
    };
    apply(&mut h.store, &t, close(first), &mut row.close_request, 1000).unwrap();
    apply(&mut h.store, &t, close(second), &mut row.close_request, 1001).unwrap();
    assert_eq!(h.store.card(TEAM, first).unwrap().unwrap().state, "closed");
    assert_eq!(h.store.card(TEAM, second).unwrap().unwrap().state, "closed");
}

#[test]
fn a_relaunch_into_the_same_row_writes_the_new_session() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let key1 = zerus_team_plan::model::SessionKey {
        machine: t.home.clone(),
        session_id: "launch_id:00000000-0000-4000-8000-000000000001".into(),
    };
    let key2 = zerus_team_plan::model::SessionKey {
        machine: t.home.clone(),
        session_id: "launch_id:00000000-0000-4000-8000-000000000002".into(),
    };
    let session = |member: &str,
                    key: zerus_team_plan::model::SessionKey|
     -> Operation {
        Operation::SetMemberSession {
            member: member.into(),
            session: key,
            account: "dev".into(),
            worktree: "/tmp/claude-1000/launch-ids".into(),
            ownership_nonce: "nonce".into(),
        }
    };
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    apply(
        &mut h.store,
        &t,
        session("m2", key1.clone()),
        &mut row.session_request,
        1000,
    )
    .unwrap();
    // A relaunch into the same row writes the next member's session under a
    // new request, never a replay of the first write.
    apply(
        &mut h.store,
        &t,
        session("m3", key2.clone()),
        &mut row.session_request,
        1001,
    )
    .unwrap();
    assert_eq!(h.store.member(TEAM, "m3").unwrap().unwrap().session, Some(key2));
}

#[test]
fn a_changed_intent_settles_the_kept_write_first_and_writes_both() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let key = zerus_team_plan::model::SessionKey {
        machine: t.home.clone(),
        session_id: "launch_id:00000000-0000-4000-8000-000000000003".into(),
    };
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    // The session write's answer is lost: storage_failed keeps the request.
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END;")
        .unwrap();
    let failed = apply(
        &mut h.store,
        &t,
        Operation::SetMemberSession {
            member: "m2".into(),
            session: key.clone(),
            account: "dev".into(),
            worktree: "/tmp/claude-1000/launch-ids".into(),
            ownership_nonce: "nonce".into(),
        },
        &mut row.session_request,
        1000,
    )
    .unwrap_err();
    assert_eq!(failed.code, "storage_failed");
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_write")
        .unwrap();
    // The same field now carries a different intent: the kept session write
    // is replayed first, then the card close gets its own request.
    let card = open_card(&mut h, 1);
    apply(
        &mut h.store,
        &t,
        Operation::CloseCard {
            card,
            reason: "the intent changed".into(),
        },
        &mut row.session_request,
        1001,
    )
    .unwrap();
    assert_eq!(h.store.member(TEAM, "m2").unwrap().unwrap().session, Some(key));
    assert_eq!(h.store.card(TEAM, card).unwrap().unwrap().state, "closed");
}

#[test]
fn a_crash_between_the_commit_and_the_ledger_save_makes_no_second_write() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let card = open_card(&mut h, 1);
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    // The close commits but the row (with its reservation) is lost.
    apply(
        &mut h.store,
        &t,
        Operation::CloseCard {
            card,
            reason: "the launch's state changed".into(),
        },
        &mut row.close_request,
        1000,
    )
    .unwrap();
    let journal = h.store.journal(TEAM, 200, None).unwrap().rows.len();
    // The restart's pass has the intent but no reservation; the caller's
    // pre-check sees the committed close and writes nothing.
    let mut restarted = Row::new(&t, "m2", "epoch", State::Submitted);
    restarted.card_id = Some(card);
    super::Runtime::open(h.temp.path(), &t.home, "epoch")
        .unwrap()
        .close_stale(&mut h.store, &t, &mut restarted, 1001)
        .unwrap();
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), journal);
}

#[test]
fn a_row_with_no_reservation_serializes_without_the_keys() {
    let h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let row = Row::new(&t, "m2", "epoch", State::Submitted);
    let value = serde_json::to_value(&row).unwrap();
    let object = value.as_object().unwrap();
    assert!(!object.contains_key("session_request"), "{value}");
    assert!(!object.contains_key("close_request"), "{value}");
}
