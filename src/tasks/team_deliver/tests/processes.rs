//! Private CLI children with a fake native port. The fence, task attempt,
//! exact actor and receipt settlement are production code.
use super::*;
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Fake {
    root: PathBuf,
    mode: String,
}
impl Delivery for Fake {
    fn before_reserve(&self) {
        if std::env::var_os("HGS_TEAM_DELIVER_BARRIER").is_none() {
            return;
        }
        std::fs::write(
            self.root.join(format!("ready-{}", std::process::id())),
            b"ready",
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        while !self.root.join("release").exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(self.root.join("release").exists());
    }
    fn deliver(
        &self,
        config: &Config,
        task: &TaskRef,
        body: &Input,
        to: &SessionKey,
        request: &Value,
    ) -> attempts::Delivered {
        if self.mode == "crash-reserved" {
            std::process::exit(73);
        }
        let mut count = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("submissions"))
            .unwrap();
        writeln!(count, "{}", body.id).unwrap();
        std::fs::write(
            self.root.join(format!("request-{}.json", body.id)),
            request.to_string(),
        )
        .unwrap();
        match self.mode.as_str() {
            "uncertain" => attempts::Delivered::Uncertain("fixture unknown native result".into()),
            "refused" => {
                let store = crate::tasks::store(config, &task.project).unwrap();
                let mut document = store.load(&task.uuid).unwrap();
                let actor = document.attempt(&body.id).unwrap().unwrap().actor;
                document
                    .settle_attempt(
                        &body.id,
                        zerus_replica::Settlement::NotAdmitted,
                        &actor,
                        now(),
                    )
                    .unwrap();
                store.save(&mut document).unwrap();
                attempts::Delivered::NotAdmitted("fixture closed before input".into())
            }
            _ => {
                let receipt = json!({"request_id":body.id,"name":to.session,"run_id":request["expected_run_id"],"conversation_id":request["expected_conversation_id"],"first_message":false,"digest":attempts::digest(request),"status":"submitted"});
                std::fs::create_dir_all(self.root.join("input_receipts")).unwrap();
                std::fs::write(
                    self.root
                        .join("input_receipts")
                        .join(format!("{}.json", body.id)),
                    receipt.to_string(),
                )
                .unwrap();
                if self.mode == "crash-receipt" {
                    std::process::exit(74);
                }
                attempts::settle(config, &task.project, &task.uuid, &body.id, &receipt).unwrap();
                if self.mode == "crash-settled" {
                    std::process::exit(75);
                }
                attempts::Delivered::Submitted(receipt)
            }
        }
    }
}

#[test]
fn entry() {
    let Some(output) = std::env::var_os("HGS_TEAM_DELIVER_REPLY") else {
        return;
    };
    let config = Config::load().unwrap();
    let mode = std::env::var("HGS_TEAM_DELIVER_MODE").unwrap();
    let result = if mode == "init" {
        crate::swarm::dispatch(&config, &["initialize".into()], false).map(|_| json!({}))
    } else {
        let args: Vec<String> =
            serde_json::from_str(&std::env::var("HGS_TEAM_DELIVER_ARGS").unwrap()).unwrap();
        if mode == "save-error" {
            std::env::set_var("HGS_TEST_TEAM_DELIVERY_SAVE_ERROR", "1");
        }
        if mode == "agent" {
            use sha2::{Digest, Sha256};
            let name = std::env::var("HGS_SESSION").unwrap();
            let path = config
                .state
                .join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
            let mut record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            record["pid"] = json!(std::process::id());
            record["process_start"] =
                json!(crate::platform::process_start_time(std::process::id()).unwrap());
            std::fs::write(path, record.to_string()).unwrap();
            crate::tasks::execute(&config, &args)
        } else if matches!(
            mode.as_str(),
            "team-done" | "team-clear" | "crash-team-done" | "crash-team-clear"
        ) {
            if mode.starts_with("crash-") {
                std::env::set_var("HGS_TEST_TEAM_COMPLETION_AFTER_SAVE", "1");
            }
            completion::execute(&config, mode.strip_prefix("crash-").unwrap_or(&mode), &args)
        } else {
            execute(
                &config,
                &args,
                &Fake {
                    root: config.state.clone(),
                    mode,
                },
            )
        }
    };
    let reply = match result {
        Ok(reply) => json!({"ok":true,"reply":reply}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    std::fs::write(output, reply.to_string()).unwrap();
}

struct Fixture {
    h: Harness,
    config: Config,
    task: TaskRef,
    body: Input,
    lead: SessionKey,
}
impl Fixture {
    fn new() -> Self {
        let h = Harness::empty();
        let config = Config {
            home: h.temp.path().display().to_string(),
            dir: h.temp.path().join("config"),
            state: h.temp.path().to_owned(),
            selves: vec!["fixture".into()],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: BTreeMap::new(),
            tab: false,
            tab_colors: String::new(),
        };
        let dummy = TaskRef {
            project: "example-project".into(),
            uuid: String::new(),
        };
        let mut f = Self {
            h,
            config,
            task: dummy,
            body: Input {
                schema: 1,
                id: String::new(),
                team: TEAM.into(),
                node: 1,
                member: "m1".into(),
            },
            lead: SessionKey::default(),
        };
        let (child,reply)=f.spawn("init",&[],r##"{"projects":[{"id":"example-project","name":"example","color":"#123456","sessions":[],"folders":[]}]}"##,false);
        assert_eq!(f.finish(child, reply)["ok"], true);
        let home = crate::swarm::local_node_id(&f.config).unwrap();
        let mut create = ops::tests::create();
        if let ops::Operation::Create { home: machine, .. } = &mut create {
            *machine = home.clone();
        }
        f.h.run(&person(), create).unwrap();
        f.lead = f.bind("lead", &home);
        f.bind("m1", &home);
        {
            let tasks =
                ProjectStore::open(&f.config.state.join("replica"), "example-project").unwrap();
            let mut task = tasks
                .new_task(
                    "Implement the fixture change",
                    Stage::Todo,
                    Target::Any,
                    Actor::Person,
                    &Position::End,
                    "example",
                    now(),
                )
                .unwrap();
            f.task.uuid = task.uuid().into();
            tasks.save(&mut task).unwrap();
        }
        let reply = crate::teams::tasks::send(
            &crate::teams::tasks::Caller {
                state: &f.config.state,
                machine: &home,
                actor: Actor::Person,
            },
            &f.task,
            TEAM,
            None,
            None,
        )
        .unwrap();
        f.body.node = reply.node.unwrap();
        f.body.id = f.h.intent(f.body.node, "m1");
        f
    }
    fn bind(&mut self, member: &str, machine: &str) -> SessionKey {
        use sha2::{Digest, Sha256};
        let launch = uuid::Uuid::new_v4().to_string();
        let name = format!("codex/deliver-{member}");
        let key = SessionKey {
            machine: machine.into(),
            session: name.clone(),
            session_id: format!("launch_id:{launch}"),
        };
        let sample = ops::tests::fixture()
            .members
            .into_iter()
            .find(|m| m.id == member)
            .unwrap();
        self.h
            .run(
                &ops::tests::service(),
                ops::Operation::SetMemberSession {
                    member: member.into(),
                    session: zerus_team_plan::model::SessionKey {
                        machine: machine.into(),
                        session_id: key.session_id.clone(),
                    },
                    account: "fixture".into(),
                    worktree: sample.worktree,
                    ownership_nonce: uuid::Uuid::new_v4().to_string(),
                },
            )
            .unwrap();
        let record = json!({"version":1,"agent":"codex","name":name,"launch_id":launch,"run_id":format!("run-{member}"),"conversation_id":format!("conversation-{member}"),"activity":"idle"});
        std::fs::write(
            self.config
                .state
                .join(format!("{:x}.json", Sha256::digest(name.as_bytes()))),
            record.to_string(),
        )
        .unwrap();
        key
    }
    fn spawn(&self, mode: &str, args: &[String], input: &str, barrier: bool) -> (Child, PathBuf) {
        let reply = self
            .config
            .state
            .join(format!("reply-{}.json", uuid::Uuid::new_v4()));
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "tasks::team_deliver::tests::processes::entry",
            "--nocapture",
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &self.config.home)
        .env("HGS_CONFIG_DIR", &self.config.dir)
        .env("HGS_STATE_DIR", &self.config.state)
        .env("HGS_SELF", "fixture")
        .env("HGS_TEAM_DELIVER_MODE", mode)
        .env(
            "HGS_TEAM_DELIVER_ARGS",
            serde_json::to_string(args).unwrap(),
        )
        .env("HGS_TEAM_DELIVER_REPLY", &reply)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        if mode == "agent" {
            cmd.env("HGS_SESSION", &self.lead.session)
                .env("HGS_RUN_ID", "run-lead");
        }
        if barrier {
            cmd.env("HGS_TEAM_DELIVER_BARRIER", "1");
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        (child, reply)
    }
    fn finish(&self, child: Child, reply: PathBuf) -> Value {
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap()
    }
    fn args(&self, recover: bool) -> Vec<String> {
        let mut args = vec![
            self.task.uuid.clone(),
            "--project".into(),
            self.task.project.clone(),
            "--json".into(),
        ];
        if recover {
            args.push("--recover-only".into());
        }
        args
    }
    fn run(&self, mode: &str, recover: bool) -> Value {
        let (child, path) = self.spawn(
            mode,
            &self.args(recover),
            &serde_json::to_string(&self.body).unwrap(),
            false,
        );
        self.finish(child, path)
    }
    fn count(&self) -> usize {
        std::fs::read_to_string(self.config.state.join("submissions"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn sessions(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(&self.config.state)
            .unwrap()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                (name.len() == 69
                    && name.ends_with(".json")
                    && name[..64].bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (name, std::fs::read(entry.path()).unwrap()))
            })
            .collect()
    }
    fn document(&self) -> TaskDoc {
        ProjectStore::open(&self.config.state.join("replica"), &self.task.project)
            .unwrap()
            .load(&self.task.uuid)
            .unwrap()
    }
}

#[test]
fn handout_records_lead_and_context_then_every_repeat_only_reads() {
    for mode in ["submitted", "refused", "uncertain"] {
        let f = Fixture::new();
        let first = f.run(mode, false);
        assert_eq!(first["reply"]["state"], mode, "{first}");
        let attempt = f.document().attempt(&f.body.id).unwrap().unwrap();
        assert_eq!(attempt.actor, Actor::Agent(f.lead.clone()));
        let request: Value = serde_json::from_slice(
            &std::fs::read(f.config.state.join(format!("request-{}.json", f.body.id))).unwrap(),
        )
        .unwrap();
        assert!(request["text"].as_str().unwrap().contains("Team context"));
        for recover in [false, true] {
            let again = f.run("submitted", recover);
            assert_eq!(again["reply"]["state"], mode, "{again}");
        }
        assert_eq!(f.count(), 1);
    }
}

#[test]
fn two_processes_with_one_id_submit_exactly_once() {
    let f = Fixture::new();
    let input = serde_json::to_string(&f.body).unwrap();
    let first = f.spawn("submitted", &f.args(false), &input, true);
    let second = f.spawn("submitted", &f.args(false), &input, true);
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until {
        let ready = std::fs::read_dir(&f.config.state)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("ready-"))
            .count();
        if ready == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(f.config.state.join("release"), b"go").unwrap();
    let a = f.finish(first.0, first.1);
    let b = f.finish(second.0, second.1);
    assert!(
        a["reply"]["state"] == "submitted" || b["reply"]["state"] == "submitted",
        "{a} {b}"
    );
    assert_eq!(f.count(), 1, "{a} {b}");
}

#[test]
fn recovery_without_an_attempt_and_preview_off_never_submit() {
    let f = Fixture::new();
    let revision = f.document().revision();
    assert_eq!(f.run("submitted", true)["reply"]["state"], "uncertain");
    assert_eq!(f.document().revision(), revision);
    std::fs::remove_file(f.config.state.join("teams/preview")).unwrap();
    let (child, path) = f.spawn("submitted", &[], "not json", false);
    let off = f.finish(child, path);
    assert!(off["error"].as_str().unwrap().contains("teams_inactive"));
    assert_eq!(f.count(), 0);
}

#[test]
fn reopen_uses_the_recorded_reason_and_lead_once() {
    let mut f = Fixture::new();
    assert_eq!(f.run("submitted", false)["reply"]["state"], "submitted");
    f.h.run(
        &ops::tests::service(),
        ops::Operation::HandoutOutcome {
            effect_request_id: f.body.id.clone(),
            outcome: ops::HandoutOutcome::Submitted {
                receipt: "fixture".into(),
            },
        },
    )
    .unwrap();
    crate::teams::tasks::observe(&f.config.state, &f.task).unwrap();
    {
        let store = ProjectStore::open(&f.config.state.join("replica"), &f.task.project).unwrap();
        let mut task = store.load(&f.task.uuid).unwrap();
        task.report_attempt(
            &f.body.id,
            "Ready",
            zerus_replica::execution::ReportKind::Ready,
            None,
            &Actor::Person,
            now(),
        )
        .unwrap();
        store.save(&mut task).unwrap();
    }
    crate::teams::tasks::observe(&f.config.state, &f.task).unwrap();
    let generation = f.h.store.generation(TEAM, f.body.node).unwrap().unwrap();
    f.h.run(
        &ops::tests::service(),
        ops::Operation::RecordReport {
            node: f.body.node,
            generation,
            member: "m1".into(),
            text: "Ready".into(),
            commit: ops::tests::COMMIT.into(),
        },
    )
    .unwrap();
    let lead = ops::Actor::Session {
        person: "local".into(),
        team: TEAM.into(),
        member: "lead".into(),
        session: zerus_team_plan::model::SessionKey {
            machine: f.lead.machine.clone(),
            session_id: f.lead.session_id.clone(),
        },
    };
    f.h.run(
        &lead,
        ops::Operation::Return {
            node: f.body.node,
            reason: "Fix the missing edge case".into(),
        },
    )
    .unwrap();
    f.body.id = f.h.reopen(f.body.node, "m1");
    let before = f.h.store.team(TEAM).unwrap().unwrap().revision;
    let result = f.run("submitted", false);
    assert_eq!(result["reply"]["state"], "submitted", "{result}");
    let attempt = f.document().attempt(&f.body.id).unwrap().unwrap();
    assert_eq!(attempt.actor, Actor::Agent(f.lead.clone()));
    assert_eq!(attempt.origin, Origin::Reopen);
    let request: Value = serde_json::from_slice(
        &std::fs::read(f.config.state.join(format!("request-{}.json", f.body.id))).unwrap(),
    )
    .unwrap();
    let text = request["text"].as_str().unwrap();
    assert!(text.contains("Fix the missing edge case"));
    assert!(text.contains("Team context"));
    assert!(text.contains(&f.body.id));
    for recover in [false, true] {
        assert_eq!(f.run("submitted", recover)["reply"]["state"], "submitted");
    }
    assert_eq!(f.count(), 2);
    assert_eq!(f.h.store.team(TEAM).unwrap().unwrap().revision, before);
}

#[test]
fn every_preflight_refusal_leaves_the_task_and_native_session_untouched() {
    for change in 0..20 {
        let mut f = Fixture::new();
        let db = f.h.store.db.as_ref().unwrap();
        let mut args = f.args(false);
        match change {
            0 => f.body.id = uuid::Uuid::new_v4().to_string(),
            1 => f.body.team = uuid::Uuid::new_v4().to_string(),
            2 => f.body.node += 1,
            3 => f.body.member = "m2".into(),
            4 => {
                db.execute("UPDATE effects SET kind='other'", []).unwrap();
            }
            5 => {
                db.execute("UPDATE effects SET generation=generation+1", [])
                    .unwrap();
            }
            6 => {
                db.execute("UPDATE effects SET state='closed'", []).unwrap();
            }
            7 => {
                db.execute(
                    "UPDATE teams SET home=?1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            8 => {
                db.execute("UPDATE teams SET status='\"paused\"'", [])
                    .unwrap();
            }
            9 => {
                db.execute(
                    "UPDATE members SET data=json_remove(data,'$.session') WHERE id='m1'",
                    [],
                )
                .unwrap();
            }
            10 => {
                db.execute(
                    "UPDATE members SET data=json_set(data,'$.state','paused') WHERE id='m1'",
                    [],
                )
                .unwrap();
            }
            11 => args[0] = uuid::Uuid::new_v4().to_string(),
            12 => {
                db.execute(
                    "UPDATE live_tasks SET task_uuid=?1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            13 => {
                db.execute(
                    "UPDATE members SET data=json_remove(data,'$.session') WHERE id='lead'",
                    [],
                )
                .unwrap();
            }
            14 | 15 => {
                db.execute(
                    "UPDATE teams SET status=?1",
                    [format!(
                        "\"{}\"",
                        if change == 14 { "stopped" } else { "archived" }
                    )],
                )
                .unwrap();
            }
            16..=18 => {
                let store = crate::tasks::store(&f.config, &f.task.project).unwrap();
                let mut doc = store.load(&f.task.uuid).unwrap();
                let key = match change {
                    16 => None,
                    17 => Some(zerus_replica::TaskTeam {
                        team: uuid::Uuid::new_v4().to_string(),
                        home: f.lead.machine.clone(),
                    }),
                    _ => Some(zerus_replica::TaskTeam {
                        team: TEAM.into(),
                        home: uuid::Uuid::new_v4().to_string(),
                    }),
                };
                doc.set_team(key.as_ref(), &Actor::Person, now()).unwrap();
                store.save(&mut doc).unwrap();
            }
            19 => {
                db.execute(
                    "UPDATE node_identity SET task_uuid=?1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = f.document().revision();
        let sessions = f.sessions();
        let team = f.h.store.team(TEAM).unwrap().unwrap().revision;
        let (child, path) = f.spawn(
            "submitted",
            &args,
            &serde_json::to_string(&f.body).unwrap(),
            false,
        );
        let result = f.finish(child, path);
        assert_eq!(
            result["reply"]["state"], "refused",
            "mutation {change}: {result}"
        );
        let expected = match change {
            0 => "No recorded delivery effect",
            1 => "Team not found",
            2..=4 => "Delivery effect binding differs",
            5 | 6 => "Effect is closed",
            7 => "another computer",
            8 | 14 | 15 => "Team is not Running",
            9 | 10 | 13 => "no active local session binding",
            12 => "not this team's live node",
            16..=18 => "exact home marker",
            19 => "Node task identity differs",
            _ => "REF",
        };
        assert!(
            result["reply"]["error"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{change}: {result}"
        );
        assert_eq!(f.document().revision(), before, "mutation {change}");
        assert_eq!(f.sessions(), sessions, "mutation {change}");
        assert_eq!(f.h.store.team(TEAM).unwrap().unwrap().revision, team);
        assert_eq!(f.count(), 0);
    }
}

mod completion_tests;

#[test]
fn restarts_after_reservation_receipt_and_settlement_never_send_again() {
    for (mode, code, state, count) in [
        ("crash-reserved", 73, "uncertain", 0),
        ("crash-receipt", 74, "submitted", 1),
        ("crash-settled", 75, "submitted", 1),
    ] {
        let f = Fixture::new();
        let (child, reply) = f.spawn(
            mode,
            &f.args(false),
            &serde_json::to_string(&f.body).unwrap(),
            false,
        );
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!reply.exists());
        for recover in [true, false, true] {
            let out = f.run("submitted", recover);
            assert_eq!(out["reply"]["state"], state, "{mode}: {out}");
        }
        assert_eq!(f.count(), count);
    }
}

#[test]
fn admission_rechecks_the_effect_marker_stage_and_executor_after_freeze() {
    for change in 0..5 {
        let f = Fixture::new();
        let (child, path) = f.spawn(
            "submitted",
            &f.args(false),
            &serde_json::to_string(&f.body).unwrap(),
            true,
        );
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            if std::fs::read_dir(&f.config.state)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("ready-"))
            {
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        match change {
            0 => {
                f.h.store
                    .db
                    .as_ref()
                    .unwrap()
                    .execute("UPDATE teams SET status='\"paused\"'", [])
                    .unwrap();
            }
            1 => {
                f.h.store
                    .db
                    .as_ref()
                    .unwrap()
                    .execute("UPDATE effects SET state='superseded'", [])
                    .unwrap();
            }
            _ => {
                let store = crate::tasks::store(&f.config, &f.task.project).unwrap();
                let mut doc = store.load(&f.task.uuid).unwrap();
                if change == 2 {
                    doc.set_team(None, &Actor::Person, now()).unwrap();
                } else if change == 3 {
                    doc.record_start(&Actor::Agent(f.lead.clone()), now())
                        .unwrap();
                } else {
                    doc.record_send(&f.lead, "other-dispatch", &Actor::Person, now())
                        .unwrap();
                }
                store.save(&mut doc).unwrap();
            }
        }
        std::fs::write(f.config.state.join("release"), "go").unwrap();
        let out = f.finish(child, path);
        assert_eq!(out["reply"]["state"], "refused", "{change}: {out}");
        assert!(f.document().attempt(&f.body.id).unwrap().is_none());
        assert_eq!(f.count(), 0);
    }
}

#[test]
fn an_effect_cannot_authorize_a_different_valid_node_or_member() {
    for other_node in [false, true] {
        let mut f = Fixture::new();
        if other_node {
            let store = crate::tasks::store(&f.config, &f.task.project).unwrap();
            let mut doc = store
                .new_task(
                    "Another task",
                    Stage::Todo,
                    Target::Any,
                    Actor::Person,
                    &Position::End,
                    "example",
                    now(),
                )
                .unwrap();
            let reference = TaskRef {
                project: f.task.project.clone(),
                uuid: doc.uuid().into(),
            };
            store.save(&mut doc).unwrap();
            drop(store);
            let reply = crate::teams::tasks::send(
                &crate::teams::tasks::Caller {
                    state: &f.config.state,
                    machine: &f.lead.machine,
                    actor: Actor::Person,
                },
                &reference,
                TEAM,
                None,
                None,
            )
            .unwrap();
            f.task = reference;
            f.body.node = reply.node.unwrap();
        } else {
            let machine = f.lead.machine.clone();
            f.bind("m2", &machine);
            f.body.member = "m2".into();
        }
        let before = f.document().revision();
        let sessions = f.sessions();
        let out = f.run("submitted", false);
        assert_eq!(out["reply"]["state"], "refused", "{out}");
        assert_eq!(f.document().revision(), before);
        assert_eq!(f.sessions(), sessions);
        assert_eq!(f.count(), 0);
    }
}

#[test]
fn delivery_cutover_repeats_reserve_nothing_and_recovery_never_sends() {
    let f = Fixture::new();
    let store = crate::tasks::store(&f.config, &f.task.project).unwrap();
    let mut project = store.project().unwrap();
    project
        .set_cutover_hold(true, &Actor::Person, now())
        .unwrap();
    store.save_project(&project).unwrap();
    drop(store);
    let revision = f.document().revision();
    for _ in 0..2 {
        let out = f.run("submitted", false);
        assert_eq!(out["reply"]["state"], "cutting_over", "{out}");
    }
    assert_eq!(f.run("submitted", true)["reply"]["state"], "uncertain");
    assert_eq!(f.document().revision(), revision);
    assert_eq!(f.count(), 0);
}

#[test]
fn a_reused_session_caption_cannot_change_the_bound_destination() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let name = "codex/deliver-m1";
    let renamed = "codex/renamed-member";
    let original_path = f
        .config
        .state
        .join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
    let original: Value = serde_json::from_slice(&std::fs::read(&original_path).unwrap()).unwrap();
    let mut actual = original.clone();
    actual["name"] = json!(renamed);
    std::fs::write(
        f.config
            .state
            .join(format!("{:x}.json", Sha256::digest(renamed.as_bytes()))),
        actual.to_string(),
    )
    .unwrap();
    let mut impostor = original.clone();
    impostor["launch_id"] = json!(uuid::Uuid::new_v4().to_string());
    impostor["run_id"] = json!("impostor-run");
    std::fs::write(original_path, impostor.to_string()).unwrap();
    let out = f.run("submitted", false);
    assert_eq!(out["reply"]["state"], "submitted", "{out}");
    let attempt = f.document().attempt(&f.body.id).unwrap().unwrap();
    assert_eq!(attempt.executor.session, renamed);
    assert_eq!(
        attempt.executor.session_id,
        format!("launch_id:{}", original["launch_id"].as_str().unwrap())
    );
    let request: Value = serde_json::from_slice(
        &std::fs::read(f.config.state.join(format!("request-{}.json", f.body.id))).unwrap(),
    )
    .unwrap();
    assert_eq!(request["expected_run_id"], "run-m1");
    assert_eq!(f.count(), 1);
}

#[test]
fn a_save_error_after_the_attempt_is_durable_is_uncertain_and_never_sends() {
    let f = Fixture::new();
    let out = f.run("save-error", false);
    assert_eq!(out["reply"]["state"], "uncertain", "{out}");
    assert!(f.document().attempt(&f.body.id).unwrap().is_some());
    for recover in [false, true] {
        assert_eq!(f.run("submitted", recover)["reply"]["state"], "uncertain");
    }
    assert_eq!(f.count(), 0);
}
