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
    fn record_cancel(&mut self, _: &mut Store, _: &Team, f: &Fact, _: i64) -> Result<()> {
        self.facts.push(f.clone());
        if self.keep_once {
            self.keep_once = false;
            self.pending[0].attempt += 1;
            self.pending[0].attempt_id = "new-person-attempt".into();
        } else {
            self.pending.retain(|p| p.decision != f.pending.decision);
        }
        Ok(())
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
fn the_tick_consumes_cancels_and_maps_released_to_a_cancelled_fact() {
    for reply in [TaskReply::Cancelled, TaskReply::Released] {
        let (mut h, mut a, mut p) = setup();
        p.reply = Some(reply);
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
        assert_eq!(p.calls, vec![1]);
        assert_eq!(p.facts.len(), 1);
        assert!(!p.facts[0].refused);
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
fn cancel_restarts_before_and_after_each_effect_keep_the_known_answer() {
    for point in ["reserved", "effect", "outcome", "recorded"] {
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
    p.reply = None;
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
fn a_legacy_saved_cancel_without_a_pair_is_observed_and_deferred_after_restart() {
    let (mut h, mut a, mut p) = setup();
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
    a.cancels(&mut h.store, &mut p, &h_team(), 100).unwrap();
    assert_eq!(p.reads, 1);
    assert!(p.calls.is_empty());
    assert!(p.facts.is_empty());
    a.cancels(&mut h.store, &mut p, &h_team(), 101).unwrap();
    assert!(p.calls.is_empty());
    a.cancels(&mut h.store, &mut p, &h_team(), 60_100).unwrap();
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.facts.len(), 1);
}
