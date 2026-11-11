use super::{
    launch::*,
    ledger::{Ledger, State, BRANCH},
    ports::*,
};
use crate::teams::ops::{self, tests::Harness};
use std::collections::BTreeMap;
use zerus_team_plan::problem::code;
use zerus_team_plan::{
    accounts::{AccountUsage, UsageWindow},
    catalog::AccountChoice,
    model::MemberState,
};
const MACHINE: &str = "00000000-0000-4000-8000-0000000000aa";
const EPOCH: &str = "00000000-0000-4000-8000-000000000001";

struct FakeEffects {
    events: Vec<String>,
    plans: Vec<Plan>,
    sessions: BTreeMap<String, ObservedSession>,
    accounts: Vec<AccountUsage>,
    exists: bool,
    unknown: bool,
    not_started: bool,
    mismatch: bool,
    account_reads: usize,
    placements: usize,
    account_error: bool,
    placement_error: bool,
    read_error: bool,
    parent_pending: bool,
    worktree_unverified: bool,
    launch_verification_unavailable: bool,
}
impl Default for FakeEffects {
    fn default() -> Self {
        Self {
            events: vec![],
            plans: vec![],
            sessions: BTreeMap::new(),
            accounts: vec![account("a"), account("b")],
            exists: false,
            unknown: false,
            not_started: false,
            mismatch: false,
            account_reads: 0,
            placements: 0,
            account_error: false,
            placement_error: false,
            read_error: false,
            parent_pending: false,
            worktree_unverified: false,
            launch_verification_unavailable: false,
        }
    }
}
fn account(id: &str) -> AccountUsage {
    AccountUsage {
        id: id.into(),
        agent: "claude".into(),
        signed_in: true,
        windows: vec![UsageWindow {
            used_percent: 30.,
            resets_at: Some(1800000000),
        }],
    }
}
impl Effects for FakeEffects {
    fn placement(&mut self, _: &str, launch: &str) -> Result<Placement> {
        self.placements += 1;
        if self.placement_error {
            return Err(invalid("The project has two local checkouts"));
        }
        Ok(Placement {
            source: "/workspace/example".into(),
            git_dir: "/workspace/example/.git".into(),
            branch: format!("team-{launch}"),
            destination: format!("/workspace/zerus-team-{launch}"),
        })
    }
    fn branch_exists(&mut self, _: &super::ledger::BranchPlan) -> Result<bool> {
        Ok(self.exists)
    }
    fn create_branch(&mut self, _: &super::ledger::BranchPlan) -> Effect<()> {
        self.events.push("branch".into());
        self.exists = true;
        Effect::Submitted(())
    }
    fn accounts(&mut self, _: &str) -> Result<Vec<AccountUsage>> {
        self.account_reads += 1;
        if self.account_error {
            return Err(invalid("Account inspection failed"));
        }
        Ok(self.accounts.clone())
    }
    fn worktree(&mut self, plan: &Plan, _: &str) -> Effect<Option<Worktree>> {
        self.events.push(format!("worktree:{}", plan.member));
        if self.worktree_unverified {
            return Effect::Submitted(None);
        }
        Effect::Submitted(Some(Worktree {
            path: format!("/workspace/example-{}", plan.member),
            ownership_nonce: format!("owned-{}", plan.member),
        }))
    }
    fn verify_worktree(&mut self, plan: &Plan) -> Result<Worktree> {
        if self.read_error {
            return Err(invalid("Ownership read unavailable"));
        }
        Ok(Worktree {
            path: plan.placement.destination.clone(),
            ownership_nonce: format!("owned-{}", plan.member),
        })
    }
    fn launch(&mut self, plan: &Plan, tree: &Worktree, _: &str) -> Effect<()> {
        if self.launch_verification_unavailable {
            return Effect::Deferred(invalid("Ownership read unavailable"));
        }
        if std::mem::take(&mut self.not_started) {
            return Effect::NotStarted;
        }
        self.events.push(format!("launch:{}", plan.member));
        self.plans.push(plan.clone());
        if self.unknown {
            return Effect::Unknown(uncertain("Synthetic lost launch result"));
        }
        let s = ObservedSession {
            machine: plan.machine.clone(),
            launch_id: plan.launch_id.clone(),
            agent: plan.agent.clone(),
            account: plan.account.clone(),
            worktree: tree.clone(),
            parent: None,
            name: format!("claude/example/{}", plan.member),
            session_id: uuid::Uuid::new_v4().to_string(),
            run_id: uuid::Uuid::new_v4().to_string(),
            conversation_id: uuid::Uuid::new_v4().to_string(),
        };
        self.sessions.insert(plan.launch_id.clone(), s);
        Effect::Submitted(())
    }
    fn observe(&mut self, plan: &Plan) -> Result<LaunchRead> {
        if self.read_error {
            return Err(invalid("Launch observation failed"));
        }
        let mut s = self.sessions.get(&plan.launch_id).cloned();
        if self.mismatch {
            if let Some(s) = &mut s {
                s.launch_id = uuid::Uuid::new_v4().to_string();
            }
        }
        Ok(s.map(|s| LaunchRead::Session(Box::new(s)))
            .unwrap_or(LaunchRead::Absent { legacy: false }))
    }
    fn parent(&mut self, plan: &Plan, _: &ObservedSession, _: &str) -> String {
        self.events.push(format!("parent:{}", plan.member));
        if self.parent_pending {
            "identity_pending"
        } else {
            "manual"
        }
        .into()
    }
}
fn runtime(h: &Harness) -> Runtime {
    Runtime::open(h.temp.path(), MACHINE, EPOCH).unwrap()
}
fn tick(r: &mut Runtime, h: &mut Harness, e: &mut FakeEffects) -> bool {
    r.tick(&mut h.store, e, ops::tests::TEAM, 1234).unwrap()
}
fn advance(r: &mut Runtime, h: &mut Harness, e: &mut FakeEffects, count: usize) {
    for _ in 0..count {
        tick(r, h, e);
    }
}
// No fixture may spin on a broken or mutated transition on the shared computer.
fn until(
    r: &mut Runtime,
    h: &mut Harness,
    e: &mut FakeEffects,
    reached: impl Fn(&Harness, &FakeEffects) -> bool,
) {
    for _ in 0..100 {
        if reached(h, e) {
            return;
        }
        tick(r, h, e);
    }
    assert!(
        reached(h, e),
        "expected fixture transition was not reached in 100 steps"
    );
}
fn cards(h: &Harness) -> Vec<crate::teams::store::Card> {
    h.store.cards(ops::tests::TEAM, 200, None).unwrap().rows
}
fn row(h: &Harness, member: &str) -> super::ledger::Row {
    Ledger::open(h.temp.path())
        .unwrap()
        .get(ops::tests::TEAM, member)
        .unwrap()
        .unwrap()
}

#[test]
fn branch_then_lead_then_members() {
    let mut h = Harness::new();
    let mut e = FakeEffects::default();
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 60);
    assert_eq!(
        e.events,
        [
            "branch",
            "worktree:lead",
            "launch:lead",
            "worktree:m1",
            "launch:m1",
            "parent:m1",
            "worktree:m2",
            "launch:m2",
            "parent:m2",
            "worktree:m3",
            "launch:m3",
            "parent:m3"
        ]
    );
    let lead = h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .unwrap();
    assert!(e.plans[0].parent.is_none());
    assert!(e.plans[1..]
        .iter()
        .all(|p| p.parent.as_ref() == Some(&lead)));
    assert_eq!(e.plans[0].account, "a");
    assert_eq!(e.plans[1].account, "b");
    assert!(cards(&h).is_empty());
}
#[test]
fn unknown_launch_opens_exactly_one_card_and_never_relaunches_after_restart() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        unknown: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    for _ in 0..5 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 301_234)
            .unwrap();
    }
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 20);
    assert_eq!(e.events, ["branch", "worktree:lead", "launch:lead"]);
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].kind, "launch_uncertain");
}
#[test]
fn restart_at_every_durable_transition_never_continues_an_original() {
    for stop_after in 1..32 {
        let mut h = Harness::new();
        let mut e = FakeEffects::default();
        let mut r = runtime(&h);
        advance(&mut r, &mut h, &mut e, stop_after);
        let before = e.plans.len();
        let interrupted: [Option<String>; 4] = ["lead", "m1", "m2", "m3"].map(|id| {
            row_if(&h, id)
                .filter(|row| matches!(row.state, State::WorktreeIntent | State::LaunchIntent))
                .map(|_| id.to_string())
        });
        let events_before = e.events.clone();
        drop(r);
        let mut replacement = runtime(&h);
        advance(&mut replacement, &mut h, &mut e, 60);
        let mut ids = std::collections::BTreeSet::new();
        assert!(
            e.plans.iter().all(|p| ids.insert(p.member.clone())),
            "duplicate at {stop_after}"
        );
        assert!(e.plans.len() >= before);
        for id in interrupted.into_iter().flatten() {
            for kind in ["worktree", "launch"] {
                let event = format!("{kind}:{id}");
                assert_eq!(
                    e.events.iter().filter(|v| **v == event).count(),
                    events_before.iter().filter(|v| **v == event).count(),
                    "restart continued an original at transition {stop_after}"
                );
            }
        }
        for id in ["lead", "m1", "m2", "m3"] {
            assert!(
                e.events
                    .iter()
                    .filter(|s| **s == format!("worktree:{id}"))
                    .count()
                    <= 1,
                "duplicate worktree at {stop_after}"
            );
        }
    }
}
#[test]
fn restart_between_intent_and_effect_does_not_create_even_the_first_child() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    tick(&mut r, &mut h, &mut e);
    assert_eq!(row(&h, "lead").state, State::WorktreeIntent);
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 10);
    assert!(e.events.is_empty());
    assert_eq!(cards(&h).len(), 1);
}
#[test]
fn launch_id_binds_after_rename_and_the_same_name_with_another_id_is_refused() {
    for mismatch in [false, true] {
        let mut h = Harness::new();
        let mut e = FakeEffects {
            exists: true,
            mismatch,
            ..Default::default()
        };
        let mut r = runtime(&h);
        until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
        for s in e.sessions.values_mut() {
            s.name = "claude/example/renamed".into();
        }
        tick(&mut r, &mut h, &mut e);
        let bound = h
            .store
            .member(ops::tests::TEAM, "lead")
            .unwrap()
            .unwrap()
            .session;
        assert_eq!(bound.is_some(), !mismatch);
        if let Some(bound) = bound {
            assert_eq!(
                bound.session_id,
                format!("launch_id:{}", e.plans[0].launch_id)
            );
        }
    }
}
#[test]
fn identity_also_checks_machine_account_agent_and_owned_worktree() {
    for field in ["machine", "account", "agent", "path", "nonce"] {
        let mut h = Harness::new();
        let mut e = FakeEffects {
            exists: true,
            ..Default::default()
        };
        let mut r = runtime(&h);
        until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
        let s = e.sessions.values_mut().next().unwrap();
        match field {
            "machine" => s.machine = "other".into(),
            "account" => s.account = "other".into(),
            "agent" => s.agent = "codex".into(),
            "path" => s.worktree.path = "/workspace/other".into(),
            _ => s.worktree.ownership_nonce = "other".into(),
        }
        advance(&mut r, &mut h, &mut e, 5);
        assert!(h
            .store
            .member(ops::tests::TEAM, "lead")
            .unwrap()
            .unwrap()
            .session
            .is_none());
        assert_eq!(cards(&h).len(), 1);
    }
}
#[test]
fn unavailable_accounts_name_the_reset_time_without_creating_a_worktree() {
    let mut h = Harness::new();
    let mut a = account("a");
    a.windows[0].used_percent = 100.;
    let mut e = FakeEffects {
        accounts: vec![a],
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    advance(&mut runtime(&h), &mut h, &mut e, 10);
    assert!(e.events.is_empty());
    assert_eq!(cards(&h).len(), 1);
    assert!(cards(&h)[0].text.contains("1800000000"));
}
#[test]
fn named_account_is_resolved_from_the_member_role() {
    let mut h = Harness::empty();
    let mut create = ops::tests::create();
    if let ops::Operation::Create { bundle, .. } = &mut create {
        bundle.template.lead.account = AccountChoice::Named {
            account: "b".into(),
        };
    }
    h.run(&ops::tests::person(), create).unwrap();
    let mut e = FakeEffects::default();
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 15);
    assert_eq!(e.plans[0].account, "b");
}
#[test]
fn not_started_is_retryable_but_does_not_spin_inside_one_pass() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        not_started: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "lead").is_some_and(|r| r.state == State::LaunchIntent)
    });
    assert!(!tick(&mut r, &mut h, &mut e));
    assert_eq!(row(&h, "lead").state, State::LaunchReady);
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 50);
    assert_eq!(
        e.events
            .iter()
            .filter(|s| s.as_str() == "launch:lead")
            .count(),
        1
    );
    assert!(cards(&h).is_empty());
}
fn row_if(h: &Harness, member: &str) -> Option<super::ledger::Row> {
    Ledger::open(h.temp.path())
        .unwrap()
        .get(ops::tests::TEAM, member)
        .unwrap()
}
#[test]
fn preview_off_and_absent_team_create_no_launch_store() {
    let temp = tempfile::tempdir().unwrap();
    let mut e = FakeEffects::default();
    let mut r = Runtime::open(temp.path(), MACHINE, EPOCH).unwrap();
    let mut h = Harness::empty();
    r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234)
        .unwrap();
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let mut r = runtime(&h);
    tick(&mut r, &mut h, &mut e);
    assert!(!h
        .temp
        .path()
        .join("teams/member-launch-v1.sqlite3")
        .exists());
}
#[test]
fn paused_stopped_and_foreign_teams_have_no_effects() {
    for op in [ops::Operation::Pause, ops::Operation::Stop] {
        let mut h = Harness::new();
        h.run(&ops::tests::person(), op).unwrap();
        let mut e = FakeEffects::default();
        let mut r = runtime(&h);
        advance(&mut r, &mut h, &mut e, 5);
        assert!(e.events.is_empty());
        assert!(!h
            .temp
            .path()
            .join("teams/member-launch-v1.sqlite3")
            .exists());
    }
    let mut h = Harness::new();
    let mut e = FakeEffects::default();
    let mut r = Runtime::open(h.temp.path(), "another-machine", EPOCH).unwrap();
    advance(&mut r, &mut h, &mut e, 5);
    assert!(e.events.is_empty());
}
#[test]
fn concurrent_runtime_cannot_acquire_an_already_reserved_launch() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut one = runtime(&h);
    let mut two = runtime(&h);
    tick(&mut one, &mut h, &mut e);
    advance(&mut two, &mut h, &mut e, 5);
    advance(&mut one, &mut h, &mut e, 5);
    assert!(e.events.is_empty());
    assert_eq!(cards(&h).len(), 1);
}
#[test]
fn branch_observation_is_not_a_new_branch_effect() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 50);
    assert!(!e.events.contains(&"branch".into()));
    assert_eq!(row(&h, BRANCH).state, State::BranchDone);
}
#[test]
fn owner_card_replay_after_store_commit_is_idempotent() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        mismatch: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 15);
    assert_eq!(cards(&h).len(), 1);
    let mut ledger = Ledger::open(h.temp.path()).unwrap();
    let mut saved = row(&h, "lead");
    saved.card_done = false;
    ledger.save(&mut saved, 1234).unwrap();
    advance(&mut runtime(&h), &mut h, &mut e, 10);
    assert_eq!(cards(&h).len(), 1);
}
#[test]
fn newer_launch_ledger_schema_is_refused() {
    let h = Harness::new();
    drop(Ledger::open(h.temp.path()).unwrap());
    let db =
        rusqlite::Connection::open(h.temp.path().join("teams/member-launch-v1.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=99").unwrap();
    drop(db);
    assert!(Ledger::open(h.temp.path()).is_err());
}

pub(super) fn plan_fixture() -> Plan {
    let mut h = Harness::new();
    let mut effects = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    tick(&mut r, &mut h, &mut effects);
    row(&h, "lead").plan.unwrap()
}
#[test]
fn presentation_parent_never_delays_binding_or_opens_an_owner_card() {
    let mut h = Harness::new();
    let mut e = FakeEffects::default();
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "m1").is_some_and(|r| r.state == State::ParentIntent)
    });
    assert_eq!(
        h.store
            .member(ops::tests::TEAM, "m1")
            .unwrap()
            .unwrap()
            .state,
        MemberState::Active
    );
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 60);
    assert_eq!(
        row(&h, "m1").parent_result.as_deref(),
        Some("unknown_after_restart")
    );
    assert!(!e.events.contains(&"parent:m1".into()));
    assert!(cards(&h).is_empty());
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn final_native_admission_rechecks_pause_and_exact_member_binding() {
    let mut h = Harness::new();
    let plan = plan_fixture();
    let config = crate::config::Config {
        home: String::new(),
        dir: h.temp.path().join("config"),
        state: h.temp.path().into(),
        selves: vec![],
        peers: vec![],
        base_peers: vec![],
        peers_from_environment: false,
        machines: Default::default(),
        tab: false,
        tab_colors: String::new(),
    };
    assert!(super::child::admission(&config, &plan, false).is_ok());
    h.run(&ops::tests::person(), ops::Operation::Pause).unwrap();
    assert!(super::child::admission(&config, &plan, false).is_err());
    h.run(&ops::tests::person(), ops::Operation::Resume)
        .unwrap();
    h.bind("lead");
    assert!(super::child::admission(&config, &plan, false).is_err());
    assert!(super::child::admission(&config, &plan, true).is_err());
}

#[test]
fn pausing_an_earlier_member_does_not_withdraw_another_members_original() {
    let mut h = Harness::new();
    let mut e = FakeEffects::default();
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "m1").is_some_and(|r| r.state == State::ParentIntent)
    });
    h.run(
        &ops::tests::person(),
        ops::Operation::PauseMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    advance(&mut r, &mut h, &mut e, 30);
    assert_eq!(
        h.store
            .member(ops::tests::TEAM, "m2")
            .unwrap()
            .unwrap()
            .state,
        MemberState::Active
    );
    assert!(cards(&h).is_empty());
}

#[test]
fn another_team_can_progress_while_a_member_is_unfinished() {
    let mut h = Harness::new();
    let mut create = ops::tests::create();
    if let ops::Operation::Create {
        integration_branch, ..
    } = &mut create
    {
        integration_branch.push_str("-second")
    }
    h.run_for(ops::tests::TEAM_B, &ops::tests::person(), create)
        .unwrap();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    tick(&mut r, &mut h, &mut e);
    assert!(r.busy());
    assert!(r
        .tick(&mut h.store, &mut e, ops::tests::TEAM_B, 1234)
        .unwrap());
}

// Regression controls supplied by the independent package review.
#[test]
fn review07_removed_member_does_not_starve_other_teams() {
    let mut h = Harness::new();
    let mut create = ops::tests::create();
    if let ops::Operation::Create {
        integration_branch, ..
    } = &mut create
    {
        integration_branch.push_str("-second");
    }
    h.run_for(ops::tests::TEAM_B, &ops::tests::person(), create)
        .unwrap();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "m3").is_some_and(|r| r.state == State::WorktreeIntent)
    });
    assert!(r.busy());
    h.run(
        &ops::tests::person(),
        ops::Operation::ArchiveMember {
            member: "m3".into(),
        },
    )
    .unwrap();
    advance(&mut r, &mut h, &mut e, 20);
    let before = e.events.len();
    let mut progressed = false;
    for _ in 0..50 {
        progressed |= r
            .tick(&mut h.store, &mut e, ops::tests::TEAM_B, 1234)
            .unwrap();
    }
    eprintln!(
        "busy={} progressed={} events_before={} after={}",
        r.busy(),
        progressed,
        before,
        e.events.len()
    );
    assert!(!r.busy(), "removed member releases its admission");
    assert!(e.events.len() > before, "team B progresses");
    assert!(cards(&h).is_empty());
}

struct LockingEffects {
    inner: FakeEffects,
    path: std::path::PathBuf,
    lock: Option<rusqlite::Connection>,
    lock_on: &'static str,
    fired: bool,
}
impl LockingEffects {
    fn after(&mut self, kind: &str) {
        if self.lock_on == kind && !self.fired {
            let db = rusqlite::Connection::open(&self.path).unwrap();
            db.execute_batch("BEGIN EXCLUSIVE").unwrap();
            self.lock = Some(db);
            self.fired = true;
        }
    }
}
impl Effects for LockingEffects {
    fn placement(&mut self, p: &str, l: &str) -> Result<Placement> {
        self.inner.placement(p, l)
    }
    fn branch_exists(&mut self, p: &super::ledger::BranchPlan) -> Result<bool> {
        self.inner.branch_exists(p)
    }
    fn create_branch(&mut self, p: &super::ledger::BranchPlan) -> Effect<()> {
        let result = self.inner.create_branch(p);
        self.after("branch");
        result
    }
    fn accounts(&mut self, a: &str) -> Result<Vec<AccountUsage>> {
        self.inner.accounts(a)
    }
    fn worktree(&mut self, p: &Plan, o: &str) -> Effect<Option<Worktree>> {
        let result = self.inner.worktree(p, o);
        self.after("worktree");
        result
    }
    fn verify_worktree(&mut self, plan: &Plan) -> Result<Worktree> {
        self.inner.verify_worktree(plan)
    }
    fn launch(&mut self, p: &Plan, t: &Worktree, o: &str) -> Effect<()> {
        let result = self.inner.launch(p, t, o);
        self.after("launch");
        result
    }
    fn observe(&mut self, p: &Plan) -> Result<LaunchRead> {
        self.inner.observe(p)
    }
    fn parent(&mut self, p: &Plan, s: &ObservedSession, o: &str) -> String {
        let result = self.inner.parent(p, s, o);
        self.after("parent");
        result
    }
}
#[test]
fn review07_lost_outcome_never_reruns_a_launch() {
    let mut h = Harness::new();
    let mut e = LockingEffects {
        inner: FakeEffects {
            exists: true,
            ..Default::default()
        },
        path: h.temp.path().join("teams/member-launch-v1.sqlite3"),
        lock: None,
        lock_on: "launch",
        fired: false,
    };
    let mut r = runtime(&h);
    let mut errors = 0;
    for _ in 0..10 {
        if r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234)
            .is_err()
        {
            errors += 1;
            e.lock = None; // the disk recovers
        }
        if errors > 0
            && e.inner
                .events
                .iter()
                .filter(|x| *x == "launch:lead")
                .count()
                > 1
        {
            break;
        }
    }
    eprintln!("errors={errors} events={:?}", e.inner.events);
    assert_eq!(errors, 1);
    assert_eq!(
        e.inner
            .events
            .iter()
            .filter(|x| *x == "launch:lead")
            .count(),
        1,
        "the launch must not run twice"
    );
    assert_eq!(cards(&h).len(), 1);
}

#[test]
fn a_changed_lead_invalidates_the_member_plan_before_an_effect() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "m1").is_some_and(|r| r.state == State::WorktreeIntent)
    });
    let mut saved = row(&h, "m1");
    saved.plan.as_mut().unwrap().parent = Some(zerus_team_plan::model::SessionKey {
        machine: MACHINE.into(),
        session_id: "launch_id:00000000-0000-4000-8000-000000000099".into(),
    });
    Ledger::open(h.temp.path())
        .unwrap()
        .save(&mut saved, 1234)
        .unwrap();
    advance(&mut r, &mut h, &mut e, 4);
    assert!(!e.events.contains(&"worktree:m1".into()));
    assert_eq!(row(&h, "m1").state, State::Blocked);
}
#[test]
fn a_member_paused_after_reservation_never_runs_its_effect() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "m1").is_some_and(|r| r.state == State::WorktreeIntent)
    });
    h.run(
        &ops::tests::service(),
        ops::Operation::SetMemberState {
            member: "m1".into(),
            state: MemberState::Paused,
        },
    )
    .unwrap();
    advance(&mut r, &mut h, &mut e, 20);
    assert!(!e.events.contains(&"worktree:m1".into()));
}
#[test]
fn a_saved_worktree_can_launch_after_restart_without_being_created_again() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "lead").is_some_and(|r| r.state == State::WorktreeDone)
    });
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 60);
    assert_eq!(
        e.events
            .iter()
            .filter(|e| e.as_str() == "worktree:lead")
            .count(),
        1
    );
    assert_eq!(
        e.events
            .iter()
            .filter(|e| e.as_str() == "launch:lead")
            .count(),
        1
    );
    assert!(cards(&h).is_empty());
}
#[test]
fn a_session_at_folder_trust_binds_without_a_conversation() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
    for s in e.sessions.values_mut() {
        s.conversation_id.clear();
    }
    tick(&mut r, &mut h, &mut e);
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert!(cards(&h).is_empty());
}
#[test]
fn an_unknown_child_waits_five_minutes_for_identity_and_can_still_bind() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        unknown: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 12);
    assert!(cards(&h).is_empty());
    assert_eq!(row(&h, "lead").state, State::LaunchUnknown);
    let plan = e.plans[0].clone();
    e.unknown = false;
    e.sessions.insert(
        plan.launch_id.clone(),
        ObservedSession {
            machine: plan.machine.clone(),
            launch_id: plan.launch_id.clone(),
            agent: plan.agent.clone(),
            account: plan.account.clone(),
            worktree: row(&h, "lead").worktree.unwrap(),
            parent: None,
            name: "claude/example/trust".into(),
            session_id: String::new(),
            run_id: String::new(),
            conversation_id: String::new(),
        },
    );
    r.tick(&mut h.store, &mut e, ops::tests::TEAM, 61_234)
        .unwrap();
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert!(cards(&h).is_empty());
}

#[test]
fn account_choice_retries_after_reset_and_closes_its_one_card_as_stale() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    for a in &mut e.accounts {
        a.windows[0].used_percent = 100.;
        a.windows[0].resets_at = Some(80);
    }
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    assert_eq!(cards(&h).len(), 1);
    assert!(cards(&h)[0]
        .text
        .contains("Automatic retry follows that account window reset"));
    let card = cards(&h)[0].seq;
    assert_eq!(e.account_reads, 1);
    for a in &mut e.accounts {
        a.windows[0].used_percent = 30.;
    }
    r.begin_pass();
    for _ in 0..8 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 80_000)
            .unwrap();
    }
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert_eq!(
        h.store
            .card(ops::tests::TEAM, card)
            .unwrap()
            .unwrap()
            .decision
            .as_deref(),
        Some("stale")
    );
}
#[test]
fn a_persons_retry_decision_bypasses_the_backoff_without_impersonation() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        account_error: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    assert_eq!(cards(&h).len(), 1);
    let card = cards(&h)[0].seq;
    h.run(
        &ops::tests::person(),
        ops::Operation::DecideCard {
            card,
            decision: "retry".into(),
        },
    )
    .unwrap();
    e.account_error = false;
    r.begin_pass();
    tick(&mut r, &mut h, &mut e);
    assert_eq!(row(&h, "lead").state, State::WorktreeIntent);
    assert_eq!(e.account_reads, 2);
}
#[test]
fn placement_precedes_account_inspection_and_repeated_faults_are_backed_off() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        placement_error: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    assert_eq!(e.account_reads, 0);
    assert_eq!(e.placements, 1);
    assert_eq!(cards(&h).len(), 1);
    e.placement_error = false;
    r.begin_pass();
    for _ in 0..60 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 61_234)
            .unwrap();
    }
    assert_eq!(e.account_reads, 1);
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
}
#[test]
fn account_reads_are_cached_once_per_pass_and_failures_are_never_signouts() {
    let mut h = Harness::new();
    let mut e = FakeEffects::default();
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 60);
    assert_eq!(e.account_reads, 1);
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        account_error: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    assert_eq!(e.account_reads, 1);
    assert_eq!(row(&h, "lead").state, State::Retry);
    assert!(cards(&h)[0].text.contains("inspection failed"));
    assert!(!cards(&h)[0].text.contains("no_account_available"));
}
#[test]
fn observation_read_errors_do_not_turn_a_running_launch_into_uncertainty() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
    e.read_error = true;
    for now in [1234, 61_234, 181_234] {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, now).unwrap();
    }
    assert_eq!(row(&h, "lead").state, State::Submitted);
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].kind, "launch_blocked");
    e.read_error = false;
    r.tick(&mut h.store, &mut e, ops::tests::TEAM, 500_000)
        .unwrap();
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
}
#[test]
fn incomplete_parent_identity_retries_are_bounded_and_do_not_block_other_members() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        parent_pending: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 60);
    assert!(h
        .store
        .member(ops::tests::TEAM, "m3")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert!(cards(&h).is_empty());
    for round in 1..=12 {
        for _ in 0..30 {
            r.tick(
                &mut h.store,
                &mut e,
                ops::tests::TEAM,
                1234 + round * 60_000,
            )
            .unwrap();
        }
    }
    let saved = row(&h, "m1");
    assert_eq!(saved.parent_tries, 10);
    assert_eq!(saved.parent_result.as_deref(), Some("identity_timeout"));
    assert!(cards(&h).is_empty());
}
#[test]
fn stopped_and_removed_pending_launches_are_noted_for_the_lifecycle_owner() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
    h.run(&ops::tests::person(), ops::Operation::Stop).unwrap();
    tick(&mut r, &mut h, &mut e);
    assert!(row(&h, "lead").late_after_stop);
}
#[test]
fn an_unknown_named_account_retries_without_a_second_card() {
    let mut h = Harness::empty();
    let mut create = ops::tests::create();
    if let ops::Operation::Create { bundle, .. } = &mut create {
        bundle.template.lead.account = AccountChoice::Named {
            account: "later".into(),
        };
    }
    h.run(&ops::tests::person(), create).unwrap();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 10);
    assert_eq!(cards(&h).len(), 1);
    assert!(cards(&h)[0].text.contains("unknown_account"));
    let card = cards(&h)[0].seq;
    e.accounts.push(account("later"));
    r.begin_pass();
    for _ in 0..10 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 61_234)
            .unwrap();
    }
    assert_eq!(e.plans[0].account, "later");
    assert_eq!(
        h.store
            .card(ops::tests::TEAM, card)
            .unwrap()
            .unwrap()
            .decision
            .as_deref(),
        Some("stale")
    );
}

#[test]
fn losing_an_outcome_never_repeats_branch_worktree_or_parent_effects() {
    for kind in ["branch", "worktree", "parent"] {
        let mut h = Harness::new();
        let mut e = LockingEffects {
            inner: FakeEffects {
                exists: kind != "branch",
                ..Default::default()
            },
            path: h.temp.path().join("teams/member-launch-v1.sqlite3"),
            lock: None,
            lock_on: kind,
            fired: false,
        };
        let mut r = runtime(&h);
        let mut errors = 0;
        for _ in 0..60 {
            if r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234)
                .is_err()
            {
                errors += 1;
                e.lock = None;
            }
        }
        assert_eq!(errors, 1, "{kind}");
        let event = match kind {
            "branch" => "branch",
            "worktree" => "worktree:lead",
            _ => "parent:m1",
        };
        assert_eq!(
            e.inner
                .events
                .iter()
                .filter(|e| e.as_str() == event)
                .count(),
            1,
            "{kind} was replayed"
        );
    }
}

// Regressions from the review07b reproductions, asserting the corrected behavior.
#[test]
fn review07b_a_failed_person_retry_opens_exactly_one_new_card() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        account_error: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    advance(&mut r, &mut h, &mut e, 20);
    let open = |h: &Harness| cards(h).into_iter().filter(|c| c.state == "open").count();
    assert_eq!(open(&h), 1);
    let card = cards(&h)[0].seq;
    h.run(
        &ops::tests::person(),
        ops::Operation::DecideCard {
            card,
            decision: "retry".into(),
        },
    )
    .unwrap();
    // The condition still holds: the retry fails again.
    r.begin_pass();
    for _ in 0..5 {
        let _ = r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234);
    }
    let lead = row(&h, "lead");
    eprintln!(
        "REVIEW07B state={:?} failures={} card_done={} open_cards={} all_cards={}",
        lead.state,
        lead.failures,
        lead.card_done,
        open(&h),
        cards(&h).len()
    );
    // Much later, still failing:
    r.begin_pass();
    for _ in 0..5 {
        let _ = r.tick(&mut h.store, &mut e, ops::tests::TEAM, 10_000_000);
    }
    let lead = row(&h, "lead");
    eprintln!(
        "REVIEW07B later state={:?} failures={} open_cards={}",
        lead.state,
        lead.failures,
        open(&h)
    );
    assert_eq!(open(&h), 1, "the failed requested retry must be visible");
    assert_eq!(
        cards(&h).len(),
        2,
        "automatic retries must keep the new card"
    );
}
#[test]
fn review07b_a_full_ledger_budget_opens_one_durable_team_card() {
    let mut h = Harness::new();
    drop(Ledger::open(h.temp.path()).unwrap());
    let db =
        rusqlite::Connection::open(h.temp.path().join("teams/member-launch-v1.sqlite3")).unwrap();
    db.execute_batch("UPDATE launch_budget SET rows=16384 WHERE id=1")
        .unwrap();
    drop(db);
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    let mut errors = 0;
    for _ in 0..20 {
        if let Err(p) = r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234) {
            errors += 1;
            if errors == 1 {
                eprintln!("REVIEW07B tick error: {} {}", p.code, p.message);
            }
        }
    }
    let lead = Ledger::open(h.temp.path())
        .unwrap()
        .get(ops::tests::TEAM, "lead")
        .unwrap();
    eprintln!(
        "REVIEW07B errors={errors} lead_row={} cards={} events={:?}",
        lead.is_some(),
        cards(&h).len(),
        e.events
    );
    assert!(lead.is_none());
    assert_eq!(
        cards(&h).len(),
        1,
        "one card tells the person about the full ledger"
    );
    drop(r);
    advance(&mut runtime(&h), &mut h, &mut e, 20);
    assert_eq!(cards(&h).len(), 1, "restart must reuse the budget card");
}

#[test]
fn a_submitted_launch_with_missing_identity_expires_once_without_relaunching() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
    e.sessions.clear();
    for now in [1234, 61_234, 301_234, 500_000, 10_000_000] {
        for _ in 0..3 {
            r.tick(&mut h.store, &mut e, ops::tests::TEAM, now).unwrap();
        }
    }
    assert_eq!(row(&h, "lead").state, State::Blocked);
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].kind, "launch_uncertain");
    assert_eq!(e.plans.len(), 1);
}
#[test]
fn retry_card_distinguishes_automatic_backoff_from_a_persons_retry() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        account_error: true,
        ..Default::default()
    };
    advance(&mut runtime(&h), &mut h, &mut e, 10);
    let text = &cards(&h)[0].text;
    assert!(!text.contains("within 60 seconds"));
    assert!(text.contains("Automatic retry"));
    assert!(text.contains("Retry checks sooner"));
}

#[test]
fn a_created_worktree_can_finish_its_ownership_read_after_restart_without_recreation() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        read_error: true,
        worktree_unverified: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "lead").is_some_and(|r| r.state == State::WorktreeVerify)
    });
    assert_eq!(
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234)
            .unwrap_err()
            .code,
        code::INVALID_STATE
    );
    assert_eq!(row(&h, "lead").state, State::WorktreeVerify);
    drop(r);
    let mut r = runtime(&h);
    e.read_error = false;
    for _ in 0..10 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 61_234)
            .unwrap();
    }
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert_eq!(
        e.events
            .iter()
            .filter(|e| e.as_str() == "worktree:lead")
            .count(),
        1
    );
}
#[test]
fn ownership_verification_before_a_launch_backs_off_and_recovers_without_a_refusal() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        launch_verification_unavailable: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |h, _| {
        row_if(h, "lead").is_some_and(|r| r.state == State::LaunchIntent)
    });
    for now in [1234, 61_234, 181_234] {
        for _ in 0..2 {
            let _ = r.tick(&mut h.store, &mut e, ops::tests::TEAM, now);
        }
        assert_eq!(row(&h, "lead").state, State::LaunchReady);
        assert!(row(&h, "lead").retry_at > now);
    }
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].kind, "launch_blocked");
    assert!(!e.events.iter().any(|e| e.starts_with("launch:")));
    e.launch_verification_unavailable = false;
    for _ in 0..4 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 500_000)
            .unwrap();
    }
    assert!(h
        .store
        .member(ops::tests::TEAM, "lead")
        .unwrap()
        .unwrap()
        .session
        .is_some());
    assert_eq!(
        e.events
            .iter()
            .filter(|e| e.as_str() == "launch:lead")
            .count(),
        1
    );
    assert_eq!(cards(&h)[0].state, "closed");
    assert_eq!(cards(&h)[0].decision.as_deref(), Some("stale"));
}

#[test]
fn a_budget_card_survives_its_reserved_outbox_and_closes_when_capacity_recovers() {
    let mut h = Harness::new();
    let team = h.store.team(ops::tests::TEAM).unwrap().unwrap();
    let mut ledger = Ledger::open(h.temp.path()).unwrap();
    // Crash after the independent notice row was inserted, before opening its card.
    let mut alert = super::ledger::Row::new(&team, super::ledger::BUDGET, EPOCH, State::Retry);
    alert.problem = Some(
        zerus_team_plan::Problem::new(
            code::OVER_CAP,
            "Team member creation is temporarily at capacity.",
        )
        .with_refs(["member_launch_ledger"]),
    );
    ledger.insert(&alert, 1234).unwrap();
    drop(ledger);
    let db =
        rusqlite::Connection::open(h.temp.path().join("teams/member-launch-v1.sqlite3")).unwrap();
    db.execute("UPDATE launch_budget SET rows=16384 WHERE id=1", [])
        .unwrap();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    for _ in 0..5 {
        let _ = r.tick(&mut h.store, &mut e, ops::tests::TEAM, 1234);
    }
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].kind, "launch_capacity");
    assert!(cards(&h)[0].text.starts_with("Team member creation:"));
    assert!(!cards(&h)[0].text.contains("@budget"));
    assert!(!cards(&h)[0].text.contains("member_launch_ledger"));
    drop(r);
    db.execute(
        "UPDATE launch_budget SET rows=(SELECT COUNT(*) FROM originals) WHERE id=1",
        [],
    )
    .unwrap();
    drop(db);
    let mut r = runtime(&h);
    for _ in 0..8 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 61_234)
            .unwrap();
    }
    assert_eq!(cards(&h).len(), 1);
    assert_eq!(cards(&h)[0].state, "closed");
    assert_eq!(cards(&h)[0].decision.as_deref(), Some("stale"));
}

#[test]
fn missing_submitted_identity_replaces_an_earlier_read_error_card_with_uncertainty() {
    let mut h = Harness::new();
    let mut e = FakeEffects {
        exists: true,
        ..Default::default()
    };
    let mut r = runtime(&h);
    until(&mut r, &mut h, &mut e, |_, e| !e.plans.is_empty());
    e.read_error = true;
    for now in [1234, 61_234, 181_234] {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, now).unwrap();
    }
    assert_eq!(cards(&h)[0].kind, "launch_blocked");
    e.read_error = false;
    e.sessions.clear();
    for _ in 0..3 {
        r.tick(&mut h.store, &mut e, ops::tests::TEAM, 600_000)
            .unwrap();
    }
    let all = cards(&h);
    let open: Vec<_> = all.iter().filter(|c| c.state == "open").collect();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].kind, "launch_uncertain");
    assert_eq!(all[0].decision.as_deref(), Some("stale"));
}
