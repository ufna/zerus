use super::super::ports::{Finish, MergePlan, MergeResult, Ports};
use super::*;
use crate::teams::{
    integrate,
    ops::{HandoutOutcome, Operation, TaskStage},
};
#[derive(Default)]
struct Work {
    text: Option<String>,
    dirty: bool,
    reports: u32,
    heads: u32,
    merges: u32,
    finishes: u32,
    contains: Option<bool>,
    merge_reply: Option<MergeResult>,
    finish_reply: Option<Finish>,
    phase: Vec<&'static str>,
    ids: Vec<String>,
    lock_after_merge: Option<std::path::PathBuf>,
    held: Option<rusqlite::Connection>,
    advance: Option<std::path::PathBuf>,
    repo: Option<integrate::Repo>,
    stop_team: Option<std::path::PathBuf>,
    stop: bool,
    last_plan: Option<MergePlan>,
}
impl Ports for Work {
    fn stopped(&self) -> bool {
        self.stop
    }
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
        self.phase.push("observe");
        Ok(())
    }
    fn free(&mut self, _: &Team, _: &Member) -> Result<bool> {
        Ok(false)
    }
    fn deliver(&mut self, _: &Team, _: &Node, _: &Member, _: &str) -> HandoutOutcome {
        panic!("work fixture must not deliver")
    }
    fn report(&mut self, _: &TaskRef) -> Result<Option<String>> {
        self.phase.push("report");
        self.reports += 1;
        Ok(self.text.clone())
    }
    fn reported(&mut self, _: &Team, _: &Member) -> Result<integrate::Reported> {
        self.heads += 1;
        Ok(integrate::Reported {
            commit: COMMIT.into(),
            branch: "teams/member".into(),
            dirty: self.dirty,
        })
    }
    fn repository(&mut self, _: &Team) -> Result<String> {
        Ok(self
            .repo
            .as_ref()
            .map(|r| r.git_dir.to_string_lossy().into_owned())
            .unwrap_or("fixture-repo".into()))
    }
    fn descends(&mut self, plan: &MergePlan) -> Result<bool> {
        self.phase.push("descends");
        if let Some(repo) = &self.repo {
            return integrate::descends(repo, &plan.commit, &plan.base);
        }
        Ok(true)
    }
    fn contains(&mut self, _: &MergePlan) -> Result<bool> {
        self.phase.push("contains");
        self.contains
            .ok_or_else(|| super::super::problem("Fixture ancestry unreadable"))
    }
    fn merge(&mut self, plan: &MergePlan) -> MergeResult {
        self.phase.push("merge");
        self.merges += 1;
        self.ids.push(plan.id.clone());
        self.last_plan = Some(plan.clone());
        if let Some(path) = self.stop_team.take() {
            let mut store = Store::open(&path).unwrap();
            apply(&mut store, &person(), Operation::Stop);
        }
        if let Some(path) = self.advance.take() {
            advance_attempt(&path);
        }
        if let Some(path) = self.lock_after_merge.take() {
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            self.held = Some(db);
        }
        if let Some(repo) = &self.repo {
            return match integrate::merge(repo, &plan.branch, &plan.commit, "Teams fixture accept")
                .unwrap()
            {
                integrate::Merge::Merged { commit } => MergeResult::Merged { commit },
                integrate::Merge::Conflict { files } => MergeResult::Conflict { files },
                integrate::Merge::Stale => panic!("fixture ref moved"),
            };
        }
        self.merge_reply.clone().unwrap_or(MergeResult::Merged {
            commit: COMMIT.into(),
        })
    }
    fn finish(&mut self, _: &Team, _: &Node, _: &str) -> Result<Finish> {
        self.phase.push("finish");
        self.finishes += 1;
        Ok(self.finish_reply.clone().unwrap_or(Finish::Done))
    }
}
fn run(h: &mut Harness, a: &mut super::super::Runtime, p: &mut Work, now: i64) {
    a.begin_pass();
    a.tick(&mut h.store, p, TEAM, now, false).unwrap();
}
fn actor(h: &Harness) -> super::super::Runtime {
    super::super::Runtime::open(h.temp.path(), &h.store.team(TEAM).unwrap().unwrap().home).unwrap()
}
#[test]
fn report_waits_for_text_and_records_the_owned_head_with_a_dirty_note_once() {
    let mut h = Harness::new();
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    h.observe(node, TaskStage::InReview);
    let mut a = actor(&h);
    let mut p = Work::default();
    run(&mut h, &mut a, &mut p, 100);
    assert!(h.store.report(TEAM, node, 1).unwrap().is_none());
    assert_eq!(p.heads, 0);
    run(&mut h, &mut a, &mut p, 101);
    assert_eq!(
        p.reports, 1,
        "unchanged wake must not reread the task report"
    );
    p.text = Some("Holder report".into());
    p.dirty = true;
    h.observe(node, TaskStage::InReview);
    run(&mut h, &mut a, &mut p, 102);
    let report = h.store.report(TEAM, node, 1).unwrap().unwrap();
    assert_eq!(report.member, "m1");
    assert_eq!(report.commit, COMMIT);
    assert!(report.text.contains("Holder report") && report.text.contains("uncommitted"));
    drop(a);
    let mut a = actor(&h);
    run(&mut h, &mut a, &mut p, 103);
    assert_eq!(p.heads, 1);
}
#[test]
fn integration_records_the_exact_decision_before_finishing() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let mut a = actor(&h);
    let mut p = Work::default();
    run(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.merges, 1);
    assert_eq!(p.finishes, 1);
    let f = h.store.integration_fact(TEAM, node).unwrap().unwrap();
    assert_eq!(f.decision, decision);
    assert_eq!(f.commit.as_deref(), Some(COMMIT));
    assert!(h.node(node).integrated);
    let merge = p.phase.iter().position(|s| *s == "merge").unwrap();
    let done = p.phase.iter().position(|s| *s == "finish").unwrap();
    assert!(merge < done);
}
#[test]
fn unknown_merge_is_only_read_and_makes_one_card_until_ancestry_settles_it() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Unknown {
            reason: "Lost original write result".into(),
        }),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    drop(a);
    let mut a = actor(&h);
    for now in [101, 60_101, 180_101] {
        run(&mut h, &mut a, &mut p, now);
    }
    assert_eq!(p.merges, 1);
    assert_eq!(p.finishes, 0);
    assert_eq!(
        h.store
            .cards(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .filter(|c| c.state == "open")
            .count(),
        1
    );
    p.contains = Some(true);
    run(&mut h, &mut a, &mut p, 900_101);
    assert_eq!(p.merges, 1);
    assert_eq!(
        h.store
            .integration_fact(TEAM, node)
            .unwrap()
            .unwrap()
            .decision,
        decision
    );
    assert_eq!(p.finishes, 1);
}
#[test]
fn an_integration_refusal_before_start_retries_with_backoff() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Unavailable {
            reason: "Git busy before write".into(),
        }),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    run(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.merges, 1);
    p.merge_reply = None;
    run(&mut h, &mut a, &mut p, 60_101);
    assert_eq!(p.merges, 2);
    assert!(h.node(node).integrated);
}
#[test]
fn no_merge_accepts_finish_and_a_refusal_waits_for_observation_or_decision_change() {
    let mut h = Harness::new();
    h.settings(|s| s.integration = Integration::None);
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        finish_reply: Some(Finish::Refused("Task key differs".into())),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    for now in [101, 60_101, 900_101] {
        run(&mut h, &mut a, &mut p, now);
    }
    assert_eq!(p.merges, 0);
    assert_eq!(p.finishes, 1);
    h.observe(node, TaskStage::InReview);
    p.finish_reply = None;
    run(&mut h, &mut a, &mut p, 900_102);
    assert_eq!(p.finishes, 2);
    h.observe(node, TaskStage::Done);
    run(&mut h, &mut a, &mut p, 960_103);
    assert_eq!(p.finishes, 2);
}
#[test]
fn finishing_cutover_retries_with_backoff_and_unchanged_wakes_do_not_repeat_done() {
    let mut h = Harness::new();
    h.settings(|s| s.integration = Integration::None);
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        finish_reply: Some(Finish::CuttingOver("Cutover".into())),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    run(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.finishes, 1);
    p.finish_reply = None;
    run(&mut h, &mut a, &mut p, 60_101);
    run(&mut h, &mut a, &mut p, 60_102);
    assert_eq!(p.finishes, 2);
}
fn apply(store: &mut Store, who: &crate::teams::ops::Actor, op: Operation) {
    let revision = store.team(TEAM).unwrap().unwrap().revision;
    crate::teams::ops::apply(store, who, &request(revision, op), 1234).unwrap();
}
fn observation(store: &mut Store, stage: TaskStage) {
    let old = store.task_observation(TEAM, 1).unwrap().unwrap();
    apply(
        store,
        &service(),
        Operation::ObserveTask {
            node: 1,
            observation: crate::teams::ops::TaskObservation {
                revision: uuid::Uuid::new_v4().to_string(),
                previous_revision: old.revision.clone(),
                stage,
                ..old
            },
        },
    );
}
fn advance_attempt(path: &std::path::Path) {
    let mut store = Store::open(path).unwrap();
    observation(&mut store, TaskStage::Done);
    observation(&mut store, TaskStage::Todo);
    let id = uuid::Uuid::new_v4().to_string();
    apply(
        &mut store,
        &service(),
        Operation::HandoutIntent {
            node: 1,
            member: "m1".into(),
            effect_request_id: id.clone(),
        },
    );
    apply(
        &mut store,
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: id,
            outcome: HandoutOutcome::Submitted {
                receipt: "new attempt".into(),
            },
        },
    );
    observation(&mut store, TaskStage::InReview);
    apply(
        &mut store,
        &service(),
        Operation::RecordReport {
            node: 1,
            generation: 2,
            member: "m1".into(),
            text: "new report".into(),
            commit: "1123456789abcdef0123456789abcdef01234567".into(),
        },
    );
    apply(&mut store, &person(), Operation::Accept { node: 1 });
}
#[test]
fn integration_fact_never_uses_a_newer_pending_decision() {
    for outcome in [
        MergeResult::Merged {
            commit: COMMIT.into(),
        },
        MergeResult::Conflict {
            files: vec!["conflict.txt".into()],
        },
    ] {
        let mut h = Harness::new();
        let (node, lead) = h.reported();
        h.run(&lead, Operation::Accept { node }).unwrap();
        let old = h.accept_decision(node);
        let mut a = actor(&h);
        let mut p = Work {
            advance: Some(h.temp.path().to_owned()),
            merge_reply: Some(outcome),
            ..Default::default()
        };
        run(&mut h, &mut a, &mut p, 100);
        let new = h.accept_decision(node);
        assert_ne!(old, new);
        assert_eq!(
            h.store
                .integration_fact(TEAM, node)
                .unwrap()
                .unwrap()
                .decision,
            old
        );
        assert!(!h.node(node).integrated);
        assert!(h
            .store
            .pending_accepts(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .any(|d| d.seq == new));
        assert_eq!(p.finishes, 0);
    }
}
#[test]
fn restart_at_every_merge_boundary_never_repeats_a_write() {
    for point in ["reserved", "effect", "outcome", "recorded"] {
        let mut h = Harness::new();
        let (node, lead) = h.reported();
        h.run(&lead, Operation::Accept { node }).unwrap();
        let mut a = actor(&h);
        let mut p = Work::default();
        super::super::integration::CRASH.with(|c| c.set(Some(point)));
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&mut h, &mut a, &mut p, 100)
        }));
        assert!(crashed.is_err(), "{point}");
        let expected = u32::from(point != "reserved");
        assert_eq!(p.merges, expected);
        drop(a);
        let mut a = actor(&h);
        for now in [101, 60_101, 180_101] {
            run(&mut h, &mut a, &mut p, now);
        }
        assert_eq!(p.merges, expected, "restart continued {point}");
        if matches!(point, "outcome" | "recorded") {
            assert!(h.node(node).integrated);
        } else {
            assert!(!h.node(node).integrated);
            assert_eq!(
                h.store
                    .cards(TEAM, 200, None)
                    .unwrap()
                    .rows
                    .iter()
                    .filter(|c| c.state == "open")
                    .count(),
                1
            );
        }
    }
}
#[test]
fn losing_a_merge_result_to_a_ledger_lock_never_repeats_the_write() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        lock_after_merge: Some(h.temp.path().join("teams/actor-v1.sqlite3")),
        ..Default::default()
    };
    a.begin_pass();
    assert!(a.tick(&mut h.store, &mut p, TEAM, 100, false).is_err());
    assert_eq!(p.merges, 1);
    drop(p.held.take());
    drop(a);
    let mut a = actor(&h);
    for now in [101, 60_101, 180_101] {
        run(&mut h, &mut a, &mut p, now);
    }
    assert_eq!(p.merges, 1);
    assert!(!h.node(node).integrated);
}
#[test]
fn a_lost_fact_write_is_retried_as_a_fact_with_no_second_merge() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        lock_after_merge: Some(h.temp.path().join("teams/teams-v1.sqlite3")),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.merges, 1);
    drop(p.held.take());
    drop(a);
    let mut a = actor(&h);
    run(&mut h, &mut a, &mut p, 60_101);
    assert_eq!(p.merges, 1);
    assert!(h.node(node).integrated);
}
mod git;
#[test]
fn report_restart_before_and_after_the_fact_keeps_one_report() {
    for point in ["read", "recorded"] {
        let mut h = Harness::new();
        h.bind("m1");
        let node = h.add(&person(), 1);
        h.submit(node, "m1");
        h.observe(node, TaskStage::InReview);
        let mut a = actor(&h);
        let mut p = Work {
            text: Some("Holder report".into()),
            ..Default::default()
        };
        super::super::reports::CRASH.with(|c| c.set(Some(point)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(
                &mut h, &mut a, &mut p, 100
            )))
            .is_err()
        );
        drop(a);
        let mut a = actor(&h);
        run(&mut h, &mut a, &mut p, 101);
        assert!(h.store.report(TEAM, node, 1).unwrap().is_some());
        assert_eq!(
            h.store
                .inbox(TEAM, 200, None)
                .unwrap()
                .rows
                .iter()
                .filter(|r| r.kind == "report")
                .count(),
            1
        );
        assert_eq!(p.heads, if point == "read" { 2 } else { 1 });
    }
}
#[test]
fn finishing_may_repeat_after_restart_until_observation_settles_it() {
    for point in ["before", "after"] {
        let mut h = Harness::new();
        h.settings(|s| s.integration = Integration::None);
        let (node, lead) = h.reported();
        h.run(&lead, Operation::Accept { node }).unwrap();
        let mut a = actor(&h);
        let mut p = Work::default();
        super::super::finish::CRASH.with(|c| c.set(Some(point)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(
                &mut h, &mut a, &mut p, 100
            )))
            .is_err()
        );
        drop(a);
        let mut a = actor(&h);
        run(&mut h, &mut a, &mut p, 101);
        assert_eq!(p.finishes, if point == "before" { 1 } else { 2 });
        h.observe(node, TaskStage::Done);
        run(&mut h, &mut a, &mut p, 102);
        assert_eq!(p.finishes, if point == "before" { 1 } else { 2 });
    }
}
#[test]
fn a_negative_ancestry_read_never_authorizes_another_merge() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        contains: Some(false),
        merge_reply: Some(MergeResult::Unknown {
            reason: "Lost receipt".into(),
        }),
        ..Default::default()
    };
    for now in [100, 60_101, 180_101, 900_101] {
        run(&mut h, &mut a, &mut p, now);
    }
    assert_eq!(p.merges, 1);
    assert!(h.store.integration_fact(TEAM, node).unwrap().is_none());
    assert_eq!(h.store.cards(TEAM, 200, None).unwrap().rows.len(), 1);
}
#[test]
fn paused_teams_take_in_reports_but_start_no_merge_or_finish() {
    let mut h = Harness::new();
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    h.observe(node, TaskStage::InReview);
    h.run(&person(), Operation::Pause).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        text: Some("Report while paused".into()),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    assert!(h.store.report(TEAM, node, 1).unwrap().is_some());
    assert_eq!(p.merges, 0);
    assert_eq!(p.finishes, 0);
}
#[test]
fn a_standing_team_prunes_finish_cooldowns_after_observation() {
    let mut h = Harness::new();
    h.settings(|s| s.integration = Integration::None);
    let lead = h.bind("lead");
    h.bind("m1");
    let mut a = actor(&h);
    let mut p = Work::default();
    for n in 1..=20 {
        let node = h.add(&person(), n);
        h.submit(node, "m1");
        h.report(node, "m1");
        h.run(&lead, Operation::Accept { node }).unwrap();
        run(&mut h, &mut a, &mut p, i64::from(n) * 60_000);
        h.observe(node, TaskStage::Done);
        run(&mut h, &mut a, &mut p, i64::from(n) * 60_000 + 1);
    }
    assert!(a
        .ledger
        .prefix_page(TEAM, "gate:finish", "")
        .unwrap()
        .is_empty());
    assert_eq!(p.finishes, 20);
}
#[test]
fn a_known_outcome_waiting_for_its_fact_does_not_stop_other_rows() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let other = h.add(&person(), 2);
    h.submit(other, "m1");
    h.report(other, "m1");
    h.run(&lead, Operation::Accept { node: other }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        lock_after_merge: Some(h.temp.path().join("teams/teams-v1.sqlite3")),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.merges, 2);
    drop(p.held.take());
    run(&mut h, &mut a, &mut p, 60_101);
    assert_eq!(p.merges, 2);
    assert!(h.node(node).integrated && h.node(other).integrated);
}
#[test]
fn service_stop_starts_no_new_work_and_an_inflight_known_merge_still_records_its_fact() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let mut a = actor(&h);
    let mut p = Work {
        stop: true,
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    assert!(p.phase.is_empty());
    p.stop = false;
    p.stop_team = Some(h.temp.path().to_owned());
    run(&mut h, &mut a, &mut p, 101);
    assert_eq!(p.merges, 1);
    assert_eq!(p.finishes, 0);
    assert_eq!(
        h.store
            .integration_fact(TEAM, node)
            .unwrap()
            .unwrap()
            .decision,
        decision
    );
    assert_eq!(
        h.store.team(TEAM).unwrap().unwrap().status,
        TeamStatus::Stopped
    );
}
#[test]
fn merge_recovery_pages_past_backed_off_rows() {
    let h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut a = actor(&h);
    let mut p = Work::default();
    for i in 0..250 {
        let key = format!("merge:{i:08}");
        let node = i + 1000;
        let plan = MergePlan {
            team: TEAM.into(),
            owner: t.owner.clone(),
            machine: t.home.clone(),
            project: t.project.clone(),
            branch: t.integration_branch.clone(),
            base: t.base_commit.clone(),
            repo: "fixture-repo".into(),
            node,
            generation: 1,
            decision: u64::from(node),
            member: "m1".into(),
            commit: COMMIT.into(),
            id: uuid::Uuid::new_v4().to_string(),
        };
        let outcome = if i < 249 {
            serde_json::json!({"Merged":{"commit":COMMIT}})
        } else {
            serde_json::json!({"Unavailable":{"reason":"Known no write"}})
        };
        a.put(
            TEAM,
            &key,
            &serde_json::json!({"plan":plan,"outcome":outcome,"fact_id":null,"fact_revision":null}),
        )
        .unwrap();
        if i < 249 {
            a.put(TEAM,&format!("gate:integration:{node}"),&serde_json::json!({"revision":format!("{node}:1"),"failures":1,"at":1_000_000,"card":null,"notice":null})).unwrap();
        }
    }
    let mut store = h.store;
    a.recover_merges(&mut store, &mut p, &t, 100).unwrap();
    a.recover_merges(&mut store, &mut p, &t, 101).unwrap();
    assert!(a.ledger.get(TEAM, "merge:00000249").unwrap().is_none());
    assert_eq!(p.merges, 0);
}
#[test]
fn retry_after_a_saved_unavailable_result_checks_again_instead_of_replaying_the_failure() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Unavailable {
            reason: "Not started".into(),
        }),
        ..Default::default()
    };
    for now in [100, 60_100, 180_100] {
        run(&mut h, &mut a, &mut p, now);
    }
    assert_eq!(p.merges, 3);
    let card = h.store.cards(TEAM, 200, None).unwrap().rows[0].seq;
    a.put(TEAM,&format!("merge:{decision}"),&serde_json::json!({"plan":p.last_plan,"outcome":{"Unavailable":{"reason":"Not started"}},"fact_id":null,"fact_revision":null})).unwrap();
    h.run(
        &person(),
        Operation::DecideCard {
            card,
            decision: "retry".into(),
        },
    )
    .unwrap();
    p.merge_reply = None;
    run(&mut h, &mut a, &mut p, 180_101);
    assert_eq!(p.merges, 4);
    assert!(h.node(node).integrated);
    assert!(h
        .store
        .cards(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .all(|c| c.state != "open"));
}
#[test]
fn a_saved_unavailable_outcome_counts_as_one_failure_after_a_restart() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Unavailable {
            reason: "Not started".into(),
        }),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    // The failure was saved, but a crash lost removal of its cached outcome.
    a.put(TEAM,&format!("merge:{decision}"),&serde_json::json!({"plan":p.last_plan,"outcome":{"Unavailable":{"reason":"Not started"}},"fact_id":null,"fact_revision":null})).unwrap();
    drop(a);
    let mut a = actor(&h);
    run(&mut h, &mut a, &mut p, 101);
    let gate = a
        .ledger
        .get(TEAM, &format!("gate:integration:{node}"))
        .unwrap()
        .unwrap();
    assert_eq!(gate.data["failures"], 1);
    assert_eq!(gate.data["at"], 60_100);
    assert_eq!(p.merges, 1);
}
