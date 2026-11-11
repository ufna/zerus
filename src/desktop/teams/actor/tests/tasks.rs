use super::super::ports::{Ports, TaskReply};
use super::*;
use std::collections::BTreeSet;

pub(super) fn wanted() -> (Harness, u32, u32) {
    let mut h = Harness::empty();
    h.run(&person(), create()).unwrap();
    h.bind("m1");
    h.bind("m3");
    let target = h.add(&person(), 1);
    h.submit(target, "m1");
    h.report(target, "m1");
    let stage = h.store.stage_tasks(TEAM, 1, None).unwrap().rows[0].node;
    (h, target, stage)
}

#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    documents: BTreeSet<String>,
    reads: usize,
    lose_first: bool,
    clears: Vec<u32>,
}
impl Ports for Fake {
    fn changes(&mut self, _: &str, g: &str, _: u64) -> Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: if g.is_empty() {
                None
            } else {
                Some(Default::default())
            },
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        self.reads += 1;
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
    ) -> crate::teams::ops::HandoutOutcome {
        panic!("no native deliveries in this fixture")
    }
    fn stage(
        &mut self,
        _: &Team,
        e: &crate::teams::store::StageTaskEffect,
        _: &str,
    ) -> Result<TaskReply> {
        self.calls.push(e.request_id.clone());
        self.documents.insert(e.request_id.clone());
        if std::mem::take(&mut self.lose_first) {
            return Err(super::super::problem("child ended after saving its task"));
        }
        Ok(TaskReply::Created)
    }
    fn clear_stage(
        &mut self,
        _: &Team,
        keyed: &crate::teams::store::timers::KeyedTask,
        _: &str,
    ) -> Result<TaskReply> {
        self.clears.push(keyed.node);
        Ok(TaskReply::Cleared)
    }
}
fn tick(h: &mut Harness, a: &mut super::super::Runtime, p: &mut Fake, now: i64) {
    a.begin_pass();
    a.tick(&mut h.store, p, TEAM, now, false).unwrap();
}

#[test]
fn the_stage_slot_creates_one_task_and_an_unchanged_pass_reads_no_document() {
    let (mut h, _, stage) = wanted();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let mut p = Fake::default();
    tick(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(h.node(stage).task.as_ref().unwrap().uuid, p.calls[0]);
    let reads = p.reads;
    tick(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.reads, reads);
}

#[test]
fn a_child_lost_after_save_repeats_only_the_recorded_creation_identity() {
    let (mut h, _, stage) = wanted();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let mut p = Fake {
        lose_first: true,
        ..Default::default()
    };
    tick(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.calls.len(), 1);
    assert!(h.node(stage).task.is_none());
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    tick(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.calls.len(), 1, "backoff was lost at restart");
    tick(&mut h, &mut a, &mut p, 60_100);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.calls[0], p.calls[1]);
    assert_eq!(p.documents.len(), 1);
    assert_eq!(h.node(stage).task.as_ref().unwrap().uuid, p.calls[0]);
}

#[test]
fn stage_restarts_before_and_after_each_effect_keep_the_same_reserved_task() {
    for point in [
        "before_intent",
        "intent",
        "reserved",
        "effect",
        "outcome",
        "recorded",
    ] {
        let (mut h, _, stage) = wanted();
        let machine = h.store.team(TEAM).unwrap().unwrap().home;
        let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
        let mut p = Fake::default();
        super::super::tasks::CRASH.with(|s| s.set(Some(point)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(
                &mut h, &mut a, &mut p, 100
            )))
            .is_err(),
            "{point}"
        );
        let reserved = h
            .store
            .stage_tasks(TEAM, 16, None)
            .unwrap()
            .rows
            .into_iter()
            .find(|s| s.node == stage)
            .and_then(|s| s.request_id);
        drop(a);
        let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
        for now in [101, 60_101] {
            tick(&mut h, &mut a, &mut p, now);
        }
        let task = h.node(stage).task.unwrap();
        assert_eq!(p.documents.len(), 1, "{point}");
        if let Some(id) = reserved {
            assert_eq!(task.uuid, id, "{point}");
        }
        if matches!(point, "outcome" | "recorded") {
            assert_eq!(p.calls.len(), 1, "{point}");
        }
    }
}

#[test]
fn a_released_stage_is_cleared_once_and_the_store_removes_it_from_the_outbox() {
    let (mut h, target, stage) = wanted();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let mut p = Fake::default();
    tick(&mut h, &mut a, &mut p, 100);
    h.run(
        &person(),
        crate::teams::ops::Operation::Return {
            node: target,
            reason: "Please revise".into(),
        },
    )
    .unwrap();
    assert_eq!(h.store.released_stages(TEAM, 16, 0).unwrap()[0].node, stage);
    tick(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.clears, vec![stage]);
    assert!(h.store.released_stages(TEAM, 16, 0).unwrap().is_empty());
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    tick(&mut h, &mut a, &mut p, 102);
    assert_eq!(p.clears, vec![stage]);
}
