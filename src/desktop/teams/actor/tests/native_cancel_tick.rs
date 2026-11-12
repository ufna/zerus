//! Real store pages and facts, with only the original child's result replaced.
use super::super::{
    cancel::{Answer, Fact, Pending},
    native::Native,
    ports::{DeliveryResult, Ports},
    Runtime,
};
use super::*;
use crate::teams::ops::{KeyClearState, Operation, TaskStage};

struct Adapter {
    native: Native,
    cancelled: Vec<u32>,
    cleared: Vec<u32>,
}
impl Ports for Adapter {
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
    fn deliver(&mut self, _: &Team, _: &Node, _: &Member, _: &str) -> DeliveryResult {
        panic!("No hand-out")
    }
    fn cancels(&mut self, store: &Store, team: &str, after: u32) -> Result<Vec<Pending>> {
        Native::pending_cancels(store, team, after)
    }
    fn cancel(&mut self, team: &Team, pending: &Pending, original: &str) -> Result<Answer> {
        self.cancelled.push(pending.node);
        crate::desktop::teams::child::FIXTURE_EXIT.with(|s| *s.borrow_mut() = Some(
            json!({"schema":1,"team":team.id,"node":pending.node,"state":"released","error":"","attempt":pending.attempt,"attempt_id":pending.attempt_id}).to_string().into_bytes()));
        self.native.run_cancel(team, pending, original)
    }
    fn record_cancel(
        &mut self,
        store: &mut Store,
        team: &Team,
        fact: &Fact,
        now: i64,
    ) -> Result<()> {
        Native::save_cancel(store, team, fact, now)
    }
    fn terminal_timers(
        &mut self,
        _: &mut super::super::auxiliary::State,
        store: &mut Store,
        now: i64,
    ) -> Result<()> {
        let pending: Vec<u32> = store
            .pending_cancels(TEAM, 200, 0)?
            .into_iter()
            .map(|p| p.node)
            .collect();
        for keyed in store.keyed_tasks(TEAM, 0, 16)? {
            assert!(
                !pending.contains(&keyed.node),
                "key clear preceded its cancel"
            );
            super::super::write(
                store,
                TEAM,
                Operation::RecordKeyClear {
                    node: keyed.node,
                    state: KeyClearState::Cleared,
                    reason: String::new(),
                },
                now,
            )?;
            self.cleared.push(keyed.node);
        }
        Ok(())
    }
}
fn stopped_stages() -> Harness {
    let mut h = Harness::new();
    for (i, stage) in [
        TaskStage::Backlog,
        TaskStage::Todo,
        TaskStage::InProgress,
        TaskStage::InReview,
        TaskStage::Done,
    ]
    .into_iter()
    .enumerate()
    {
        let node = h.add(&person(), i as u32 + 1);
        h.observe(node, stage);
    }
    h.run(&person(), Operation::Stop).unwrap();
    h
}
#[test]
fn a_stop_cancels_each_unfinished_stage_before_any_key_is_cleared() {
    let mut h = stopped_stages();
    let expected: Vec<u32> = h
        .store
        .pending_cancels(TEAM, 200, 0)
        .unwrap()
        .into_iter()
        .filter(|p| p.task.is_some())
        .map(|p| p.node)
        .collect();
    assert!(!expected.is_empty());
    assert!(h
        .store
        .keyed_tasks(TEAM, 0, 200)
        .unwrap()
        .iter()
        .all(|k| !expected.contains(&k.node)));
    let native = super::production::native(&h);
    let mut runtime = Runtime::open(h.temp.path(), &native.machine).unwrap();
    let mut p = Adapter {
        native,
        cancelled: Vec::new(),
        cleared: Vec::new(),
    };
    for pass in 0..3 {
        runtime
            .page(&mut h.store, &mut p, &[], 100 + pass * 60_000, false)
            .unwrap();
    }
    assert_eq!(p.cancelled, expected);
    assert!(h.store.pending_cancels(TEAM, 200, 0).unwrap().is_empty());
    assert!(expected.iter().all(|n| p.cleared.contains(n)));
    let calls = p.cancelled.clone();
    runtime
        .page(&mut h.store, &mut p, &[], 240_100, false)
        .unwrap();
    assert_eq!(p.cancelled, calls);
}

#[test]
fn a_stage_without_a_task_records_cancelled_without_invoking_the_native_adapter() {
    let (mut h, _, stage) = super::tasks::wanted();
    assert!(h.node(stage).task.is_none());
    h.run(
        &person(),
        Operation::Cancel {
            node: stage,
            reason: "No stage work remains".into(),
        },
    )
    .unwrap();
    let native = super::production::native(&h);
    let mut runtime = Runtime::open(h.temp.path(), &native.machine).unwrap();
    let mut p = Adapter {
        native,
        cancelled: Vec::new(),
        cleared: Vec::new(),
    };
    let team = h.store.team(TEAM).unwrap().unwrap();
    runtime.cancels(&mut h.store, &mut p, &team, 100).unwrap();
    assert_eq!(h.node(stage).state, NodeState::Cancelled);
    assert!(p.cancelled.is_empty());
    assert!(h.store.pending_cancels(TEAM, 16, 0).unwrap().is_empty());
}
