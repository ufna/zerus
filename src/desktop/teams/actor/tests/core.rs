use super::*;
use crate::teams::ops::{self, HandoutOutcome, Operation, TaskStage};
use std::collections::BTreeSet;
struct Fake {
    reads: usize,
    journals: usize,
    sends: usize,
    free: BTreeSet<String>,
    reply: HandoutOutcome,
    driven: bool,
    busy_on_recheck: bool,
    free_reads: usize,
    documents: BTreeSet<String>,
    failed_task: Option<String>,
    lock_after_send: Option<std::path::PathBuf>,
    held: Option<rusqlite::Connection>,
    pause_on_recheck: Option<std::path::PathBuf>,
}
impl Fake {
    fn new() -> Self {
        Self {
            reads: 0,
            journals: 0,
            sends: 0,
            free: ["m1".into(), "m2".into()].into(),
            reply: HandoutOutcome::Submitted {
                receipt: "submitted".into(),
            },
            driven: false,
            busy_on_recheck: false,
            free_reads: 0,
            documents: BTreeSet::new(),
            failed_task: None,
            lock_after_send: None,
            held: None,
            pause_on_recheck: None,
        }
    }
}
impl super::super::ports::Ports for Fake {
    fn changes(&mut self, _: &str, g: &str, _: u64) -> Result<zerus_replica::Changes> {
        self.journals += 1;
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: if g != "journal" {
                None
            } else {
                Some(std::mem::take(&mut self.documents))
            },
        })
    }
    fn observe(&mut self, store: &mut Store, task: &TaskRef) -> Result<()> {
        self.reads += 1;
        if self.failed_task.as_deref() == Some(&task.uuid) {
            return Err(super::super::problem("Fixture observation unavailable"));
        }
        if self.driven && self.reads > 1 {
            for node in store.queue(TEAM, 200, None)?.rows {
                if node.task.as_ref() == Some(task) {
                    let old = store.task_observation(TEAM, node.id)?.unwrap();
                    let o = ops::TaskObservation {
                        revision: format!("external-{}", self.reads),
                        previous_revision: old.revision.clone(),
                        stage: TaskStage::InProgress,
                        ..old
                    };
                    super::super::write(
                        store,
                        TEAM,
                        Operation::ObserveTask {
                            node: node.id,
                            observation: o,
                        },
                        100,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn free(&mut self, _: &Team, m: &Member) -> Result<bool> {
        self.free_reads += 1;
        if self.free_reads > 2 {
            if let Some(state) = self.pause_on_recheck.take() {
                let mut store = Store::open(&state)?;
                let revision = store.team(TEAM)?.unwrap().revision;
                ops::apply(
                    &mut store,
                    &person(),
                    &request(
                        revision,
                        Operation::PauseMember {
                            member: m.id.clone(),
                        },
                    ),
                    100,
                )?;
            }
        }
        Ok(self.free.contains(&m.id) && !(self.busy_on_recheck && self.free_reads > 2))
    }
    fn deliver(&mut self, _: &Team, _: &Node, _: &Member, _: &str) -> HandoutOutcome {
        self.sends += 1;
        if let Some(path) = self.lock_after_send.take() {
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            self.held = Some(db);
        }
        self.reply.clone()
    }
}
fn setup(n: u32) -> (Harness, super::super::Runtime, Fake) {
    let mut h = Harness::new();
    h.bind("m1");
    h.bind("m2");
    for i in 1..=n {
        h.add(&person(), i);
    }
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    (h, a, Fake::new())
}
fn tick(h: &mut Harness, a: &mut super::super::Runtime, p: &mut Fake, now: i64, safety: bool) {
    a.begin_pass();
    a.tick(&mut h.store, p, TEAM, now, safety).unwrap();
}
#[test]
fn two_free_members_receive_only_two_of_three_ready_nodes() {
    let (mut h, mut a, mut p) = setup(3);
    tick(&mut h, &mut a, &mut p, 100, false);
    assert_eq!(p.sends, 2);
    assert_eq!(h.store.open_effects(TEAM, 200, None).unwrap().rows.len(), 2);
}
#[test]
fn a_part_a_retry_row_reopens_without_the_new_failure_event_field() {
    let (mut h, mut a, _) = setup(0);
    a.put(
        TEAM,
        "gate:report:1",
        &json!({"revision":"old","failures":2,"at":60_100,"card":null,"notice":null}),
    )
    .unwrap();
    drop(a);
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut a = super::super::Runtime::open(h.temp.path(), &t.home).unwrap();
    assert!(!a.due(&mut h.store, &t, "report:1", "old", 100).unwrap());
    a.failure(
        &mut h.store,
        &t,
        "report:1",
        "old",
        &super::super::problem("Still unavailable"),
        60_100,
    )
    .unwrap();
    assert_eq!(
        a.ledger.get(TEAM, "gate:report:1").unwrap().unwrap().data["failures"],
        3
    );
    assert_eq!(h.store.cards(TEAM, 200, None).unwrap().rows.len(), 1);
}
#[test]
fn unknown_delivery_creates_one_inbox_and_never_sends_again_after_restart() {
    let (mut h, mut a, mut p) = setup(1);
    p.reply = HandoutOutcome::Uncertain {
        reason: "Lost writer receipt".into(),
    };
    tick(&mut h, &mut a, &mut p, 100, false);
    drop(a);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    for i in 0..3 {
        tick(&mut h, &mut a, &mut p, 200 + i, false);
    }
    assert_eq!(p.sends, 1);
    assert_eq!(
        h.store
            .inbox(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .filter(|r| r.kind == "delivery_uncertain")
            .count(),
        1
    );
}
#[test]
fn refusal_leaves_no_assignee_and_obeys_retry_gate() {
    let (mut h, mut a, mut p) = setup(1);
    p.reply = HandoutOutcome::Refused {
        reason: "Busy elsewhere".into(),
    };
    tick(&mut h, &mut a, &mut p, 100, false);
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.sends, 1);
    assert!(h.node(1).assignee.is_none());
    tick(&mut h, &mut a, &mut p, 60_101, false);
    assert_eq!(p.sends, 2);
}
#[test]
fn task_driven_elsewhere_and_member_busy_races_do_not_send() {
    for driven in [true, false] {
        let (mut h, mut a, mut p) = setup(1);
        p.driven = driven;
        p.busy_on_recheck = !driven;
        tick(&mut h, &mut a, &mut p, 100, false);
        assert_eq!(p.sends, 0);
    }
}
#[test]
fn second_unchanged_woken_pass_reads_no_task_document_or_effect() {
    let (mut h, mut a, mut p) = setup(1);
    p.free.clear();
    tick(&mut h, &mut a, &mut p, 100, false);
    let reads = p.reads;
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.reads, reads);
    assert_eq!(p.sends, 0);
    tick(&mut h, &mut a, &mut p, 60_101, true);
    assert_eq!(p.reads, reads + 1);
}
#[test]
fn restart_at_each_delivery_boundary_never_continues_an_original() {
    for point in ["reserved", "intent", "effect", "outcome", "recorded"] {
        let (mut h, mut a, mut p) = setup(1);
        super::super::delivery::CRASH.with(|c| c.set(Some(point)));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tick(&mut h, &mut a, &mut p, 100, false)
        }));
        assert!(result.is_err(), "boundary {point} was not exercised");
        let expected = usize::from(matches!(point, "effect" | "outcome" | "recorded"));
        assert_eq!(p.sends, expected);
        drop(a);
        let home = h.store.team(TEAM).unwrap().unwrap().home;
        let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
        for i in 0..3 {
            tick(&mut h, &mut a, &mut p, 200 + i, false);
        }
        assert_eq!(p.sends, expected, "restart continued {point}");
        let inbox = h.store.inbox(TEAM, 200, None).unwrap();
        assert_eq!(
            inbox
                .rows
                .iter()
                .filter(|r| r.kind == "delivery_uncertain")
                .count(),
            usize::from(matches!(point, "reserved" | "intent" | "effect"))
        );
    }
}
#[test]
fn unknown_fleet_or_external_task_occupancy_never_qualifies_a_member() {
    let (mut h, mut a, mut p) = setup(2);
    p.free.clear();
    tick(&mut h, &mut a, &mut p, 100, false);
    assert_eq!(p.sends, 0);
    assert!(h
        .store
        .open_effects(TEAM, 200, None)
        .unwrap()
        .rows
        .is_empty());
}
#[test]
fn new_journal_generation_observes_all_live_nodes_once() {
    let (mut h, mut a, mut p) = setup(3);
    p.free.clear();
    tick(&mut h, &mut a, &mut p, 100, false);
    let before = p.reads;
    let old = super::super::observe::position_fixture("other-generation");
    a.put(TEAM, "journal", &old).unwrap();
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.reads, before + 3);
    tick(&mut h, &mut a, &mut p, 102, false);
    assert_eq!(p.reads, before + 3);
}
#[test]
fn a_return_reserves_and_delivers_a_new_generation_once() {
    let (mut h, mut a, mut p) = setup(1);
    tick(&mut h, &mut a, &mut p, 100, false);
    h.observe(1, TaskStage::InReview);
    h.run(
        &service(),
        Operation::RecordReport {
            node: 1,
            generation: 1,
            member: "m1".into(),
            text: "Report".into(),
            commit: COMMIT.into(),
        },
    )
    .unwrap();
    let lead = h.bind("lead");
    h.run(
        &lead,
        Operation::Return {
            node: 1,
            reason: "Revise".into(),
        },
    )
    .unwrap();
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.sends, 2);
    assert_eq!(h.store.generation(TEAM, 1).unwrap(), Some(2));
    tick(&mut h, &mut a, &mut p, 102, false);
    assert_eq!(p.sends, 2);
}
#[test]
fn journal_hint_observes_only_the_changed_live_task() {
    let (mut h, mut a, mut p) = setup(3);
    p.free.clear();
    tick(&mut h, &mut a, &mut p, 100, false);
    let before = p.reads;
    p.documents.insert(h.node(2).task.unwrap().uuid);
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.reads, before + 1);
    tick(&mut h, &mut a, &mut p, 102, false);
    assert_eq!(p.reads, before + 1);
}
#[test]
fn failed_observation_continues_other_rows_and_obeys_backoff() {
    let (mut h, mut a, mut p) = setup(3);
    p.free.clear();
    p.failed_task = Some(h.node(1).task.unwrap().uuid);
    tick(&mut h, &mut a, &mut p, 100, false);
    assert_eq!(p.reads, 3);
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(
        p.reads, 5,
        "failed row must stay backed off while other rows progress"
    );
}
#[test]
fn one_journal_read_serves_two_teams_in_the_same_project() {
    let (mut h, mut a, mut p) = setup(1);
    p.free.clear();
    let mut op = create();
    if let Operation::Create {
        integration_branch, ..
    } = &mut op
    {
        *integration_branch = "teams/other".into();
    }
    h.run_for(TEAM_B, &person(), op).unwrap();
    let teams = h.store.live_teams(16, None).unwrap().rows;
    a.prepare(&teams).unwrap();
    for t in teams {
        a.tick(&mut h.store, &mut p, &t.id, 100, false).unwrap();
    }
    assert_eq!(p.journals, 1);
}
#[test]
fn retry_decision_runs_the_row_early_and_its_next_failure_opens_one_new_card() {
    let (mut h, mut a, mut p) = setup(1);
    p.free.clear();
    p.failed_task = Some(h.node(1).task.unwrap().uuid);
    for now in [100, 60_100, 180_100] {
        tick(&mut h, &mut a, &mut p, now, false);
    }
    let cards = h.store.cards(TEAM, 200, None).unwrap().rows;
    assert_eq!(cards.len(), 1);
    let first = cards[0].seq;
    h.run(
        &person(),
        Operation::DecideCard {
            card: first,
            decision: "retry".into(),
        },
    )
    .unwrap();
    tick(&mut h, &mut a, &mut p, 180_101, false);
    let cards = h.store.cards(TEAM, 200, None).unwrap().rows;
    assert_eq!(cards.len(), 2);
    assert_eq!(cards.iter().filter(|c| c.state == "open").count(), 1);
    p.failed_task = None;
    tick(&mut h, &mut a, &mut p, 660_102, false);
    assert!(h
        .store
        .cards(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .all(|c| c.state != "open"));
}
#[test]
fn an_uncertain_reopen_is_not_sent_twice_and_stays_in_its_new_generation() {
    let (mut h, mut a, mut p) = setup(1);
    tick(&mut h, &mut a, &mut p, 100, false);
    h.observe(1, TaskStage::InReview);
    h.run(
        &service(),
        Operation::RecordReport {
            node: 1,
            generation: 1,
            member: "m1".into(),
            text: "Report".into(),
            commit: COMMIT.into(),
        },
    )
    .unwrap();
    let lead = h.bind("lead");
    h.run(
        &lead,
        Operation::Return {
            node: 1,
            reason: "Revise".into(),
        },
    )
    .unwrap();
    p.reply = HandoutOutcome::Uncertain {
        reason: "Lost receipt".into(),
    };
    tick(&mut h, &mut a, &mut p, 101, false);
    drop(a);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    for now in [102, 60_101, 600_101] {
        tick(&mut h, &mut a, &mut p, now, false);
    }
    assert_eq!(p.sends, 2);
    assert_eq!(h.store.generation(TEAM, 1).unwrap(), Some(2));
    assert_eq!(
        h.store
            .inbox(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .filter(|r| r.kind == "delivery_uncertain")
            .count(),
        1
    );
}
#[test]
fn failed_durable_outcome_save_never_repeats_the_original_delivery() {
    let (mut h, mut a, mut p) = setup(1);
    p.lock_after_send = Some(h.temp.path().join("teams/actor-v1.sqlite3"));
    a.begin_pass();
    assert!(a.tick(&mut h.store, &mut p, TEAM, 100, false).is_err());
    assert_eq!(p.sends, 1);
    drop(p.held.take());
    drop(a);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    tick(&mut h, &mut a, &mut p, 101, false);
    tick(&mut h, &mut a, &mut p, 102, false);
    assert_eq!(p.sends, 1);
    assert_eq!(
        h.store
            .inbox(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .filter(|r| r.kind == "delivery_uncertain")
            .count(),
        1
    );
}
#[test]
fn saved_intent_refusal_removes_its_unsent_reservation_and_allows_a_later_choice() {
    let (mut h, mut a, mut p) = setup(1);
    p.pause_on_recheck = Some(h.temp.path().to_owned());
    tick(&mut h, &mut a, &mut p, 100, false);
    assert_eq!(p.sends, 0);
    assert!(
        a.ledger
            .page(TEAM, "")
            .unwrap()
            .iter()
            .all(|r| !r.key.starts_with("delivery:")),
        "the saved refusal must remove the unsent original"
    );
    h.run(
        &person(),
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.sends, 0);
    tick(&mut h, &mut a, &mut p, 60_101, false);
    assert_eq!(p.sends, 1);
}
#[test]
fn a_fault_of_a_known_row_does_not_become_a_worker_health_fault() {
    let (mut h, mut a, mut p) = setup(1);
    p.free.clear();
    a.put(
        TEAM,
        "gate:observe:1",
        &serde_json::json!({"unreadable":"fixture"}),
    )
    .unwrap();
    let teams = h.store.live_teams(16, None).unwrap().rows;
    assert!(a.page(&mut h.store, &mut p, &teams, 100, false).is_ok());
    assert_eq!(p.sends, 0);
}
#[test]
fn cooling_refused_tasks_do_not_claim_members_from_other_ready_tasks() {
    let (mut h, mut a, mut p) = setup(2);
    p.reply = HandoutOutcome::Refused {
        reason: "Task writer refused".into(),
    };
    tick(&mut h, &mut a, &mut p, 100, false);
    assert_eq!(p.sends, 2);
    let node = h.add(&person(), 3);
    p.reply = HandoutOutcome::Submitted {
        receipt: "new task".into(),
    };
    tick(&mut h, &mut a, &mut p, 101, false);
    assert_eq!(p.sends, 3);
    assert!(h.node(node).implementer.is_some());
}
#[test]
fn a_full_actor_ledger_is_a_visible_sweep_fault_before_any_new_delivery() {
    let (mut h, mut a, mut p) = setup(1);
    p.free.clear();
    let teams = h.store.live_teams(16, None).unwrap().rows;
    a.page(&mut h.store, &mut p, &teams, 100, false).unwrap();
    let db = rusqlite::Connection::open(h.temp.path().join("teams/actor-v1.sqlite3")).unwrap();
    db.execute("UPDATE actor_budget SET rows=8192 WHERE id=1", [])
        .unwrap();
    p.free.insert("m1".into());
    let error = a
        .page(&mut h.store, &mut p, &teams, 101, false)
        .unwrap_err();
    assert_eq!(error.code, super::super::ledger::CAPACITY);
    assert_eq!(p.sends, 0);
}
