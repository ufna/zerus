use super::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Default)]
struct Durable {
    rows: BTreeMap<String, Record>,
    audits: BTreeMap<String, String>,
    cards: BTreeMap<String, String>,
    notes: Vec<String>,
    resumes: Vec<i64>,
}

struct Fake {
    binding: Binding,
    facts: Facts,
    snapshot: Snapshot,
    owner: Ownership,
    durable: Rc<RefCell<Durable>>,
    answers: usize,
    resumes: usize,
    briefs: usize,
    unknown: bool,
    busy: bool,
    before_original: Option<fn(&mut Fake)>,
    crash_after_audit: bool,
    crash_after_card: bool,
    fail_audit: bool,
    fail_finish: bool,
}

impl Fake {
    fn new() -> Self {
        let (binding, mut facts, snapshot) = super::super::policy::tests::sample();
        facts.team.settings.delegations.confirm_trust = true;
        facts.team.settings.delegations.resume_exited = true;
        let owner = Ownership {
            folder: binding.folder.clone(),
            common: binding.common.clone(),
            nonce: binding.nonce.clone(),
            launch: binding.launch.clone(),
            exists: true,
            unchanged: true,
        };
        Self {
            binding,
            facts,
            snapshot,
            owner,
            durable: Rc::default(),
            answers: 0,
            resumes: 0,
            briefs: 0,
            unknown: false,
            busy: false,
            before_original: None,
            crash_after_audit: false,
            crash_after_card: false,
            fail_audit: false,
            fail_finish: false,
        }
    }
    fn run(&mut self, epoch: &str, now: i64) -> Result<()> {
        step(self, &self.binding.clone(), epoch, now)
    }
    fn exited(&mut self, n: usize) {
        self.binding.run = format!("run-{n}");
        self.binding.conversation = "conversation-1".into();
        self.facts.binding = self.binding.clone();
        self.snapshot.run = self.binding.run.clone();
        self.snapshot.conversation = self.binding.conversation.clone();
        self.snapshot.questions.clear();
        self.snapshot.live = false;
        self.snapshot.ended = true;
        self.snapshot.self_exit = true;
        self.snapshot.exit_identity = format!("exit-{n}");
    }
}

impl Ports for Fake {
    fn facts(&mut self, _: &Binding) -> Result<Facts> {
        Ok(Facts {
            team: self.facts.team.clone(),
            member: self.facts.member.clone(),
            binding: self.facts.binding.clone(),
            preview: self.facts.preview,
            planning_paused: self.facts.planning_paused,
        })
    }
    fn observe(&mut self, _: &Binding) -> Result<Snapshot> {
        Ok(self.snapshot.clone())
    }
    fn ownership(&mut self, _: &Binding) -> Result<Ownership> {
        Ok(self.owner.clone())
    }
    fn unsettled(&mut self, _: &Binding) -> Result<Option<Record>> {
        Ok(self
            .durable
            .borrow()
            .rows
            .values()
            .find(|r| {
                r.state == "prepared"
                    || (r.audit.is_none() && r.plan.kind != Kind::Notice)
                    || (r.state != "approved" && !r.card_done)
                    || (r.state == "approved" && feed_line(&r.plan) && !r.card_done)
                    || (r.audit.is_some() && !r.audit_done)
            })
            .cloned())
    }
    fn pending_brief(&mut self, b: &Binding) -> Result<Option<Plan>> {
        let db = self.durable.borrow();
        let plan = db.rows.values().find_map(|row| {
            row.result
                .as_ref()
                .filter(|target| {
                    row.plan.kind == Kind::Resume && row.state == "approved" && target.run == b.run
                })
                .map(|target| brief(row, target))
        });
        Ok(plan.filter(|plan| !db.rows.contains_key(&plan.key)))
    }
    fn claim(&mut self, plan: &Plan, epoch: &str, now: i64) -> Result<Claim> {
        let mut db = self.durable.borrow_mut();
        if let Some(row) = db.rows.get_mut(&plan.key) {
            let fresh = row.state == "waiting" && row.plan == *plan;
            if fresh {
                row.state = "prepared".into();
                row.epoch = epoch.into();
                row.original = uuid::Uuid::new_v4().to_string();
            }
            return Ok(Claim {
                record: row.clone(),
                fresh,
                limited: false,
            });
        }
        let limited = plan.kind == Kind::Resume
            && db
                .resumes
                .iter()
                .filter(|at| **at > now - 3_600_000)
                .count()
                >= 3;
        if plan.kind == Kind::Resume && !limited {
            db.resumes.push(now);
        }
        let row = Record {
            plan: plan.clone(),
            original: uuid::Uuid::new_v4().to_string(),
            epoch: epoch.into(),
            state: "prepared".into(),
            audit: None,
            audit_done: false,
            result: None,
            detail: String::new(),
            card_request: uuid::Uuid::new_v4().to_string(),
            card_done: false,
            at: now,
        };
        db.rows.insert(plan.key.clone(), row.clone());
        Ok(Claim {
            record: row,
            fresh: true,
            limited,
        })
    }
    fn save(&mut self, row: &Record) -> Result<()> {
        self.durable
            .borrow_mut()
            .rows
            .insert(row.plan.key.clone(), row.clone());
        if row.state == "approved" && row.plan.kind == Kind::Resume {
            if let Some(target) = &row.result {
                if self.binding.run != target.run {
                    self.binding.run = target.run.clone();
                    self.binding.conversation = target.conversation.clone();
                    self.facts.binding = self.binding.clone();
                    self.snapshot = target.clone();
                }
            }
        }
        Ok(())
    }
    fn audit_begin(&mut self, plan: &Plan) -> Result<Audit> {
        if self.fail_audit {
            return Err("Audit storage unavailable".into());
        }
        let mut db = self.durable.borrow_mut();
        let fresh = !db.audits.contains_key(&plan.key);
        db.audits
            .entry(plan.key.clone())
            .or_insert("prepared".into());
        Ok(Audit {
            id: 1,
            request: uuid::Uuid::new_v4().to_string(),
            answers: json!([]),
            fresh,
        })
    }
    fn audit_finish(&mut self, row: &Record) -> Result<()> {
        if self.fail_finish {
            return Err("Audit finish unavailable".into());
        }
        self.durable
            .borrow_mut()
            .audits
            .insert(row.plan.key.clone(), row.state.clone());
        Ok(())
    }
    fn card(&mut self, row: &Record) -> Result<()> {
        self.durable
            .borrow_mut()
            .cards
            .insert(row.card_request.clone(), row.detail.clone());
        if self.crash_after_card {
            return Err("Restart after the local card committed".into());
        }
        Ok(())
    }
    fn note(&mut self, row: &Record) -> Result<()> {
        self.durable.borrow_mut().notes.push(feed_text(&row.plan));
        if self.crash_after_card {
            return Err("Restart after the local note committed".into());
        }
        Ok(())
    }
    fn original(
        &mut self,
        row: &Record,
        check: &mut dyn FnMut(&mut Self) -> Result<()>,
    ) -> Outcome {
        assert_eq!(self.durable.borrow().audits[&row.plan.key], "prepared");
        assert_eq!(
            self.durable.borrow().rows[&row.plan.key].audit,
            row.audit,
            "The durable effect row must name its audit before delivery"
        );
        if self.crash_after_audit {
            return Outcome::Unknown("Synthetic process restart before delivery".into());
        }
        if let Some(change) = self.before_original.take() {
            change(self);
        }
        if let Err(reason) = check(self) {
            return Outcome::NotStarted(reason);
        }
        if self.busy {
            return Outcome::Retry;
        }
        match row.plan.kind {
            Kind::Trust => self.answers += 1,
            Kind::Resume => self.resumes += 1,
            Kind::Brief => self.briefs += 1,
            Kind::Notice => panic!("A notice cannot start a child"),
        }
        if self.unknown {
            Outcome::Unknown("Synthetic lost receipt".into())
        } else {
            if row.plan.kind == Kind::Resume {
                let mut target = self.snapshot.clone();
                target.run = format!("resumed-{}", target.run);
                target.ended = false;
                target.live = true;
                target.self_exit = false;
                target.receipt_at = Some(row.at);
                target.archive = None;
                Outcome::Approved(Some(Box::new(target)))
            } else {
                Outcome::Approved(None)
            }
        }
    }
}

#[test]
fn journal_is_reserved_before_answer_and_finished_after_receipt() {
    let mut f = Fake::new();
    f.run("epoch-1", 10).unwrap();
    assert_eq!(f.answers, 1);
    assert!(f
        .durable
        .borrow()
        .audits
        .values()
        .all(|state| state == "approved"));
}

#[test]
fn unknown_answer_and_a_restart_never_repeat_and_make_one_card() {
    let mut f = Fake::new();
    f.unknown = true;
    f.run("epoch-1", 10).unwrap();
    for epoch in ["epoch-1", "epoch-2", "epoch-3"] {
        f.run(epoch, 20).unwrap();
    }
    assert_eq!(f.answers, 1);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn restart_after_journal_reservation_never_starts_an_answer() {
    let mut f = Fake::new();
    f.crash_after_audit = true;
    f.run("epoch-1", 10).unwrap();
    f.crash_after_audit = false;
    f.run("epoch-2", 20).unwrap();
    f.run("epoch-3", 30).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn changed_hash_between_read_and_answer_goes_to_person() {
    let mut f = Fake::new();
    f.before_original = Some(|f| f.snapshot.questions[0]["question_hash"] = json!("cd".repeat(32)));
    f.run("epoch-1", 10).unwrap();
    f.run("epoch-2", 20).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn delegation_revoked_between_read_and_answer_withholds_delivery() {
    let mut f = Fake::new();
    f.before_original = Some(|f| f.facts.team.settings.delegations.confirm_trust = false);
    f.run("epoch-1", 10).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn missing_replaced_or_foreign_ownership_is_never_used() {
    for change in [0, 1, 2, 3] {
        let mut f = Fake::new();
        match change {
            0 => f.owner.exists = false,
            1 => f.owner.unchanged = false,
            2 => f.owner.nonce.push('x'),
            _ => f.owner.launch.push('x'),
        }
        f.run("epoch-1", 10).unwrap();
        assert_eq!(f.answers, 0);
        assert_eq!(f.durable.borrow().cards.len(), 1);
    }
}

#[test]
fn three_resumes_per_hour_and_the_fourth_refused_across_restart() {
    let mut f = Fake::new();
    for n in 1..=4 {
        f.exited(n);
        f.run(&format!("epoch-{n}"), n as i64 * 1000).unwrap();
    }
    assert_eq!(f.resumes, 3);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    assert_eq!(f.durable.borrow().resumes.len(), 3);
}

#[test]
fn member_stopped_by_a_person_or_nonself_exit_never_resumes() {
    for paused in [false, true] {
        let mut f = Fake::new();
        f.exited(1);
        if paused {
            f.facts.member.state = zerus_team_plan::model::MemberState::Paused;
        } else {
            f.snapshot.self_exit = false;
        }
        f.run("epoch-1", 100).unwrap();
        assert_eq!(f.resumes, 0);
        assert_eq!(f.durable.borrow().cards.len(), 1);
    }
}

/// A session the person paused, or is pausing or terminating, reads as
/// ended and paused: nothing is planned, no card opens and the run keeps its
/// one resume (zerus-63rru.56).
#[test]
fn a_paused_session_plans_nothing_and_keeps_its_resume() {
    let mut f = Fake::new();
    f.exited(1);
    f.snapshot.self_exit = false;
    f.snapshot.paused = true;
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.resumes, 0);
    assert!(f.durable.borrow().rows.is_empty(), "nothing claimed");
    assert!(f.durable.borrow().cards.is_empty(), "no owner card");
    // Once the member exits by itself in that run, its resume is still there.
    f.snapshot.paused = false;
    f.snapshot.self_exit = true;
    f.run("epoch-1", 200).unwrap();
    assert_eq!(f.resumes, 1);
}

/// A folder-trust prompt the policy refuses, and every inactive team, keep
/// their one notice card for the person and are never answered.
#[test]
fn a_refused_trust_prompt_and_inactive_teams_make_one_card_without_an_answer() {
    for field in [
        "two",
        "folder",
        "team_paused",
        "team_stopped",
        "trust_off",
        "same_folder_other_session",
    ] {
        let mut f = Fake::new();
        match field {
            "two" => f.snapshot.questions.push(f.snapshot.questions[0].clone()),
            "folder" => {
                f.snapshot.questions[0]["questions"][0]["body"] =
                    json!("/workspace/unowned\n\nDisclosure")
            }
            "team_paused" => f.facts.team.status = zerus_team_plan::model::TeamStatus::Paused,
            "team_stopped" => f.facts.team.status = zerus_team_plan::model::TeamStatus::Stopped,
            "trust_off" => f.facts.team.settings.delegations.confirm_trust = false,
            _ => f.snapshot.session.push('x'),
        }
        f.run("epoch-1", 100).unwrap();
        f.run("epoch-2", 200).unwrap();
        assert_eq!(f.answers, 0, "{field}");
        assert_eq!(f.durable.borrow().cards.len(), 1, "{field}");
    }
}

#[test]
fn resume_delegation_and_missing_checkout_are_rechecked_before_delivery() {
    for field in [
        "off",
        "off_at_spawn",
        "missing",
        "replaced",
        "archive",
        "exit",
    ] {
        let mut f = Fake::new();
        f.exited(1);
        match field {
            "off" => f.facts.team.settings.delegations.resume_exited = false,
            "off_at_spawn" => {
                f.before_original =
                    Some(|f| f.facts.team.settings.delegations.resume_exited = false)
            }
            "missing" => f.owner.exists = false,
            "replaced" => f.before_original = Some(|f| f.owner.unchanged = false),
            "archive" => {
                f.before_original = Some(|f| f.snapshot.archive = Some("archive-other".into()))
            }
            _ => f.before_original = Some(|f| f.snapshot.exit_identity.push('x')),
        }
        f.run("epoch-1", 100).unwrap();
        f.run("epoch-2", 200).unwrap();
        assert_eq!(f.resumes, 0, "{field}");
        assert_eq!(f.durable.borrow().cards.len(), 1, "{field}");
    }
}

#[test]
fn restart_recovers_an_unfinished_claim_even_after_the_prompt_disappears() {
    let mut f = Fake::new();
    let plan = candidate(
        &f.binding,
        &f.snapshot,
        Kind::Trust,
        f.snapshot.questions[0].clone(),
    );
    let _claim = f.claim(&plan, "epoch-1", 10).unwrap();
    let _audit = f.audit_begin(&plan).unwrap(); // Crash before saving its ID in the other database.
    f.snapshot.questions.clear();
    f.run("epoch-2", 20).unwrap();
    f.run("epoch-3", 30).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    assert!(f
        .durable
        .borrow()
        .audits
        .values()
        .all(|state| state == "unknown"));
}

#[test]
fn an_old_audit_reservation_never_admits_a_new_original() {
    let mut f = Fake::new();
    let plan = candidate(
        &f.binding,
        &f.snapshot,
        Kind::Trust,
        f.snapshot.questions[0].clone(),
    );
    f.audit_begin(&plan).unwrap();
    f.run("epoch-2", 100).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn resume_brief_is_its_own_guarded_send_to_the_receipts_new_run() {
    let mut f = Fake::new();
    f.exited(1);
    let old = f.binding.run.clone();
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.resumes, 1);
    assert_ne!(f.binding.run, old);
    f.snapshot.ready = true;
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.briefs, 1);
    let db = f.durable.borrow();
    let brief = db
        .rows
        .values()
        .find(|r| r.plan.kind == Kind::Brief)
        .unwrap();
    assert_eq!(brief.plan.snapshot.run, f.binding.run);
    assert_eq!(brief.plan.snapshot.conversation, f.binding.conversation);
    assert!(brief.plan.text.contains(&f.binding.team));
    assert!(brief.plan.text.contains(&f.binding.role));
    assert!(brief.plan.text.contains("hgs team current"));
    assert!(brief.plan.parent.is_some());
    assert!(db.audits.values().all(|state| state == "approved"));
}

#[test]
fn unknown_brief_is_never_sent_again_after_restart() {
    let mut f = Fake::new();
    f.exited(1);
    f.run("epoch-1", 100).unwrap();
    f.snapshot.ready = true;
    f.unknown = true;
    f.run("epoch-2", 200).unwrap();
    f.unknown = false;
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.resumes, 1);
    assert_eq!(f.briefs, 1);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn revoked_resume_delegation_or_changed_target_withholds_the_brief() {
    for field in ["off", "run", "conversation", "paused", "questions"] {
        let mut f = Fake::new();
        f.exited(1);
        f.run("epoch-1", 100).unwrap();
        f.snapshot.ready = true;
        f.before_original = Some(match field {
            "off" => |f| f.facts.team.settings.delegations.resume_exited = false,
            "run" => |f| f.snapshot.run.push('x'),
            "conversation" => |f| f.snapshot.conversation.push('x'),
            "paused" => |f| f.snapshot.paused = true,
            // The agent's own question, as its producer writes it: it
            // withholds the brief and is the person's; it plans no notice of
            // its own (zerus-63rru.73).
            _ => |f| {
                f.snapshot
                    .questions
                    .push(crate::state::trust_fixture::ordinary_question(
                        "claude",
                        "conversation-1",
                    ))
            },
        });
        f.run("epoch-2", 200).unwrap();
        f.run("epoch-3", 300).unwrap();
        assert_eq!(f.briefs, 0, "{field}");
        assert_eq!(f.durable.borrow().cards.len(), 1, "{field}");
    }
}

#[test]
fn crash_after_card_commit_reuses_its_durable_identity() {
    let mut f = Fake::new();
    f.unknown = true;
    f.crash_after_card = true;
    assert!(f.run("epoch-1", 100).is_err());
    f.crash_after_card = false;
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn an_uncertain_resume_is_not_another_self_exit_when_metadata_changes() {
    let mut f = Fake::new();
    f.exited(1);
    f.unknown = true;
    f.run("epoch-1", 100).unwrap();
    f.unknown = false;
    f.snapshot.exit_identity.push('x');
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.resumes, 1);
    assert_eq!(f.durable.borrow().resumes.len(), 1);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn audit_storage_failure_cannot_hide_the_blocked_card_or_create_a_later_delivery() {
    let mut f = Fake::new();
    f.fail_audit = true;
    assert!(f.run("epoch-1", 100).is_err());
    assert!(f.run("epoch-2", 200).is_err());
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    f.fail_audit = false;
    f.run("epoch-3", 300).unwrap();
    f.run("epoch-4", 400).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    assert!(f
        .durable
        .borrow()
        .audits
        .values()
        .all(|state| state == "cancelled"));
}

#[test]
fn an_unknown_delivery_has_one_card_even_while_audit_finish_is_unavailable() {
    let mut f = Fake::new();
    f.unknown = true;
    f.fail_finish = true;
    assert!(f.run("epoch-1", 100).is_err());
    assert!(f.run("epoch-2", 200).is_err());
    assert_eq!(f.answers, 1);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    f.fail_finish = false;
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1);
    assert!(f
        .durable
        .borrow()
        .audits
        .values()
        .all(|state| state == "unknown"));
}

#[test]
fn busy_admission_makes_no_card_and_a_later_pass_starts_once() {
    let mut p = Fake::new();
    p.busy = true;
    p.run("epoch-1", 100).unwrap();
    assert_eq!(p.answers, 0);
    assert!(p.durable.borrow().cards.is_empty());
    assert_eq!(
        p.durable.borrow().rows.values().next().unwrap().state,
        "waiting"
    );
    assert_eq!(
        p.durable.borrow().audits.values().next().unwrap(),
        "prepared"
    );
    p.busy = false;
    p.run("epoch-2", 200).unwrap();
    assert_eq!(p.answers, 1);
    p.run("epoch-3", 300).unwrap();
    assert_eq!(p.answers, 1);
    assert!(p.durable.borrow().cards.is_empty());
    assert_eq!(
        p.durable.borrow().audits.values().next().unwrap(),
        "approved"
    );
}
#[test]
fn a_revocation_while_waiting_finishes_the_reservation_without_delivery() {
    let mut p = Fake::new();
    p.busy = true;
    p.run("epoch-1", 100).unwrap();
    p.facts.team.settings.delegations.confirm_trust = false;
    p.busy = false;
    p.run("epoch-2", 200).unwrap();
    assert_eq!(p.answers, 0);
    assert_eq!(p.durable.borrow().cards.len(), 1);
    p.run("epoch-3", 300).unwrap();
    assert_eq!(p.answers, 0);
    assert_eq!(p.durable.borrow().cards.len(), 1);
}

// zerus-63rru.73: only a folder-trust prompt of the member's own agent is a
// candidate; every other pending question is the person's and plans nothing.

const CONVERSATION: &str = "conversation-1";

/// The member at its startup folder-trust prompt, as `agent`'s producer
/// writes it (`state::trust_fixture`), in a run with no conversation yet.
fn at_startup(f: &mut Fake, agent: &str) {
    let card = crate::state::trust_fixture::startup_card(agent);
    let folder = crate::state::trust_fixture::FOLDER;
    f.binding.agent = agent.into();
    f.binding.folder = folder.into();
    f.binding.run = card["run_id"].as_str().unwrap().into();
    f.binding.conversation = String::new();
    f.facts.member.agent = agent.into();
    f.facts.member.worktree = folder.into();
    f.facts.binding = f.binding.clone();
    f.owner.folder = folder.into();
    f.snapshot.agent = agent.into();
    f.snapshot.folder = folder.into();
    f.snapshot.run = f.binding.run.clone();
    f.snapshot.conversation = String::new();
    f.snapshot.questions = vec![card];
}

/// The same run once its conversation started, with `questions` pending.
fn asking(f: &mut Fake, questions: Vec<Value>) {
    f.snapshot.conversation = CONVERSATION.into();
    f.snapshot.questions = questions;
}

/// Nothing was planned: no row, no card, no audit reservation, no answer.
fn nothing(f: &Fake, what: &str) {
    let db = f.durable.borrow();
    assert!(db.rows.is_empty(), "{what}: a row was claimed");
    assert!(db.cards.is_empty(), "{what}: a card was opened");
    assert!(db.audits.is_empty(), "{what}: an audit was reserved");
    assert_eq!(f.answers, 0, "{what}");
}

/// The questions a member's standing confirmation never answers. Codex's
/// hooks-trust prompt is one of them for every agent but Codex: a Codex
/// member's own hooks-trust prompt joined the confirmation (zerus-f67qp).
fn others(agent: &str) -> Vec<(&'static str, Value)> {
    use crate::state::trust_fixture as produced;
    let mut others = vec![
        (
            "the agent's own question",
            produced::ordinary_question(agent, CONVERSATION),
        ),
        (
            "a permission prompt",
            produced::permission_prompt(CONVERSATION),
        ),
    ];
    if agent != "codex" {
        others.push(("Codex's hooks-trust prompt", produced::hooks_trust_prompt()));
    }
    others
}

#[test]
fn a_question_that_is_not_a_folder_trust_prompt_plans_nothing() {
    for agent in ["claude", "codex", "kimi"] {
        for (what, question) in others(agent) {
            let mut f = Fake::new();
            at_startup(&mut f, agent);
            asking(&mut f, vec![question]);
            f.run("epoch-1", 100).unwrap();
            f.run("epoch-2", 200).unwrap();
            nothing(&f, &format!("{agent}: {what}"));
        }
    }
}

/// The repository card's root guard, driven by the real producer on real
/// Git layouts (zerus-63rru.66 round 2): the producer emits the note and its
/// root on one line; the named root is required, compared canonicalized with
/// the main checkout Git resolves, and only the member's worktree's main
/// checkout or a symlink spelling of it is accepted. A sibling checkout, a
/// different repository, a bare repository and a submodule are refused.
#[test]
fn a_codex_repository_card_answers_only_the_main_checkout_the_producer_names() {
    let temp = tempfile::tempdir().unwrap();
    let run = |args: &[&str], at: &std::path::Path| -> std::process::Output {
        std::process::Command::new("git")
            .arg("-C")
            .arg(at)
            .args(["-c", "user.email=review@example.invalid", "-c", "user.name=review"])
            .args(["-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false"])
            .args(args)
            .output()
            .unwrap()
    };
    let init = |dir: &std::path::Path| {
        std::fs::create_dir_all(dir).unwrap();
        run(&["init", "-q"], dir);
        std::fs::write(dir.join("file.txt"), "fixture").unwrap();
        run(&["add", "file.txt"], dir);
        run(&["commit", "-qm", "fixture"], dir);
    };
    let main = temp.path().join("repo");
    init(&main);
    run(&["worktree", "add", "-q", "--detach", "../member-wt"], &main);
    let worktree = temp.path().join("member-wt");
    // A sibling checkout of the same repository.
    let sibling = temp.path().join("sibling");
    init(&sibling);
    // A bare repository.
    let bare = temp.path().join("bare.git");
    std::fs::create_dir_all(&bare).unwrap();
    run(&["init", "-q", "--bare"], &bare);
    // A different repository and an initialized submodule of it.
    let other = temp.path().join("other");
    init(&other);
    run(&["-c", "protocol.file.allow=always", "submodule", "add", "-q", other.to_str().unwrap(), "sub"], &main);
    run(&["commit", "-qm", "submodule"], &main);
    let submodule_git = main.join(".git/modules/sub");
    // A symlink spelling of the main checkout.
    let spelling = temp.path().join("spelling");
    std::os::unix::fs::symlink(&main, &spelling).unwrap();
    // The producer's own card for each layout, from the real capture's
    // screen shape: the note and its root are the capture's own lines of
    // one paragraph, which the producer joins into one disclosure line.
    let screen = |root: &str| {
        include_str!("../../../../../tests/fixtures/codex-folder-trust-git-root-0167.txt")
            .replace("/work/repository/subdir", &worktree.display().to_string())
            .replace("/work/repository", root)
    };
    let mut record = crate::state::trust_fixture::record("codex");
    record["launch_dir"] = json!(worktree.to_str().unwrap());
    let card = |root: &str| {
        let screen = screen(root);
        crate::state::trust_fixture::codex_repository_card(&record, &screen)
    };
    let main_str = main.to_str().unwrap();
    for (root, answered) in [
        (main_str, true),
        (spelling.to_str().unwrap(), true),
        (sibling.to_str().unwrap(), false),
        (other.to_str().unwrap(), false),
        (bare.to_str().unwrap(), false),
        (submodule_git.to_str().unwrap(), false),
    ] {
        let produced = card(root);
        let mut f = Fake::new();
        at_startup(&mut f, "codex");
        f.binding.folder = worktree.to_str().unwrap().to_owned();
        f.snapshot.folder = f.binding.folder.clone();
        f.owner.folder = f.binding.folder.clone();
        f.facts.member.worktree = f.binding.folder.clone();
        f.facts.binding = f.binding.clone();
        f.snapshot.questions[0] = produced.unwrap_or_else(|| {
            panic!("the producer wrote no card for the root {root}");
        });
        let why = super::super::policy::trust(&f.binding, &f.snapshot).err();
        assert_eq!(why.is_none(), answered, "root {root}: {why:?}");
    }
}


/// A folder card without a disclosure is refused for every agent
/// (zerus-63rru.66 round 3, the restored shared guard's committed oracle).
#[test]
fn a_folder_card_without_a_disclosure_is_refused_for_every_agent() {
    for agent in ["claude", "codex", "kimi"] {
        let mut f = Fake::new();
        at_startup(&mut f, agent);
        let folder = crate::state::trust_fixture::FOLDER;
        let q = &mut f.snapshot.questions[0];
        q["questions"][0]["body"] = json!(format!("{folder}\n\n"));
        let why = super::super::policy::trust(&f.binding, &f.snapshot).err();
        assert_eq!(why.as_deref(), Some("The prompt carries no disclosure"), "{agent}");
    }
}

/// The repository card is Codex's own: Claude's policy refuses it
/// (zerus-63rru.66).
#[test]
fn a_repository_card_for_claude_is_refused() {
    let mut f = Fake::new();
    at_startup(&mut f, "claude");
    let folder = crate::state::trust_fixture::FOLDER;
    let q = &mut f.snapshot.questions[0];
    let item = &mut q["questions"][0];
    item["question"] = json!("Trust this Git repository?");
    item["body"] = json!(format!("{folder}\n\nNote: repository root:\n{folder}"));
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.answers, 0, "Claude keeps its own card");
}

#[test]
fn a_startup_trust_prompt_is_answered_as_before_and_a_later_question_plans_nothing() {
    for agent in ["claude", "codex", "kimi"] {
        let mut f = Fake::new();
        at_startup(&mut f, agent);
        f.run("epoch-1", 100).unwrap();
        assert_eq!(f.answers, 1, "{agent}: the startup prompt is answered");
        let before = (
            f.durable.borrow().rows.len(),
            f.durable.borrow().cards.len(),
            f.durable.borrow().audits.clone(),
        );
        // After the grant, in the same run: the agent asks the person.
        for (what, question) in others(agent) {
            asking(&mut f, vec![question]);
            f.run("epoch-1", 200).unwrap();
            f.run("epoch-2", 300).unwrap();
            let after = (
                f.durable.borrow().rows.len(),
                f.durable.borrow().cards.len(),
                f.durable.borrow().audits.clone(),
            );
            assert_eq!(after, before, "{agent}: {what} after the grant");
            assert_eq!(f.answers, 1, "{agent}: {what}");
        }
    }
}

#[test]
fn a_trust_prompt_beside_another_question_goes_to_the_person_and_two_others_plan_nothing() {
    // The trust prompt is the candidate; the policy wants it alone.
    let mut f = Fake::new();
    at_startup(&mut f, "claude");
    let ordinary = crate::state::trust_fixture::ordinary_question("claude", "");
    f.snapshot.questions.push(ordinary);
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    // Two questions, neither a folder-trust prompt.
    let mut f = Fake::new();
    at_startup(&mut f, "claude");
    asking(
        &mut f,
        others("claude").into_iter().map(|(_, q)| q).collect(),
    );
    f.run("epoch-1", 100).unwrap();
    nothing(&f, "two other questions");
}

#[test]
fn with_the_delegation_off_a_trust_prompt_keeps_its_notice_and_another_question_plans_nothing() {
    let mut f = Fake::new();
    f.facts.team.settings.delegations.confirm_trust = false;
    at_startup(&mut f, "claude");
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    let mut f = Fake::new();
    f.facts.team.settings.delegations.confirm_trust = false;
    at_startup(&mut f, "claude");
    asking(
        &mut f,
        vec![crate::state::trust_fixture::ordinary_question(
            "claude",
            CONVERSATION,
        )],
    );
    f.run("epoch-1", 100).unwrap();
    nothing(&f, "delegation off");
}

/// A team that already holds a notice row keyed "trust" for a run, claimed
/// before this change for an ordinary question, meets the new code on its
/// next pass: the row is settled history. Another question plans nothing
/// and adds no card; the row and its card stay as they were.
#[test]
fn an_earlier_trust_notice_row_for_an_ordinary_question_stays_history() {
    let mut f = Fake::new();
    at_startup(&mut f, "claude");
    asking(
        &mut f,
        vec![crate::state::trust_fixture::ordinary_question(
            "claude",
            CONVERSATION,
        )],
    );
    // The row the earlier code made, by its own path: a Trust plan for that
    // question, refused by the policy, settled with its notice card.
    let question = f.snapshot.questions[0].clone();
    let plan = candidate(&f.binding, &f.snapshot, Kind::Trust, question);
    execute(&mut f, plan, "epoch-0", 50).unwrap();
    let rows = f.durable.borrow().rows.clone();
    let cards = f.durable.borrow().cards.clone();
    assert_eq!((rows.len(), cards.len()), (1, 1));
    assert!(rows.values().all(|row| row.state == "cancelled"));
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.durable.borrow().rows, rows);
    assert_eq!(f.durable.borrow().cards, cards);
    assert_eq!(f.answers, 0);
}

/// What an earlier row still does: its "trust" key belongs to the run. If the
/// earlier code claimed it for another startup prompt (here Codex's
/// hooks-trust prompt) and the same run then shows its folder-trust prompt,
/// that prompt meets the settled row: no answer and no new card. It stays
/// with the person, as it did before this change. Only a run caught at
/// startup across the update can hold such a row.
#[test]
fn an_earlier_trust_row_of_the_same_run_leaves_its_folder_trust_prompt_to_the_person() {
    let mut f = Fake::new();
    at_startup(&mut f, "codex");
    let prompt = f.snapshot.questions[0].clone();
    f.snapshot.questions = vec![crate::state::trust_fixture::hooks_trust_prompt()];
    let question = f.snapshot.questions[0].clone();
    // The earlier code's row: the run's "trust" key, and the policy of that
    // code refused the hooks prompt, so the row is its notice.
    let mut plan = candidate(&f.binding, &f.snapshot, Kind::Trust, question);
    plan.key = key(&json!([
        f.binding.team,
        f.binding.member,
        f.binding.launch,
        f.snapshot.run,
        "trust"
    ]));
    execute_refusal(
        &mut f,
        plan,
        "epoch-0",
        50,
        Some("This prompt needs a person".into()),
    )
    .unwrap();
    let rows = f.durable.borrow().rows.clone();
    let cards = f.durable.borrow().cards.clone();
    assert_eq!((rows.len(), cards.len()), (1, 1));
    f.snapshot.questions = vec![prompt];
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.durable.borrow().rows, rows);
    assert_eq!(f.durable.borrow().cards, cards);
    assert_eq!(f.answers, 0);
    // The member's next run has its own key: its prompt is answered as before,
    // beside the earlier row, which stays as it was.
    let run = "run-after-the-update";
    f.binding.run = run.into();
    f.facts.binding = f.binding.clone();
    f.snapshot.run = run.into();
    f.snapshot.questions[0]["run_id"] = json!(run);
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1);
    assert_eq!(f.durable.borrow().rows.len(), 2);
    let earlier = rows.keys().next().unwrap();
    assert_eq!(f.durable.borrow().rows[earlier], rows[earlier]);
}

/// An exited member that still shows a pending question that is not a
/// folder-trust prompt plans nothing: neither a trust notice nor a resume
/// (which a pending question holds back anyway).
#[test]
fn an_exited_member_with_another_question_plans_nothing() {
    let mut f = Fake::new();
    f.exited(1);
    f.snapshot.questions = vec![crate::state::trust_fixture::ordinary_question(
        "claude",
        "conversation-1",
    )];
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.resumes, 0);
    nothing(&f, "an exited member's other question");
}

/// A Codex member at Codex's hooks-trust prompt, as its producer writes it
/// (`state::trust_fixture`), in its worktree.
fn at_hooks_trust(f: &mut Fake, card: Value, conversation: &str) {
    at_startup(f, "codex");
    f.binding.run = card["run_id"].as_str().unwrap().into();
    f.binding.conversation = conversation.into();
    f.facts.binding = f.binding.clone();
    f.snapshot.run = f.binding.run.clone();
    f.snapshot.conversation = conversation.into();
    f.snapshot.questions = vec![card];
}

/// The member resumed in its saved conversation meets Codex's hooks-trust
/// prompt before its SessionStart, as after an update changed its hook
/// definitions (zerus-f67qp).
fn at_resumed_hooks_trust(f: &mut Fake) {
    let card = crate::state::trust_fixture::resumed_hooks_trust_prompt(CONVERSATION);
    at_hooks_trust(f, card, CONVERSATION);
}

/// The producer's card for the resumed run at `screen`.
fn resumed_hooks_card(screen: &str) -> Option<Value> {
    use crate::state::trust_fixture as produced;
    produced::hooks_trust_card(&produced::resumed_record("codex", CONVERSATION), screen)
}

#[test]
fn a_codex_hooks_trust_prompt_at_a_resumed_startup_is_answered_once() {
    let mut f = Fake::new();
    at_resumed_hooks_trust(&mut f);
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.answers, 1);
    assert!(f.durable.borrow().cards.is_empty());
    let rows = f.durable.borrow().rows.clone();
    assert_eq!(rows.len(), 1);
    let row = rows.values().next().unwrap();
    assert_eq!(row.state, "approved");
    assert_eq!(row.plan.kind, Kind::Trust);
    assert_eq!(row.plan.question, f.snapshot.questions[0]);
    assert!(f
        .durable
        .borrow()
        .audits
        .values()
        .all(|state| state == "approved"));
    assert_eq!(
        f.durable.borrow().notes,
        [format!(
            "Zerus confirmed Codex hook trust for member {}: 4 hooks are new or changed.",
            f.binding.member
        )]
    );
    // The prompt is still on screen at the next pass: never answered again.
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1);
    assert_eq!(f.durable.borrow().rows.len(), 1);
    assert_eq!(f.durable.borrow().notes.len(), 1, "one feed line");
}

#[test]
fn a_crash_after_the_feed_line_commits_writes_it_once_more_under_its_request_and_never_answers_again(
) {
    let mut f = Fake::new();
    at_resumed_hooks_trust(&mut f);
    f.crash_after_card = true;
    assert!(f.run("epoch-1", 100).is_err());
    assert_eq!(f.answers, 1);
    let row = f.durable.borrow().rows.values().next().unwrap().clone();
    assert_eq!(
        (row.state.as_str(), row.card_done),
        ("approved", false),
        "the note is still owed"
    );
    f.crash_after_card = false;
    f.snapshot.questions.clear();
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1);
    // The real port replays the saved request (owner_note); the fake counts calls.
    assert_eq!(f.durable.borrow().notes.len(), 2);
    assert!(f.durable.borrow().rows.values().all(|row| row.card_done));
}

#[test]
fn a_codex_hooks_trust_prompt_after_its_folder_trust_in_the_same_run_is_answered() {
    let mut f = Fake::new();
    at_startup(&mut f, "codex");
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.answers, 1, "the folder trust");
    // The same new run, still before its SessionStart, then shows its hooks.
    f.snapshot.questions = vec![crate::state::trust_fixture::hooks_trust_prompt()];
    f.run("epoch-1", 200).unwrap();
    f.run("epoch-2", 300).unwrap();
    assert_eq!(f.answers, 2, "the hooks trust has its own key");
    assert_eq!(f.durable.borrow().rows.len(), 2);
    assert!(f.durable.borrow().cards.is_empty());
    assert_eq!(
        f.durable.borrow().notes.len(),
        1,
        "only the hooks trust has a feed line"
    );
}

#[test]
fn with_the_delegation_off_a_hooks_trust_prompt_is_the_persons() {
    let mut f = Fake::new();
    f.facts.team.settings.delegations.confirm_trust = false;
    at_resumed_hooks_trust(&mut f);
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
    assert!(f
        .durable
        .borrow()
        .rows
        .values()
        .all(|row| row.plan.kind == Kind::Notice));
    assert!(f.durable.borrow().notes.is_empty());
    assert!(f
        .durable
        .borrow()
        .cards
        .values()
        .all(|text| text == "Trust delegation is off"));
}

#[test]
fn a_codex_hooks_trust_card_of_another_agents_member_plans_nothing() {
    for agent in ["claude", "kimi"] {
        let mut f = Fake::new();
        at_resumed_hooks_trust(&mut f);
        f.binding.agent = agent.into();
        f.facts.member.agent = agent.into();
        f.facts.binding = f.binding.clone();
        f.snapshot.agent = agent.into();
        f.run("epoch-1", 100).unwrap();
        f.run("epoch-2", 200).unwrap();
        nothing(&f, agent);
    }
}

#[test]
fn an_unknown_hooks_trust_delivery_is_never_repeated_and_a_changed_hash_is_a_new_question() {
    let mut f = Fake::new();
    at_resumed_hooks_trust(&mut f);
    f.unknown = true;
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    f.unknown = false;
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1, "an unknown delivery is never replayed");
    assert_eq!(f.durable.borrow().cards.len(), 1);
    assert!(
        f.durable.borrow().notes.is_empty(),
        "an unknown delivery has no feed line"
    );
    // Codex shows another panel in the same run: another hook count is
    // another hash, a new question, answered once.
    let screen = crate::state::trust_fixture::HOOKS_TRUST_SCREEN.replace("4 hooks", "5 hooks");
    let card = resumed_hooks_card(&screen).unwrap();
    assert_ne!(
        card["question_hash"],
        f.snapshot.questions[0]["question_hash"]
    );
    f.snapshot.questions = vec![card];
    f.run("epoch-4", 400).unwrap();
    f.run("epoch-5", 500).unwrap();
    assert_eq!(f.answers, 2);
    assert_eq!(f.durable.borrow().rows.len(), 2);
}

/// A stale hash: the prompt changed between the read and the answer. The
/// read's plan is never delivered and goes to the person; the changed prompt
/// is a new question with its own key, answered once.
#[test]
fn a_hooks_trust_hash_changed_between_read_and_answer_is_never_delivered_and_the_new_prompt_is_its_own_question(
) {
    let mut f = Fake::new();
    at_resumed_hooks_trust(&mut f);
    let read = f.snapshot.questions[0]["question_hash"].clone();
    f.before_original = Some(|f| f.snapshot.questions[0]["question_hash"] = json!("cd".repeat(32)));
    f.run("epoch-1", 100).unwrap();
    assert_eq!(f.answers, 0, "the stale plan is never delivered");
    assert_eq!(f.durable.borrow().cards.len(), 1);
    let rows = f.durable.borrow().rows.clone();
    assert_eq!(rows.len(), 1);
    let stale = rows.values().next().unwrap();
    assert_eq!(
        (stale.state.as_str(), &stale.plan.question["question_hash"]),
        ("cancelled", &read)
    );
    f.run("epoch-2", 200).unwrap();
    f.run("epoch-3", 300).unwrap();
    assert_eq!(f.answers, 1, "the new prompt is answered once");
    assert_eq!(f.durable.borrow().rows.len(), 2);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}

#[test]
fn an_unfamiliar_hooks_trust_disclosure_goes_to_the_person() {
    let screen = crate::state::trust_fixture::HOOKS_TRUST_SCREEN.replace(
        "Hooks can run outside the sandbox after you trust them.\n",
        "Hooks can run outside the sandbox after you trust them.\nFailed to trust hooks: hook configuration changed\n",
    );
    let card = resumed_hooks_card(&screen).expect("the producer shows the error verbatim");
    let mut f = Fake::new();
    at_hooks_trust(&mut f, card, CONVERSATION);
    f.run("epoch-1", 100).unwrap();
    f.run("epoch-2", 200).unwrap();
    assert_eq!(f.answers, 0);
    assert_eq!(f.durable.borrow().cards.len(), 1);
}
