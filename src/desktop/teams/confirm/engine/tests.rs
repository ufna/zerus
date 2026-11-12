use super::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Default)]
struct Durable {
    rows: BTreeMap<String, Record>,
    audits: BTreeMap<String, String>,
    cards: BTreeMap<String, String>,
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

#[test]
fn all_manual_prompts_and_inactive_teams_make_one_card_without_an_answer() {
    for field in [
        "ordinary",
        "permission",
        "sign_in",
        "hooks",
        "two",
        "folder",
        "team_paused",
        "team_stopped",
        "trust_off",
        "same_folder_other_session",
    ] {
        let mut f = Fake::new();
        match field {
            "ordinary" => f.snapshot.questions[0]["source"] = json!("ordinary_question"),
            "permission" => f.snapshot.questions[0]["source"] = json!("claude_tool_approval"),
            "sign_in" => f.snapshot.questions[0]["source"] = json!("sign_in"),
            "hooks" => f.snapshot.questions[0]["source"] = json!("codex_hooks_trust"),
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
            _ => |f| {
                f.snapshot
                    .questions
                    .push(json!({"source":"ordinary_question"}))
            },
        });
        f.run("epoch-2", 200).unwrap();
        f.run("epoch-3", 300).unwrap();
        assert_eq!(f.briefs, 0, "{field}");
        assert_eq!(
            f.durable.borrow().cards.len(),
            if field == "questions" { 2 } else { 1 },
            "{field}"
        );
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
