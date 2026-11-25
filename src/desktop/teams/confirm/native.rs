//! Production effects use the native CLI, the original child custodian and
//! receipts read through the original held store. Reads confer no authority.
mod binding;
#[cfg(test)]
pub(super) use binding::launch_binding;
mod observation;
mod original;
mod ownership;
mod receipt;
use super::{
    ledger::Ledger,
    ports::{Ports, Result},
    types::*,
};
use crate::{config::Config, teams::store::Store};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) struct Native<'a> {
    pub config: &'a Config,
    pub machine: &'a str,
    pub stop: &'a Arc<AtomicBool>,
    pub ledger: Ledger,
    pub store: &'a mut Store,
    capture: Option<(Snapshot, Value)>,
    pub observation: Option<Result<()>>,
    pins: Vec<ownership::Pin>,
}
impl<'a> Native<'a> {
    pub fn new(
        config: &'a Config,
        machine: &'a str,
        stop: &'a Arc<AtomicBool>,
        store: &'a mut Store,
    ) -> Result<Self> {
        Ok(Self {
            config,
            machine,
            stop,
            ledger: Ledger::open(&config.state)?,
            store,
            capture: None,
            observation: None,
            pins: Vec::new(),
        })
    }
    pub fn reset(&mut self) {
        self.capture = None;
        self.observation = None;
        self.pins.clear();
    }
    fn read(&self, args: Vec<String>, input: Option<&[u8]>) -> Result<Value> {
        let mut scoped = vec!["--expect-machine".into(), self.machine.into()];
        scoped.extend(args);
        self.run(&scoped, input)
    }
    /// The send's effect-free observation of a resumed Codex member or a
    /// lead waiting for its first message, run as
    /// exactly the argv the CLI's own gate admits ([`observation`]).
    fn observe_send(&self, name: &str, input: &[u8]) -> Result<Value> {
        self.run(&observation(name), Some(input))
    }
    fn run(&self, args: &[String], input: Option<&[u8]>) -> Result<Value> {
        match super::super::child::run(
            self.config,
            args,
            input,
            None,
            false,
            Instant::now() + Duration::from_secs(10),
            self.stop,
        ) {
            super::super::child::Ended::Exited { code: 0, output } => {
                serde_json::from_slice(&output).map_err(|e| e.to_string())
            }
            _ => Err("The bounded member observation is unavailable".into()),
        }
    }
}
/// The argv of the send's effect-free observation (zerus-63rru.54): exactly
/// `send NAME --observe --json`, the only argv `cli::observe_gate` admits.
/// That gate runs before any machine guard and refuses every other argv with
/// `send` and `--observe`, a leading `--expect-machine` included, with exit
/// 2. No guard is lost: the view this observation follows was read under the
/// machine guard on this computer, and the gate refuses a remote name.
pub(super) fn observation(name: &str) -> Vec<String> {
    vec![
        "send".into(),
        name.into(),
        "--observe".into(),
        "--json".into(),
    ]
}
impl Ports for Native<'_> {
    fn facts(&mut self, b: &Binding) -> Result<Facts> {
        self.fresh_facts(b)
    }
    fn observe(&mut self, b: &Binding) -> Result<Snapshot> {
        self.snapshot(b)
    }
    fn observe_member(&mut self, b: &Binding) -> Result<Snapshot> {
        let result = self.snapshot(b);
        self.observation = Some(result.as_ref().map(|_| ()).map_err(String::clone));
        result
    }
    fn ownership(&mut self, b: &Binding) -> Result<Ownership> {
        self.owned(b)
    }
    fn unsettled(&mut self, b: &Binding) -> Result<Option<Record>> {
        self.ledger.unsettled(b)
    }
    fn pending_brief(&mut self, b: &Binding) -> Result<Option<Plan>> {
        let team = self
            .store
            .team(&b.team)
            .map_err(|e| e.to_string())?
            .ok_or("Team disappeared")?;
        if team.lead != b.member {
            return self.ledger.pending_brief(b);
        }
        if !self.ledger.lead_brief_needed(b)? {
            return Ok(None);
        }
        let snapshot = self.observe_member(b)?;
        let planning = self.store.planning(&team.id).map_err(|e| e.to_string())?;
        self.ledger
            .pending_lead_brief(b, &snapshot, &team, planning.as_ref())
    }
    fn claim(&mut self, p: &Plan, epoch: &str, now: i64) -> Result<Claim> {
        self.ledger.claim(p, epoch, now)
    }
    fn save(&mut self, row: &Record) -> Result<()> {
        self.ledger.save(row)
    }
    fn audit_begin(&mut self, plan: &Plan) -> Result<Audit> {
        // Before-start hook trust has its own synthetic audit writer.
        let kind = plan.audit_kind();
        // The scope is recorded only for a FolderTrust plan: it is the
        // extent of the trust grant, never of a resume or any other plan
        // (zerus-63rru.66 round 4).
        let mut payload = json!({"host":self.machine,"session":plan.snapshot.name,"provider":plan.binding.agent,"question":plan.question});
        if kind == crate::approval_log::TeamKind::FolderTrust {
            payload["scope"] = json!(super::policy::scope_for(&plan.binding.folder));
        }
        let mut db = crate::approval_log::open(&self.config.state.join("approval-audit"))
            .map_err(|e| e.to_string())?;
        let reserved =
            crate::approval_log::begin_team(&mut db, &payload, kind).map_err(|e| e.to_string())?;
        if reserved["eligible"] != true {
            return Err("The team audit refused this kind".into());
        }
        Ok(Audit {
            id: reserved["id"].as_i64().ok_or("Missing audit ID")?,
            request: reserved["request_id"]
                .as_str()
                .ok_or("Missing audit request")?
                .into(),
            answers: reserved["answers"].clone(),
            fresh: reserved["fresh"] == true,
        })
    }
    fn audit_finish(&mut self, row: &Record) -> Result<()> {
        let audit = row.audit.as_ref().ok_or("Missing audit")?;
        let db = crate::approval_log::open(&self.config.state.join("approval-audit"))
            .map_err(|e| e.to_string())?;
        crate::approval_log::finish(&db,&json!({"id":audit.id,"request_id":audit.request,"status":row.state,"detail":row.detail})).map_err(|e|e.to_string())?;
        Ok(())
    }
    fn card(&mut self, row: &Record) -> Result<()> {
        self.owner_card(row)
    }
    fn before_start(&mut self, plan: &Plan) {
        let outcome = self.pretrust(plan);
        if let Some(outcome) = outcome.filter(|outcome| {
            !matches!(
                outcome,
                super::hook_trust::Pretrust::Nothing
                    | super::hook_trust::Pretrust::Trusted(_)
                    | super::hook_trust::Pretrust::Repeated
            )
        }) {
            eprintln!(
                "hgs: Teams hook trust for member {}: {outcome:?}",
                plan.binding.member
            );
        }
    }
    fn hook_listing(&mut self, plan: &Plan) -> Result<String> {
        let b = &plan.binding;
        let base = self.fresh_facts(b)?.team.base_commit;
        Ok(super::hook_trust::listing(
            self.agent_home().as_deref(),
            &b.folder,
            &base,
        ))
    }
    fn notes_owed(&mut self, b: &Binding) -> Result<Vec<Note>> {
        self.ledger.notes_owed(b)
    }
    fn note(&mut self, note: &Note) -> Result<()> {
        self.owner_note(note)
    }
    fn original(
        &mut self,
        row: &Record,
        check: &mut dyn FnMut(&mut Self) -> Result<()>,
    ) -> Outcome {
        if let Err(why) = check(self) {
            return Outcome::NotStarted(why);
        }
        self.deliver(row)
    }
}
impl Native<'_> {
    fn owner_card(&mut self, row: &Record) -> Result<()> {
        use crate::teams::ops::{Operation, Request};
        self.store.refresh().map_err(|e| e.to_string())?;
        let team = self
            .store
            .team(&row.plan.binding.team)
            .map_err(|e| e.to_string())?
            .ok_or("Team disappeared")?;
        let request = if let Some(mut request) = self.ledger.card_request(&row.plan.key)? {
            // The saved request keeps its pre-change shape and stores no
            // parameters (zerus-63rru.34, F1); the member is derived here from
            // the standing row the request already names.
            if let Operation::OpenCard {
                parameters: current,
                ..
            } = &mut request.operation
            {
                if current.is_none() {
                    *current = Some(vec![(
                        "member".into(),
                        row.plan.binding.member.clone(),
                    )]);
                }
            }
            request
        } else {
            let request = Request {
                request_id: uuid::Uuid::new_v4().to_string(),
                team: team.id.clone(),
                expected_revision: team.revision,
                operation: Operation::OpenCard {
                    kind: "standing_confirmation".into(),
                    parameters: Some(vec![("member".into(), row.plan.binding.member.clone())]),
                    text: format!(
                        "Member {} needs your attention: {}",
                        row.plan.binding.member,
                        row.detail.chars().take(2048).collect::<String>()
                    ),
                    node: None,
                    // Nothing reads its decision: acknowledged alone.
                    decisions: None,
                },
            };
            self.ledger.save_card(&row.plan.key, Some(&request))?;
            request
        };
        self.service_write(row, &request)
    }
    /// A resumed Codex member's pre-trust, only under the team's trust
    /// setting and for the account the held observation's record names;
    /// `None` when nothing was tried.
    fn pretrust(&self, plan: &Plan) -> Option<super::hook_trust::Pretrust> {
        let b = &plan.binding;
        let facts = self.fresh_facts(b).ok()?;
        if !facts.team.settings.delegations.confirm_trust {
            return None;
        }
        let home = self.agent_home()?;
        Some(super::hook_trust::before_start(
            self.config,
            &super::hook_trust::Start {
                team: &b.team,
                member: &b.member,
                person: &b.person,
                machine: &b.machine,
                launch: &b.launch,
                session: &plan.snapshot.name,
                run: &plan.snapshot.run,
                conversation: &plan.snapshot.conversation,
                folder: &b.folder,
                base: &facts.team.base_commit,
                home: &home,
            },
        ))
    }
    /// The member's Codex account, from the held observation's record.
    fn agent_home(&self) -> Option<std::path::PathBuf> {
        self.capture
            .as_ref()
            .and_then(|(_, raw)| raw["agent_home"].as_str().filter(|home| !home.is_empty()))
            .map(std::path::PathBuf::from)
    }
    /// One owed feed line (zerus-f67qp): the service's journal row naming the
    /// member, under the line's saved request, so a retry after a crash
    /// replays that request and never writes a second. A definite refusal
    /// other than a stale revision cannot be written later either: the line
    /// is then no longer owed.
    fn owner_note(&mut self, note: &Note) -> Result<()> {
        use crate::teams::ops::{self, Actor, Operation, Request};
        self.store.refresh().map_err(|e| e.to_string())?;
        let team = self
            .store
            .team(&note.team)
            .map_err(|e| e.to_string())?
            .ok_or("Team disappeared")?;
        let request = if let Some(request) = self.ledger.note_request(&note.key)? {
            request
        } else {
            let request = Request {
                request_id: uuid::Uuid::new_v4().to_string(),
                team: team.id.clone(),
                expected_revision: team.revision,
                operation: Operation::NoteConfirmation {
                    member: note.member.clone(),
                    text: note.text.clone(),
                },
            };
            self.ledger.save_note_request(&note.key, Some(&request))?;
            request
        };
        let actor = Actor::Service {
            person: note.person.clone(),
            team: request.team.clone(),
        };
        match ops::apply(
            self.store,
            &actor,
            &request,
            chrono::Utc::now().timestamp_millis(),
        ) {
            Ok(_) => self.ledger.note_done(&note.key),
            Err(p) if p.code == zerus_team_plan::problem::code::STALE_REVISION => {
                self.ledger.save_note_request(&note.key, None)?;
                Err("Feed line revision changed; it is written next pass".into())
            }
            Err(p)
                if p.code == zerus_team_plan::problem::code::STORE_BUSY
                    || p.code == zerus_team_plan::problem::code::STORAGE_FAILED =>
            {
                Err(p.to_string())
            }
            Err(_) => self.ledger.note_done(&note.key),
        }
    }
    /// The service applies the row's saved request. A stale revision drops
    /// the saved request; the next pass writes a fresh one.
    fn service_write(&mut self, row: &Record, request: &crate::teams::ops::Request) -> Result<()> {
        use crate::teams::ops::{self, Actor};
        let actor = Actor::Service {
            person: row.plan.binding.person.clone(),
            team: request.team.clone(),
        };
        match ops::apply(
            self.store,
            &actor,
            request,
            chrono::Utc::now().timestamp_millis(),
        ) {
            Ok(_) => Ok(()),
            Err(p) if p.code == zerus_team_plan::problem::code::STALE_REVISION => {
                self.ledger.save_card(&row.plan.key, None)?;
                Err(
                    "Owner card revision changed; its definite refusal will be checked next pass"
                        .into(),
                )
            }
            Err(p) => Err(p.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_resume_plans_audit_row_carries_no_scope() {
        // zerus-63rru.66 round 4: the scope is recorded only for a
        // FolderTrust plan; a resume's row reads as before.
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            home: String::new(),
            dir: temp.path().join("config"),
            state: temp.path().join("state"),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        };
        crate::platform::private_dir(&config.state.join("teams")).unwrap();
        std::fs::write(config.state.join("teams/preview"), "on").unwrap();
        let mut store = Store::open(&config.state).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (b, _, mut s) = super::super::policy::tests::sample();
        s.ended = true;
        s.paused = false;
        s.self_exit = true;
        s.exit_identity = "exit".into();
        s.conversation = "conversation-1".into();
        s.questions.clear();
        let plan =
            super::super::engine::candidate(&b, &s, Kind::Resume, serde_json::Value::Null);
        let mut native = Native::new(&config, &b.machine, &stop, &mut store).unwrap();
        let audit = native.audit_begin(&plan).unwrap();
        let db = crate::approval_log::open(&config.state.join("approval-audit")).unwrap();
        let saved: String = db
            .query_row("SELECT question FROM attempts WHERE id=?1", [audit.id], |r| {
                r.get(0)
            })
            .unwrap();
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert!(saved.get("scope").is_none(), "{saved}");
    }

    /// A resumed Codex member's pre-trust runs only under the team's trust
    /// setting, for the account its observation names (zerus-f67qp): with it
    /// on, the pre-trust is tried (here an account whose folder cannot exist,
    /// so nothing is written anywhere); with it off, or no account, nothing.
    #[test]
    fn a_resumed_codex_members_pretrust_follows_the_teams_trust_setting() {
        use crate::teams::ops::tests::TEAM;
        let mut h = crate::desktop::teams::tests::bound151_fixture();
        let state = h.temp.path().to_owned();
        let config = Config {
            home: String::new(),
            dir: state.join("config"),
            state: state.clone(),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        };
        let (b, name) = launch_binding(&config, TEAM, "m1").unwrap();
        let (_, _, mut s) = super::super::policy::tests::sample();
        s.name = name;
        s.questions.clear();
        let plan = super::super::engine::candidate(&b, &s, Kind::Resume, serde_json::Value::Null);
        let blocker = state.join("not-a-folder");
        std::fs::write(&blocker, "file").unwrap();
        let held = |home: &std::path::Path| {
            Some((s.clone(), json!({"agent_home": home.to_str().unwrap()})))
        };
        let stop = Arc::new(AtomicBool::new(false));
        let mut native = Native::new(&config, &b.machine, &stop, &mut h.store).unwrap();
        assert_eq!(native.pretrust(&plan), None, "no observation, no account");
        native.capture = held(&blocker.join("codex-home"));
        assert!(matches!(
            native.pretrust(&plan),
            Some(super::super::hook_trust::Pretrust::Refused(_))
        ));
        drop(native);
        h.settings(|s| s.delegations.confirm_trust = false);
        let mut native = Native::new(&config, &b.machine, &stop, &mut h.store).unwrap();
        native.capture = held(&blocker.join("codex-home"));
        assert_eq!(native.pretrust(&plan), None);
    }

    /// A confirmed pre-trust's feed line through the store the product wrote
    /// (zerus-f67qp): owed by `hook_trust::owe_line`, written as one service
    /// journal row naming the member; a retry after a crash replays its
    /// saved request and writes no second.
    #[test]
    fn a_pretrusts_feed_line_is_one_service_journal_row() {
        use crate::teams::ops::tests::TEAM;
        let mut h = crate::desktop::teams::tests::bound151_fixture();
        let state = h.temp.path().to_owned();
        let config = Config {
            home: String::new(),
            dir: state.join("config"),
            state: state.clone(),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        };
        let (b, name) = launch_binding(&config, TEAM, "m1").unwrap();
        let home = state.join("codex-home");
        let start = super::super::hook_trust::Start {
            team: &b.team,
            member: &b.member,
            person: &b.person,
            machine: &b.machine,
            launch: &b.launch,
            session: &name,
            run: &b.run,
            conversation: &b.conversation,
            folder: &b.folder,
            base: "0123456789abcdef0123456789abcdef01234567",
            home: &home,
        };
        super::super::hook_trust::owe_line(&config, &start, "request-1", 16).unwrap();
        super::super::hook_trust::owe_line(&config, &start, "request-1", 16).unwrap();
        let before = h.store.journal(TEAM, 200, None).unwrap().rows.len();
        let stop = Arc::new(AtomicBool::new(false));
        let mut native = Native::new(&config, &b.machine, &stop, &mut h.store).unwrap();
        let owed = native.notes_owed(&b).unwrap();
        assert_eq!(owed.len(), 1);
        let note = owed[0].clone();
        // A crash after the journal row committed, before the line was marked
        // written: the retry replays the saved request.
        let request = {
            native.note(&note).unwrap();
            native.ledger.note_request(&note.key).unwrap().unwrap()
        };
        rusqlite::Connection::open(config.state.join("teams/standing-v1/journal.sqlite3"))
            .unwrap()
            .execute("UPDATE standing_lines SET done=0 WHERE key=?1", [&note.key])
            .unwrap();
        native.note(&note).unwrap();
        assert_eq!(
            native.ledger.note_request(&note.key).unwrap().unwrap(),
            request
        );
        assert!(native.notes_owed(&b).unwrap().is_empty());
        drop(native);
        let rows = h.store.journal(TEAM, 200, None).unwrap().rows;
        assert_eq!(rows.len(), before + 1, "one row for the replayed request");
        let line = rows.last().unwrap();
        assert_eq!(line.kind, "note_confirmation");
        assert_eq!(
            line.text,
            "Zerus trusted 16 of its own and the team's committed Codex hooks for member m1 before its agent started."
        );
        assert_eq!(
            serde_json::to_value(&line.actor).unwrap()["kind"],
            "service",
            "{:?}",
            line.actor
        );
    }

    /// Zerus never presses Trust all and continue (zerus-f67qp delta): a plan
    /// for Codex's hooks panel, as its producer writes the card, reserves no
    /// team audit, so no answer could be delivered for it.
    #[test]
    fn a_hooks_panels_plan_reserves_no_audit() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            home: String::new(),
            dir: temp.path().join("config"),
            state: temp.path().join("state"),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        };
        crate::platform::private_dir(&config.state.join("teams")).unwrap();
        std::fs::write(config.state.join("teams/preview"), "on").unwrap();
        let mut store = Store::open(&config.state).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (mut b, _, mut s) = super::super::policy::tests::sample();
        let card = crate::state::trust_fixture::resumed_hooks_trust_prompt("conversation-1");
        b.agent = "codex".into();
        b.run = card["run_id"].as_str().unwrap().into();
        b.conversation = "conversation-1".into();
        s.agent = "codex".into();
        s.run = b.run.clone();
        s.conversation = b.conversation.clone();
        s.questions = vec![card.clone()];
        let plan = super::super::engine::candidate(&b, &s, Kind::Trust, card);
        let mut native = Native::new(&config, &b.machine, &stop, &mut store).unwrap();
        assert_eq!(
            native.audit_begin(&plan).unwrap_err(),
            "The team audit refused this kind"
        );
        let db = crate::approval_log::open(&config.state.join("approval-audit")).unwrap();
        let reserved: i64 = db
            .query_row("SELECT count(*) FROM attempts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(reserved, 0);
    }

    #[test]
    fn team_audit_is_durable_before_delivery_and_finishes_only_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::canonical(temp.path()).unwrap();
        let config = Config {
            home: String::new(),
            dir: root.join("config"),
            state: root.join("state"),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        };
        crate::platform::private_dir(&config.state.join("teams")).unwrap();
        std::fs::write(config.state.join("teams/preview"), "on").unwrap();
        let mut store = Store::open(&config.state).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (mut b, _, s) = super::super::policy::tests::sample();
        // A real scratch worktree so the granted scope is the repository's
        // main checkout, an independent known value (zerus-63rru.66 round 2).
        let scratch = root.join("repo");
        std::fs::create_dir_all(&scratch).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "review@example.invalid"],
            vec!["config", "user.name", "review"],
        ] {
            assert!(std::process::Command::new("git").arg("-C").arg(&scratch).args(args).output().unwrap().status.success());
        }
        std::fs::write(scratch.join("file.txt"), "fixture").unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(&scratch).args(["add", "file.txt"]).output().unwrap().status.success());
        assert!(std::process::Command::new("git").arg("-C").arg(&scratch).args(["commit", "-qm", "fixture"]).output().unwrap().status.success());
        assert!(std::process::Command::new("git").arg("-C").arg(&scratch).args(["worktree", "add", "-q", "--detach", "../member-wt"]).output().unwrap().status.success());
        let worktree = root.join("member-wt");
        b.folder = worktree.to_str().unwrap().to_owned();
        b.launch = b.folder.clone();
        let mut native = Native::new(&config, &b.machine, &stop, &mut store).unwrap();
        let plan = super::super::engine::candidate(&b, &s, Kind::Trust, s.questions[0].clone());
        let audit = native.audit_begin(&plan).unwrap();
        assert!(audit.fresh);
        let db = crate::approval_log::open(&config.state.join("approval-audit")).unwrap();
        let saved: String = db
            .query_row("SELECT question FROM attempts WHERE id=?1", [audit.id], |r| {
                r.get(0)
            })
            .unwrap();
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        // The independent value: the scratch repository's main checkout,
        // the known path of the worktree the binding names.
        assert_eq!(saved["scope"], scratch.to_str().unwrap(), "{saved}");
        let status: String = db
            .query_row("SELECT status FROM attempts WHERE id=?1", [audit.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "prepared");
        let row = Record {
            plan,
            original: uuid::Uuid::new_v4().to_string(),
            epoch: "epoch-1".into(),
            state: "unknown".into(),
            audit: Some(audit),
            audit_done: false,
            result: None,
            detail: "Lost receipt".into(),
            card_request: uuid::Uuid::new_v4().to_string(),
            card_done: false,
            at: 100,
        };
        native.audit_finish(&row).unwrap();
        native.audit_finish(&row).unwrap();
        drop(native);
        let mut native = Native::new(&config, &b.machine, &stop, &mut store).unwrap();
        assert!(!native.audit_begin(&row.plan).unwrap().fresh);
        let status: String = db
            .query_row(
                "SELECT status FROM attempts WHERE id=?1",
                [row.audit.unwrap().id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "unknown");
    }
}
#[cfg(test)]
mod ownership_launch;
