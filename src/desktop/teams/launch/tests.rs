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
    let mut h = Harness::new();
    h.bind("m1");
    let t = h.store.team(TEAM).unwrap().unwrap();
    let (op, key) = session_write(&t.home);
    let mut row = Row::new(&t, "m2", "epoch", State::Submitted);
    apply(&mut h.store, &t, op.clone(), &mut row.session_request, 1000).unwrap();
    let id = row.session_request.as_ref().unwrap().request_id.clone();
    assert_eq!(h.store.member(TEAM, "m2").unwrap().unwrap().session, Some(key.clone()));
    // The answer is lost: the runtime is gone and the store is reopened.
    h.store = crate::teams::store::Store::open(h.temp.path()).unwrap();
    let mut restarted = row.clone();
    apply(&mut h.store, &t, op, &mut restarted.session_request, 1001).unwrap();
    assert_eq!(
        restarted.session_request.as_ref().unwrap().request_id, id,
        "the retry reserved a new ID instead of replaying the persisted one"
    );
    let saved: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, 1, "the write was recorded twice");
    assert_eq!(h.store.member(TEAM, "m2").unwrap().unwrap().session, Some(key));
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
    assert_eq!(
        loaded.session_request.as_ref().unwrap().request_id, id,
        "the restart reserved a new ID instead of the persisted one"
    );
    let saved: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, 1);
}
