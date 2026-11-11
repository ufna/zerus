use super::*;

fn saved(h: &Harness, id: &str) -> u32 {
    h.store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM requests WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
}
fn pause_request(h: &Harness) -> Request {
    request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::Pause,
    )
}
fn retry_once(h: &mut Harness, request: &Request) {
    let first = crate::teams::ops::apply(&mut h.store, &person(), request, 1234).unwrap();
    assert_eq!(
        h.store.team(TEAM).unwrap().unwrap().status,
        TeamStatus::Paused
    );
    assert_eq!(saved(h, &request.request_id), 1);
    let count = h.store.journal(TEAM, 200, None).unwrap().rows.len();
    assert_eq!(
        crate::teams::ops::apply(&mut h.store, &person(), request, 1235).unwrap(),
        first
    );
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), count);
}

#[test]
fn statement_failures_are_not_saved_refusals_and_the_same_request_is_processed_after_repair() {
    for trigger in [
        "CREATE TRIGGER fail_write BEFORE UPDATE OF status ON teams BEGIN SELECT RAISE(ABORT,'fixture statement failure'); END;",
        "CREATE TRIGGER fail_write BEFORE INSERT ON journal BEGIN SELECT RAISE(ABORT,'fixture journal failure'); END;",
        "CREATE TRIGGER fail_write BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END;",
    ] {
        let mut h=Harness::new();
        let request=pause_request(&h);
        let before=h.store.team(TEAM).unwrap();
        let journal=h.store.journal(TEAM,200,None).unwrap();
        h.store.db.as_ref().unwrap().execute_batch(trigger).unwrap();
        let problem=crate::teams::ops::apply(&mut h.store,&person(),&request,1234).unwrap_err();
        assert_eq!(problem.code,"storage_failed");
        assert_eq!(saved(&h,&request.request_id),0);
        assert_eq!(h.store.team(TEAM).unwrap(),before);
        assert_eq!(h.store.journal(TEAM,200,None).unwrap(),journal);
        assert!(h.store.db.as_ref().unwrap().is_autocommit());
        h.store.db.as_ref().unwrap().execute_batch("DROP TRIGGER fail_write").unwrap();
        retry_once(&mut h,&request);
    }
}

#[test]
fn commit_failure_leaves_no_receipt_and_the_same_request_is_processed_after_repair() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let mut h = Harness::new();
    let request = pause_request(&h);
    let before = h.store.team(TEAM).unwrap();
    let journal = h.store.journal(TEAM, 200, None).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    h.store.db.as_ref().unwrap().commit_hook(Some(move || {
        observed.fetch_add(1, Ordering::SeqCst);
        true
    }));
    let problem = crate::teams::ops::apply(&mut h.store, &person(), &request, 1234).unwrap_err();
    h.store
        .db
        .as_ref()
        .unwrap()
        .commit_hook(None::<fn() -> bool>);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(problem.code, "storage_failed");
    assert_eq!(saved(&h, &request.request_id), 0);
    assert_eq!(h.store.team(TEAM).unwrap(), before);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap(), journal);
    assert!(h.store.db.as_ref().unwrap().is_autocommit());
    retry_once(&mut h, &request);
}

#[test]
fn a_logical_invalid_state_remains_saved_but_a_failed_refusal_write_does_not() {
    let mut h = Harness::new();
    let r = request(1, Operation::Resume);
    h.store.db.as_ref().unwrap().execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture refusal receipt failure'); END;").unwrap();
    let failed = crate::teams::ops::apply(&mut h.store, &person(), &r, 1234).unwrap_err();
    assert_eq!(failed.code, "storage_failed");
    assert_eq!(saved(&h, &r.request_id), 0);
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_write")
        .unwrap();
    let logical = crate::teams::ops::apply(&mut h.store, &person(), &r, 1235).unwrap_err();
    assert_eq!(logical.code, code::INVALID_STATE);
    assert_eq!(saved(&h, &r.request_id), 1);
    h.run(&person(), Operation::Pause).unwrap();
    assert_eq!(
        crate::teams::ops::apply(&mut h.store, &person(), &r, 1236).unwrap_err(),
        logical
    );
}

#[test]
fn a_lost_success_reply_reopens_and_replays_the_same_request_once() {
    let mut h = Harness::new();
    let r = pause_request(&h);
    let reply = crate::teams::ops::apply(&mut h.store, &person(), &r, 1234).unwrap();
    h.store = Store::open(h.temp.path()).unwrap();
    assert_eq!(
        crate::teams::ops::apply(&mut h.store, &person(), &r, 1235).unwrap(),
        reply
    );
    assert_eq!(saved(&h, &r.request_id), 1);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), 2);
}

#[test]
fn a_session_bound_elsewhere_is_a_saved_logical_refusal() {
    let mut h = Harness::new();
    h.bind("m1");
    let m = h.store.member(TEAM, "m1").unwrap().unwrap();
    let r = request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::SetMemberSession {
            member: "m2".into(),
            session: m.session.unwrap(),
            account: m.account,
            worktree: m.worktree,
            ownership_nonce: m.ownership_nonce,
        },
    );
    let problem = crate::teams::ops::apply(&mut h.store, &service(), &r, 1234).unwrap_err();
    assert_eq!(problem.code, code::INVALID_STATE);
    assert_eq!(saved(&h, &r.request_id), 1);
    assert_eq!(
        crate::teams::ops::apply(&mut h.store, &service(), &r, 1235).unwrap_err(),
        problem
    );
    assert!(h
        .store
        .member(TEAM, "m2")
        .unwrap()
        .unwrap()
        .session
        .is_none());
}

#[test]
fn reusing_an_effect_identity_is_a_saved_logical_refusal() {
    let mut h = Harness::new();
    h.bind("m1");
    let first = h.add(&person(), 1);
    let id = uuid::Uuid::new_v4().to_string();
    h.run(
        &service(),
        Operation::HandoutIntent {
            node: first,
            member: "m1".into(),
            effect_request_id: id.clone(),
        },
    )
    .unwrap();
    h.run(
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: id.clone(),
            outcome: HandoutOutcome::Refused {
                reason: "Writer refused".into(),
            },
        },
    )
    .unwrap();
    let second = h.add(&person(), 2);
    let r = request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::HandoutIntent {
            node: second,
            member: "m1".into(),
            effect_request_id: id,
        },
    );
    let problem = crate::teams::ops::apply(&mut h.store, &service(), &r, 1234).unwrap_err();
    assert_eq!(problem.code, code::INVALID_STATE);
    assert_eq!(saved(&h, &r.request_id), 1);
    assert_eq!(h.store.effects(TEAM, 200, None).unwrap().rows.len(), 1);
}
