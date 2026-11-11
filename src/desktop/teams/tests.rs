use super::{
    launch::*,
    ledger::{Ledger, State, BRANCH},
    ports::*,
};
use crate::teams::ops::{self, tests::Harness};
use std::collections::BTreeMap;
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
        Ok(self.accounts.clone())
    }
    fn worktree(&mut self, plan: &Plan, _: &str) -> Effect<Worktree> {
        self.events.push(format!("worktree:{}", plan.member));
        Effect::Submitted(Worktree {
            path: format!("/workspace/example-{}", plan.member),
            ownership_nonce: format!("owned-{}", plan.member),
        })
    }
    fn launch(&mut self, plan: &Plan, tree: &Worktree, _: &str) -> Effect<()> {
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
    fn observe(&mut self, plan: &Plan) -> Result<Option<ObservedSession>> {
        let mut s = self.sessions.get(&plan.launch_id).cloned();
        if self.mismatch {
            if let Some(s) = &mut s {
                s.launch_id = uuid::Uuid::new_v4().to_string();
            }
        }
        Ok(s)
    }
    fn parent(&mut self, plan: &Plan, _: &ObservedSession, _: &str) -> String {
        self.events.push(format!("parent:{}", plan.member));
        "manual".into()
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
        drop(r);
        let mut replacement = runtime(&h);
        advance(&mut replacement, &mut h, &mut e, 60);
        let mut ids = std::collections::BTreeSet::new();
        assert!(
            e.plans.iter().all(|p| ids.insert(p.member.clone())),
            "duplicate at {stop_after}"
        );
        assert!(e.plans.len() >= before);
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
        while e.plans.is_empty() {
            tick(&mut r, &mut h, &mut e);
        }
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
        while e.plans.is_empty() {
            tick(&mut r, &mut h, &mut e);
        }
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
    while row_if(&h, "lead").is_none_or(|r| r.state != State::LaunchIntent) {
        tick(&mut r, &mut h, &mut e);
    }
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
        unknown: true,
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
    while row_if(&h, "m1").is_none_or(|r| r.state != State::ParentIntent) {
        tick(&mut r, &mut h, &mut e);
    }
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
    while row_if(&h, "m1").is_none_or(|r| r.state != State::ParentIntent) {
        tick(&mut r, &mut h, &mut e);
    }
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
fn another_team_cannot_replace_a_live_original() {
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
    tick(&mut r, &mut h, &mut e);
    assert!(r.busy());
    assert!(!r
        .tick(&mut h.store, &mut e, ops::tests::TEAM_B, 1234)
        .unwrap());
    assert!(e.events.is_empty());
    advance(&mut r, &mut h, &mut e, 60);
    assert!(!r.busy());
    assert_eq!(e.plans.len(), 4);
    assert!(cards(&h).is_empty());
}
