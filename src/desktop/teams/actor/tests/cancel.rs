use super::super::{
    cancel::{Answer, Attempt, Fact, Pending},
    ports::{Ports, TaskReply},
};
use super::*;
#[derive(Default)]
struct Fake {
    pending: Vec<Pending>,
    calls: Vec<u32>,
    facts: Vec<Fact>,
    reply: Option<TaskReply>,
    terminal: usize,
    reads: usize,
    move_pair: bool,
    answer_pair: Option<Attempt>,
    missing_pair: bool,
    keep_once: bool,
    always_changed: bool,
    keep_observed: bool,
    keep_recorded: bool,
    answered_cancelled: bool,
    fail_observe: bool,
}
impl Ports for Fake {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: Some(Default::default()),
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        self.reads += 1;
        if self.fail_observe {
            return Err(super::super::problem("fixture observation unavailable"));
        }
        if self.answered_cancelled && !self.keep_observed {
            self.pending.clear();
        }
        if self.move_pair {
            for p in &mut self.pending {
                p.attempt += 1;
                p.attempt_id = format!("observed-{}", p.attempt);
            }
        }
        Ok(())
    }
    fn free(&mut self, _: &Team, _: &Member) -> Result<bool> {
        Ok(false)
    }
    fn deliver(
        &mut self,
        _: &Team,
        _: &Node,
        _: &Member,
        _: &str,
    ) -> super::super::ports::DeliveryResult {
        panic!("No delivery")
    }
    fn cancels(&mut self, _: &Store, _: &str, after: u32) -> Result<Vec<Pending>> {
        Ok(self
            .pending
            .iter()
            .filter(|p| p.node > after)
            .take(16)
            .cloned()
            .collect())
    }
    fn cancel(&mut self, _: &Team, p: &Pending, _: &str) -> Result<Answer> {
        self.calls.push(p.node);
        self.answered_cancelled = matches!(self.reply, None | Some(TaskReply::Cancelled));
        Ok(Answer {
            reply: self.reply.clone().unwrap_or(TaskReply::Cancelled),
            attempt: (!self.missing_pair).then(|| {
                self.answer_pair.clone().unwrap_or(Attempt {
                    number: p.attempt,
                    id: p.attempt_id.clone(),
                })
            }),
        })
    }
    fn cancel_fact_outcome(
        &mut self,
        _: &mut Store,
        _: &Team,
        f: &Fact,
        _: i64,
    ) -> Result<super::super::cancel::FactOutcome> {
        self.facts.push(f.clone());
        if (self.keep_once || self.always_changed) && !f.refused {
            self.keep_once = false;
            self.pending[0].attempt += 1;
            self.pending[0].attempt_id = format!("person-attempt-{}", self.pending[0].attempt);
            return Ok(super::super::cancel::FactOutcome::Changed {
                reason: "The observed execution changed".into(),
            });
        } else if !self.keep_recorded || f.refused {
            self.pending.retain(|p| p.decision != f.pending.decision);
        }
        Ok(super::super::cancel::FactOutcome::Recorded)
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn terminal_timers(
        &mut self,
        _: &mut super::super::auxiliary::State,
        _: &mut Store,
        _: i64,
    ) -> Result<()> {
        assert!(
            self.pending.is_empty(),
            "a key was cleared before cancellation settled"
        );
        self.terminal += 1;
        Ok(())
    }
}
fn setup() -> (Harness, super::super::Runtime, Fake) {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let pending = Pending {
        node,
        generation: 1,
        decision: 1,
        task: h.node(node).task,
        attempt: 0,
        attempt_id: String::new(),
        attempt_conflict: Some(false),
    };
    (
        h,
        a,
        Fake {
            pending: vec![pending],
            ..Default::default()
        },
    )
}
#[test]
fn the_tick_consumes_cancels_and_keeps_their_distinct_final_state() {
    for reply in [TaskReply::Cancelled, TaskReply::Released] {
        let (mut h, mut a, mut p) = setup();
        let released = reply == TaskReply::Released;
        p.reply = Some(reply);
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
        assert_eq!(p.calls, vec![1]);
        assert_eq!(p.facts.len(), usize::from(released));
        if released {
            assert!(!p.facts[0].refused);
            assert!(p.facts[0].released);
        } else {
            assert_eq!(p.reads, 2);
        }
        a.tick(&mut h.store, &mut p, TEAM, 101, false).unwrap();
        assert_eq!(p.calls.len(), 1);
    }
}
#[test]
fn a_no_task_cancel_records_without_a_child() {
    let (mut h, mut a, mut p) = setup();
    p.pending[0].task = None;
    a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
    assert_eq!(p.facts.len(), 1);
    assert!(p.calls.is_empty());
}
#[test]
fn released_cancel_restarts_before_and_after_each_effect_keep_the_known_answer() {
    for point in ["reserved", "effect", "outcome", "recorded"] {
        let (mut h, mut a, mut p) = setup();
        p.reply = Some(TaskReply::Released);
        super::super::cancel::CRASH.with(|s| s.set(Some(point)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.cancels(
                &mut h.store,
                &mut p,
                &h_team(),
                100
            )))
            .is_err(),
            "{point}"
        );
        let machine = a.machine.clone();
        drop(a);
        let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
        a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
        assert!(p.pending.is_empty(), "{point}");
        assert_eq!(
            p.calls.len(),
            if point == "effect" { 2 } else { 1 },
            "{point}"
        );
        assert!(p
            .facts
            .iter()
            .all(|f| f.request_id == p.facts[0].request_id));
    }
}
fn h_team() -> Team {
    fixture().team
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_stop_runs_cancel_before_the_terminal_entry() {
    let (mut h, mut a, mut p) = setup();
    h.run(&person(), crate::teams::ops::Operation::Stop)
        .unwrap();
    a.page(&mut h.store, &mut p, &[], 100, false).unwrap();
    assert_eq!(p.calls, vec![1]);
    assert!(p.pending.is_empty());
    assert_eq!(p.terminal, 1);
}
#[test]
fn a_terminal_cancel_without_an_answer_backs_off_without_sweep_health() {
    let (mut h, mut a, mut p) = setup();
    h.run(&person(), crate::teams::ops::Operation::Stop)
        .unwrap();
    p.reply = Some(TaskReply::CuttingOver {
        reason: "cutting over".into(),
    });
    a.terminal_cancels(&mut h.store, &mut p, 100).unwrap();
    a.terminal_cancels(&mut h.store, &mut p, 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    p.reply = Some(TaskReply::Refused {
        reason: "delivery is unknown".into(),
    });
    a.terminal_cancels(&mut h.store, &mut p, 60_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert!(p.pending.is_empty());
    assert!(p.facts[0].refused);
    assert!(crate::desktop::teams::health(h.temp.path()).is_none());
}

#[test]
fn stale_cancel_observes_and_uses_the_new_pair_after_its_backoff_without_a_card() {
    let (mut h, mut a, mut p) = setup();
    p.reply = Some(TaskReply::Stale);
    p.move_pair = true;
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.reads, 1);
    assert!(p.facts.is_empty());
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    a.cancels(&mut h.store, &mut p, &h_team(), 60_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.reads, 2);
    a.cancels(&mut h.store, &mut p, &h_team(), 120_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert!(p.facts.is_empty());
    assert!(h.store.cards(TEAM, 16, None).unwrap().rows.is_empty());
}
#[test]
fn a_cancel_fact_uses_the_reply_pair_instead_of_the_pending_pair() {
    let (mut h, mut a, mut p) = setup();
    p.reply = Some(TaskReply::Released);
    p.answer_pair = Some(Attempt {
        number: 7,
        id: "actual-reply".into(),
    });
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    assert_eq!(p.facts.len(), 1);
    assert_eq!(p.facts[0].attempt, 7);
    assert_eq!(p.facts[0].attempt_id, "actual-reply");
    assert_eq!(p.facts[0].pending.attempt, 0);
}

#[test]
fn a_saved_answer_whose_attempt_moved_ends_the_visit_without_a_second_child() {
    let (mut h, mut a, mut p) = setup();
    p.reply = Some(TaskReply::Released);
    super::super::cancel::CRASH.with(|s| s.set(Some("outcome")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.cancels(
            &mut h.store,
            &mut p,
            &h_team(),
            100
        )))
        .is_err()
    );
    p.keep_once = true;
    let machine = a.machine.clone();
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.facts.len(), 1);
    assert_eq!(p.pending.len(), 1);
    a.cancels(&mut h.store, &mut p, &h_team(), 102).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert!(p.pending.is_empty());
}
#[test]
fn a_stale_cancel_keeps_its_backoff_across_a_restart() {
    let (mut h, mut a, mut p) = setup();
    p.reply = Some(TaskReply::Stale);
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    let machine = a.machine.clone();
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    p.reply = Some(TaskReply::Released);
    a.cancels(&mut h.store, &mut p, &h_team(), 60_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.facts.len(), 1);
    assert!(h.store.cards(TEAM, 16, None).unwrap().rows.is_empty());
}
#[test]
fn a_cancel_success_without_attempt_evidence_never_records_a_fact() {
    let (mut h, mut a, mut p) = setup();
    p.missing_pair = true;
    for now in [100, 101] {
        a.cancels(&mut h.store, &mut p, &h_team(), now).unwrap();
    }
    assert_eq!(p.calls.len(), 1);
    assert!(p.facts.is_empty());
    assert!(h.store.cards(TEAM, 16, None).unwrap().rows.is_empty());
    for now in [60_100, 180_100] {
        a.cancels(&mut h.store, &mut p, &h_team(), now).unwrap();
    }
    assert_eq!(p.calls.len(), 3);
    assert!(p.facts.is_empty());
    assert_eq!(h.store.cards(TEAM, 16, None).unwrap().rows.len(), 1);
}
#[test]
fn a_legacy_saved_cancel_without_a_pair_is_settled_by_observation_only() {
    let (mut h, mut a, mut p) = setup();
    p.answered_cancelled = true;
    let key = format!("cancel:{}", p.pending[0].decision);
    a.put(
        TEAM,
        &key,
        &json!({"pending":p.pending[0],"reply":"Cancelled",
        "fact":null,"failures":0,"next_ms":0,"reason":""}),
    )
    .unwrap();
    let machine = a.machine.clone();
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    for now in [100, 101, 60_100] {
        a.cancels(&mut h.store, &mut p, &h_team(), now).unwrap();
    }
    assert_eq!(p.reads, 1);
    assert!(p.calls.is_empty());
    assert!(p.facts.is_empty());
    assert!(a.ledger.get(TEAM, &key).unwrap().is_none());
}

#[test]
fn a_legacy_reserved_live_cancel_fact_is_retired_without_reusing_its_request() {
    let (mut h, mut a, mut p) = setup();
    p.answered_cancelled = true;
    let key = format!("cancel:{}", p.pending[0].decision);
    let id = uuid::Uuid::new_v4().to_string();
    let revision = h.store.team(TEAM).unwrap().unwrap().revision;
    a.put(
        TEAM,
        &key,
        &json!({"pending":p.pending[0],"reply":"Released",
        "attempt":{"number":0,"id":""},"fact":{"request_id":id,"revision":revision,
        "pending":p.pending[0],"refused":false,"reason":"","attempt":0,"attempt_id":""},
        "failures":0,"next_ms":0,"reason":""}),
    )
    .unwrap();
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    assert!(p.calls.is_empty());
    assert!(
        p.facts.is_empty(),
        "a legacy fact is not rewritten under its old or a new ID"
    );
    assert_eq!(p.reads, 1);
    assert!(a.ledger.get(TEAM, &key).unwrap().is_none());
}

#[test]
fn live_cancel_restarts_before_and_after_the_observation_never_record_an_end_fact() {
    for point in ["reserved", "effect", "before_observe", "observed"] {
        let (mut h, mut a, mut p) = setup();
        super::super::cancel::CRASH.with(|s| s.set(Some(point)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.cancels(
                &mut h.store,
                &mut p,
                &h_team(),
                100
            )))
            .is_err(),
            "{point}"
        );
        let machine = a.machine.clone();
        drop(a);
        let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
        a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
        assert!(p.pending.is_empty(), "{point}");
        assert_eq!(
            p.calls.len(),
            if matches!(point, "effect" | "before_observe") {
                2
            } else {
                1
            },
            "{point}"
        );
        assert_eq!(p.reads, 1, "{point}");
        assert!(p.facts.is_empty(), "{point}");
    }
}
#[test]
fn a_failed_live_cancel_observation_backs_off_and_repeats_only_the_idempotent_command() {
    let (mut h, mut a, mut p) = setup();
    p.fail_observe = true;
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    assert!(p.facts.is_empty());
    p.fail_observe = false;
    a.cancels(&mut h.store, &mut p, &h_team(), 60_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert!(p.pending.is_empty());
    assert!(p.facts.is_empty());
}

#[test]
fn a_cached_release_without_a_tuple_is_observed_without_recording_a_default_pair() {
    let (mut h, mut a, mut p) = setup();
    p.answered_cancelled = true;
    let key = format!("cancel:{}", p.pending[0].decision);
    a.put(
        TEAM,
        &key,
        &json!({"pending":p.pending[0],"reply":"Released","fact":null,
        "failures":0,"next_ms":0,"reason":""}),
    )
    .unwrap();
    for now in [100, 101, 60_100] {
        a.cancels(&mut h.store, &mut p, &h_team(), now).unwrap();
    }
    assert_eq!(p.reads, 1);
    assert!(p.calls.is_empty());
    assert!(p.facts.is_empty());
    assert!(a.ledger.get(TEAM, &key).unwrap().is_none());
}
#[test]
fn an_invalid_terminal_stale_reply_backs_off_without_observing_or_recording_a_refusal() {
    let (mut h, mut a, mut p) = setup();
    h.run(&person(), crate::teams::ops::Operation::Stop)
        .unwrap();
    p.reply = Some(TaskReply::Stale);
    a.terminal_cancels(&mut h.store, &mut p, 100).unwrap();
    a.terminal_cancels(&mut h.store, &mut p, 101).unwrap();
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.reads, 0);
    assert!(p.facts.is_empty());
    p.reply = Some(TaskReply::Refused {
        reason: "attempt_changed: the person resumed it".into(),
    });
    a.terminal_cancels(&mut h.store, &mut p, 60_100).unwrap();
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.reads, 0);
    assert!(p.facts[0].refused);
    assert_eq!(p.facts[0].reason, "attempt_changed: the person resumed it");
}

#[test]
fn one_decision_starts_at_most_three_cancel_commands_across_a_hundred_passes_and_restarts() {
    for changed in [false, true] {
        let (mut h, mut a, mut p) = setup();
        p.reply = Some(if changed {
            TaskReply::Released
        } else {
            TaskReply::CuttingOver {
                reason: "No outcome yet".into(),
            }
        });
        p.always_changed = changed;
        for pass in 0..100 {
            let machine = a.machine.clone();
            if pass > 0 {
                drop(a);
                a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
            }
            a.tick(&mut h.store, &mut p, TEAM, 100 + pass * 900_000, false)
                .unwrap();
        }
        assert_eq!(p.calls.len(), 3, "changed={changed}");
        assert!(p.pending.is_empty());
        let exits: Vec<_> = p.facts.iter().filter(|f| f.refused).collect();
        assert_eq!(exits.len(), 1);
        assert!(exits[0].reason.contains(if changed {
            "execution changed"
        } else {
            "Three cancel command attempts"
        }));
    }
}

#[test]
fn a_cancel_pass_spends_at_most_four_effects_and_resumes_the_rest() {
    let (mut h, mut a, mut p) = setup();
    let template = p.pending[0].clone();
    p.pending = (1..=9)
        .map(|node| Pending {
            node,
            decision: 100 + u64::from(node),
            task: Some(task(node)),
            ..template.clone()
        })
        .collect();
    p.reply = Some(TaskReply::Released);
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    assert_eq!(p.calls, vec![1, 2, 3, 4]);
    assert_eq!(p.pending.len(), 5);
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert_eq!(p.calls.len(), 8);
    a.cancels(&mut h.store, &mut p, &h_team(), 102).unwrap();
    assert_eq!(p.calls, (1..=9).collect::<Vec<_>>());
    assert!(p.pending.is_empty());
}

#[test]
fn the_decision_budget_survives_successful_visits_that_leave_the_decision_pending() {
    for observed in [false, true] {
        let (mut h, mut a, mut p) = setup();
        p.reply = Some(if observed {
            TaskReply::Cancelled
        } else {
            TaskReply::Released
        });
        p.keep_observed = observed;
        p.keep_recorded = !observed;
        for pass in 0..100 {
            let machine = a.machine.clone();
            if pass > 0 {
                drop(a);
                a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
            }
            a.tick(&mut h.store, &mut p, TEAM, 100 + pass * 900_000, false)
                .unwrap();
        }
        assert_eq!(p.calls.len(), 3, "observed={observed}");
        assert!(p.pending.is_empty());
        assert_eq!(p.facts.iter().filter(|f| f.refused).count(), 1);
    }
}

#[test]
fn a_changed_snapshot_before_the_exit_keeps_the_same_decisions_child_budget() {
    let (mut h, mut a, mut p) = setup();
    p.reply = Some(TaskReply::CuttingOver {
        reason: "No answer".into(),
    });
    for pass in 0..3 {
        a.tick(&mut h.store, &mut p, TEAM, 100 + pass * 900_000, false)
            .unwrap();
    }
    assert_eq!(p.calls.len(), 3);
    super::super::cancel::CRASH.with(|s| s.set(Some("refusal_exit")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.tick(
            &mut h.store,
            &mut p,
            TEAM,
            2_700_100,
            false
        )))
        .is_err()
    );
    p.pending[0].attempt += 1;
    p.pending[0].attempt_id = "new-observation".into();
    let machine = a.machine.clone();
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    for pass in 4..100 {
        a.tick(&mut h.store, &mut p, TEAM, 100 + pass * 900_000, false)
            .unwrap();
    }
    assert_eq!(p.calls.len(), 3);
    assert!(p.pending.is_empty());
    assert_eq!(p.facts.iter().filter(|f| f.refused).count(), 1);
}
