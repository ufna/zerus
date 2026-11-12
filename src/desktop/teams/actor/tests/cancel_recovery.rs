//! Cancel receipts survive revision changes; one halted row owns only its retry.
use super::super::{
    cancel::{Answer, Attempt, Fact, Pending},
    native::Native,
    ports::{DeliveryResult, Ports, Result},
    Runtime,
};
use super::*;
use crate::teams::ops::{CancelState, HandoutOutcome, KeyClearState, Operation, TaskStage};
struct Recovery {
    native: Native,
    live: bool,
    pair: Attempt,
    loss: Option<bool>,
    only: Option<u32>,
    calls: Vec<(String, u32)>,
    facts: Vec<Fact>,
    reject_exit: bool,
    cleared: Vec<(String, u32)>,
}
impl Ports for Recovery {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> Result<zerus_replica::Changes> {
        unreachable!()
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        unreachable!()
    }
    fn free(&mut self, _: &Team, _: &Member) -> Result<bool> {
        Ok(false)
    }
    fn deliver(&mut self, _: &Team, _: &Node, _: &Member, _: &str) -> DeliveryResult {
        unreachable!()
    }
    fn cancels(&mut self, store: &Store, team: &str, after: u32) -> Result<Vec<Pending>> {
        Ok(Native::pending_cancels(store, team, after)?
            .into_iter()
            .filter(|p| self.only.is_none_or(|n| n == p.node))
            .collect())
    }
    fn cancel(&mut self, team: &Team, pending: &Pending, original: &str) -> Result<Answer> {
        self.calls.push((team.id.clone(), pending.node));
        let pair = if pending.node == 1 {
            self.pair.clone()
        } else {
            Attempt {
                number: pending.attempt,
                id: pending.attempt_id.clone(),
            }
        };
        let state = if self.live { "released" } else { "cancelled" };
        crate::desktop::teams::child::FIXTURE_EXIT.with(|s| {
            *s.borrow_mut() = Some(
                json!({"schema":1,"team":team.id,"node":pending.node,"state":state,"error":"",
                "attempt":pair.number,"attempt_id":pair.id})
                .to_string()
                .into_bytes(),
            )
        });
        self.native.run_cancel(team, pending, original)
    }
    fn record_cancel(
        &mut self,
        store: &mut Store,
        team: &Team,
        fact: &Fact,
        now: i64,
    ) -> Result<()> {
        self.facts.push(fact.clone());
        let lose = self.loss == Some(fact.refused);
        if lose {
            self.loss = None;
            store.db.as_ref().unwrap().execute_batch("CREATE TRIGGER lose_cancel_receipt BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT,'fixture lost receipt'); END").unwrap();
        }
        let mut attempted = fact.clone();
        if self.reject_exit && fact.pending.node == 1 && fact.refused {
            attempted.reason.clear();
        }
        let result = Native::save_cancel(store, team, &attempted, now);
        if lose {
            store
                .db
                .as_ref()
                .unwrap()
                .execute_batch("DROP TRIGGER lose_cancel_receipt")
                .unwrap();
            assert_eq!(
                result.as_ref().unwrap_err().code,
                zerus_team_plan::problem::code::STORAGE_FAILED
            );
        }
        result
    }
    fn terminal_timers(
        &mut self,
        _: &mut super::super::auxiliary::State,
        store: &mut Store,
        now: i64,
    ) -> Result<()> {
        for id in store.keyed_teams("", 16)? {
            for key in store.keyed_tasks(&id, 0, 200)? {
                super::super::write(
                    store,
                    &id,
                    Operation::RecordKeyClear {
                        node: key.node,
                        state: KeyClearState::Cleared,
                        reason: String::new(),
                    },
                    now,
                )?;
                self.cleared.push((id.clone(), key.node));
            }
        }
        Ok(())
    }
}
fn case(status: &str, exit: bool) -> (Harness, Runtime, Recovery) {
    let mut h = Harness::new();
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.add(&person(), 2);
    let id = if status == "live" {
        String::new()
    } else {
        let id = h.intent(node, "m1");
        h.outcome(
            &id,
            HandoutOutcome::Submitted {
                receipt: "submitted".into(),
            },
        );
        id
    };
    if status == "live" {
        h.run(
            &person(),
            Operation::Cancel {
                node,
                reason: "Cancel".into(),
            },
        )
        .unwrap();
    } else {
        h.run(&person(), Operation::Stop).unwrap();
        if status == "archived" {
            h.run(&person(), Operation::Archive).unwrap();
        }
    }
    let native = super::production::native(&h);
    let runtime = Runtime::open(h.temp.path(), &native.machine).unwrap();
    let p = Recovery {
        native,
        live: status == "live",
        pair: if exit {
            Attempt {
                number: 9,
                id: "not-the-node".into(),
            }
        } else {
            Attempt {
                number: u64::from(status != "live"),
                id,
            }
        },
        loss: Some(exit),
        only: Some(node),
        calls: vec![],
        facts: vec![],
        reject_exit: false,
        cleared: vec![],
    };
    (h, runtime, p)
}
fn team_write(h: &mut Harness, live: bool) {
    if live {
        h.observe(2, TaskStage::Todo);
    } else {
        let c = h
            .store
            .pending_cancels(TEAM, 16, 0)
            .unwrap()
            .into_iter()
            .find(|c| c.node == 2)
            .unwrap();
        h.run(
            &service(),
            Operation::RecordCancel {
                node: c.node,
                decision: c.decision,
                state: CancelState::Refused,
                reason: "Sibling progress".into(),
                attempt: c.attempt,
                attempt_id: c.attempt_id,
            },
        )
        .unwrap();
    }
}
fn check_stale(status: &str, exit: bool) {
    let (mut h, mut a, mut p) = case(status, exit);
    let team = h.store.team(TEAM).unwrap().unwrap();
    a.cancels(&mut h.store, &mut p, &team, 100).unwrap();
    assert!(p.loss.is_none());
    let lost = p.facts.last().unwrap().request_id.clone();
    assert!(!h.store.request_refused(TEAM, &lost).unwrap());
    team_write(&mut h, p.live);
    for pass in 1..=5 {
        let machine = a.machine.clone();
        drop(a);
        a = Runtime::open(h.temp.path(), &machine).unwrap();
        let t = h.store.team(TEAM).unwrap().unwrap();
        a.cancels(&mut h.store, &mut p, &t, 100 + pass * 60_000)
            .unwrap();
    }
    assert_eq!(p.calls.len(), 1, "{status}, exit={exit}");
    assert!(h
        .store
        .pending_cancels(TEAM, 16, 0)
        .unwrap()
        .iter()
        .all(|c| c.node != 1));
    assert!(
        h.store.request_refused(TEAM, &lost).unwrap(),
        "stale request is spent"
    );
    let repeats: Vec<_> = p.facts.iter().filter(|f| f.request_id == lost).collect();
    assert_eq!(repeats.len(), 2, "lost reply must first retry its UUID");
    let saved = p.facts.last().unwrap();
    assert_ne!(saved.request_id, lost);
    assert_eq!(saved.refused, exit, "{status}");
    assert_eq!(saved.released, p.live && !exit);
    assert!(!h.store.request_refused(TEAM, &saved.request_id).unwrap());
    assert!(a
        .ledger
        .prefix_page(TEAM, "cancel:", "")
        .unwrap()
        .is_empty());
}
#[test]
fn a_lost_original_cancel_fact_replays_stale_then_records_the_answer_with_a_fresh_uuid() {
    for status in ["stopped", "archived", "live"] {
        check_stale(status, false);
    }
}
#[test]
fn a_lost_exit_fact_replays_stale_without_becoming_a_second_rule_refusal() {
    for status in ["stopped", "archived", "live"] {
        check_stale(status, true);
    }
}
