use super::super::{
    cancel::{Fact, Pending},
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
    fn cancel(&mut self, _: &Team, p: &Pending, _: &str) -> Result<TaskReply> {
        self.calls.push(p.node);
        Ok(self.reply.clone().unwrap_or(TaskReply::Cancelled))
    }
    fn record_cancel(&mut self, _: &mut Store, _: &Team, f: &Fact, _: i64) -> Result<()> {
        self.facts.push(f.clone());
        self.pending.retain(|p| p.decision != f.pending.decision);
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
