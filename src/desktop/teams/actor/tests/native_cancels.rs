use super::super::{
    cancel::{Fact, Pending},
    native::Native,
    ports::TaskReply,
};
use super::*;
use crate::teams::ops::Operation;

fn cancelling() -> (Harness, Pending) {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.run(
        &person(),
        Operation::Cancel {
            node,
            reason: "Cancel this work".into(),
        },
    )
    .unwrap();
    let row = Native::pending_cancels(&h.store, TEAM, 0)
        .unwrap()
        .remove(0);
    (h, row)
}
fn reply(pending: &Pending, state: &str) {
    crate::desktop::teams::child::FIXTURE_EXIT.with(|s| {
        *s.borrow_mut() = Some(
            json!({"schema":1,"team":TEAM,"node":pending.node,"state":state,"error":"","attempt":pending.attempt,"attempt_id":pending.attempt_id})
                .to_string()
                .into_bytes(),
        )
    });
}
#[test]
fn native_cancel_reads_the_real_pending_decision_and_accepts_live_or_terminal_teams() {
    for status in ["running", "paused", "stopped", "archived"] {
        let (mut h, p) = cancelling();
        if status == "paused" {
            h.run(&person(), Operation::Pause).unwrap();
        }
        if matches!(status, "stopped" | "archived") {
            h.run(&person(), Operation::Stop).unwrap();
        }
        if status == "archived" {
            h.run(&person(), Operation::Archive).unwrap();
        }
        let t = h.store.team(TEAM).unwrap().unwrap();
        let current = Native::pending_cancels(&h.store, TEAM, 0)
            .unwrap()
            .remove(0);
        assert_eq!(current.node, p.node);
        reply(&current, "released");
        let n = super::production::native(&h);
        let answer = n
            .run_cancel(&t, &current, &uuid::Uuid::new_v4().to_string())
            .unwrap();
        assert_eq!(answer.reply, TaskReply::Released);
        assert_eq!(answer.attempt.unwrap().number, current.attempt);
    }
}
#[test]
fn cancel_admission_refuses_a_changed_decision_generation_task_or_home_before_spawn() {
    for sql in [
        "UPDATE decisions SET kind='return'",
        "UPDATE decisions SET state='settled'",
        "UPDATE decisions SET generation=generation+1",
        "UPDATE task_observations SET attempt=99,attempt_id='other-attempt'",
        "UPDATE node_identity SET task_uuid='00000000-0000-4000-8000-000000000077'",
        "UPDATE teams SET home='00000000-0000-4000-8000-000000000077'",
    ] {
        let (h, p) = cancelling();
        let t = h.store.team(TEAM).unwrap().unwrap();
        let n = super::production::native(&h);
        h.store.db.as_ref().unwrap().execute(sql, []).unwrap();
        reply(&p, "cancelled");
        let seen = std::rc::Rc::new(std::cell::Cell::new(false));
        let s = seen.clone();
        crate::desktop::original_child::AFTER_FINAL_CHECK
            .with(|slot| *slot.borrow_mut() = Some(Box::new(move || s.set(true))));
        let result = n.run_cancel(&t, &p, &uuid::Uuid::new_v4().to_string());
        crate::desktop::original_child::AFTER_FINAL_CHECK.with(|s| s.borrow_mut().take());
        assert!(result.is_err(), "{sql}");
        assert!(!seen.get(), "{sql}");
    }
}
#[test]
fn a_cancel_fact_settles_a_saved_refusal_only_when_the_exact_decision_has_gone() {
    let (mut h, p) = cancelling();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut f = Fact {
        request_id: uuid::Uuid::new_v4().to_string(),
        revision: t.revision,
        pending: p.clone(),
        refused: true,
        reason: "".into(),
        attempt: p.attempt,
        attempt_id: p.attempt_id.clone(),
    };
    assert!(Native::save_cancel(&mut h.store, &t, &f, 100).is_err());
    assert!(h.store.request_refused(TEAM, &f.request_id).unwrap());
    assert_eq!(
        Native::pending_cancels(&h.store, TEAM, 0).unwrap(),
        vec![p.clone()]
    );
    h.cancel_fact(p.node);
    f.request_id = uuid::Uuid::new_v4().to_string();
    f.revision = h.store.team(TEAM).unwrap().unwrap().revision;
    f.refused = false;
    Native::save_cancel(&mut h.store, &t, &f, 101).unwrap();
    assert!(h.store.request_refused(TEAM, &f.request_id).unwrap());
}
#[test]
fn cancel_fact_storage_failure_is_never_read_as_a_saved_refusal() {
    let (mut h, p) = cancelling();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let db = rusqlite::Connection::open(h.temp.path().join("teams/teams-v1.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_request BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END").unwrap();
    let f = Fact {
        request_id: uuid::Uuid::new_v4().to_string(),
        revision: t.revision,
        pending: p.clone(),
        refused: false,
        reason: "".into(),
        attempt: p.attempt,
        attempt_id: p.attempt_id.clone(),
    };
    assert!(Native::save_cancel(&mut h.store, &t, &f, 100).is_err());
    assert!(!h.store.request_refused(TEAM, &f.request_id).unwrap());
    assert_eq!(Native::pending_cancels(&h.store, TEAM, 0).unwrap(), vec![p]);
}

#[test]
fn a_missing_saved_refusal_never_settles_a_cancel_even_after_the_decision_moved() {
    let (mut h, p) = cancelling();
    h.cancel_fact(p.node);
    let t = h.store.team(TEAM).unwrap().unwrap();
    h.store.db.as_ref().unwrap().execute_batch(
        "CREATE TRIGGER reject_receipt BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END"
    ).unwrap();
    let fact = Fact {
        request_id: uuid::Uuid::new_v4().to_string(),
        revision: t.revision,
        pending: p,
        refused: false,
        reason: String::new(),
        attempt: 0,
        attempt_id: String::new(),
    };
    assert!(Native::save_cancel(&mut h.store, &t, &fact, 100).is_err());
    assert!(!h.store.request_refused(TEAM, &fact.request_id).unwrap());
    assert!(Native::pending_cancels(&h.store, TEAM, 0)
        .unwrap()
        .is_empty());
}
#[test]
fn a_cancel_fact_records_the_reply_pair_even_when_it_differs_from_the_pending_snapshot() {
    let mut h = Harness::new();
    let (node, _) = h.reported();
    h.run(
        &person(),
        Operation::Cancel {
            node,
            reason: "Release this result".into(),
        },
    )
    .unwrap();
    let pending = Native::pending_cancels(&h.store, TEAM, 0)
        .unwrap()
        .remove(0);
    let mut observed = h.observation(node, crate::teams::ops::TaskStage::InReview);
    observed.attempt = 7;
    observed.attempt_id = "reply-attempt".into();
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: observed,
        },
    )
    .unwrap();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let fact = Fact {
        request_id: uuid::Uuid::new_v4().to_string(),
        revision: t.revision,
        pending,
        refused: false,
        reason: String::new(),
        attempt: 7,
        attempt_id: "reply-attempt".into(),
    };
    Native::save_cancel(&mut h.store, &t, &fact, 100).unwrap();
    assert_eq!(h.node(node).state, NodeState::Cancelled);
    assert!(Native::pending_cancels(&h.store, TEAM, 0)
        .unwrap()
        .is_empty());
}
#[test]
fn production_cancel_replies_require_the_actual_attempt_pair_and_the_exact_echo() {
    use super::super::native::cancel_answer;
    use crate::desktop::teams::child::Ended;
    let input = json!({"schema":1,"team":TEAM,"node":1});
    for state in ["cancelled", "released"] {
        let value = json!({"schema":1,"team":TEAM,"node":1,"state":state,"error":"","attempt":7,"attempt_id":"reply-attempt"});
        let result = cancel_answer(
            Ended::Exited {
                code: 0,
                output: value.to_string().into_bytes(),
            },
            &input,
        )
        .unwrap();
        let pair = result.attempt.unwrap();
        assert_eq!((pair.number, pair.id.as_str()), (7, "reply-attempt"));
        for field in ["attempt", "attempt_id", "schema", "team", "node"] {
            let mut bad = value.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                cancel_answer(
                    Ended::Exited {
                        code: 0,
                        output: bad.to_string().into_bytes()
                    },
                    &input
                )
                .is_err(),
                "{state} {field}"
            );
        }
        assert!(cancel_answer(
            Ended::Exited {
                code: 1,
                output: value.to_string().into_bytes()
            },
            &input
        )
        .is_err());
    }
    let value =
        json!({"schema":1,"team":TEAM,"node":1,"state":"stale","error":"Observe this task"});
    let answer = cancel_answer(
        Ended::Exited {
            code: 0,
            output: value.to_string().into_bytes(),
        },
        &input,
    )
    .unwrap();
    assert_eq!(answer.reply, TaskReply::Stale);
    assert!(answer.attempt.is_none());
}
#[test]
fn cancel_holds_its_admission_until_spawn_and_then_releases_it() {
    let (h, pending) = cancelling();
    let path = h.temp.path().join("teams/teams-v1.sqlite3");
    let first = path.clone();
    let mark = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = mark.clone();
    crate::desktop::original_child::AFTER_FINAL_CHECK.with(|s| {
        *s.borrow_mut() = Some(Box::new(move || {
            let db = rusqlite::Connection::open(first).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            assert!(db.execute_batch("BEGIN IMMEDIATE").is_err());
            seen.set(1);
        }))
    });
    let seen = mark.clone();
    crate::desktop::original_child::AFTER_SPAWN.with(|s| {
        *s.borrow_mut() = Some(Box::new(move || {
            let db = rusqlite::Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
            seen.set(2);
        }))
    });
    reply(&pending, "released");
    let t = h.store.team(TEAM).unwrap().unwrap();
    super::production::native(&h)
        .run_cancel(&t, &pending, &uuid::Uuid::new_v4().to_string())
        .unwrap();
    assert_eq!(mark.get(), 2);
}
