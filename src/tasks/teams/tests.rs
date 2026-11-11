use super::super::*;
use crate::teams::ops::{
    self,
    tests::{self as fixtures, Harness, TEAM},
};
use std::io::Write;
use std::process::{Command, Stdio};

/// Run the real command entry with private configuration, stdin and caller
/// ancestry, without changing this test process's environment or sessions.
#[test]
fn cli_entry() {
    let Some(reply) = std::env::var_os("HGS_TEAMS_TEST_REPLY") else {
        return;
    };
    let config = Config::load().unwrap();
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("HGS_TEAMS_TEST_ARGS").unwrap()).unwrap();
    let result = if args[0] == "observe-team-fixture" {
        let ready = std::env::var_os("HGS_TEAM_OBSERVE_READY").unwrap();
        std::fs::write(ready, "ready").unwrap();
        crate::teams::tasks::observe(
            &config.state,
            &zerus_team_plan::model::TaskRef {
                project: args[1].clone(),
                uuid: args[2].clone(),
            },
        )
        .map(|_| json!({}))
        .map_err(|error| fail(error.message))
    } else if args[0] == "swarm" {
        crate::swarm::dispatch(&config, &args[1..], false).map(|_| json!({}))
    } else {
        execute(&config, &args)
    };
    let out = match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    std::fs::write(reply, out.to_string()).unwrap();
}

struct Fixture {
    h: Harness,
    machine: String,
}

impl Fixture {
    fn new() -> Self {
        let h = Harness::empty();
        let mut f = Self {
            h,
            machine: String::new(),
        };
        let out = f.run(&["swarm", "initialize"], r##"{"projects":[{"id":"example-project","name":"example","color":"#123456","folders":[],"sessions":[]}]}"##, None);
        assert_eq!(out["ok"], true, "{out}");
        let config = Config {
            home: f.h.temp.path().display().to_string(),
            dir: f.h.temp.path().join("config"),
            state: f.h.temp.path().to_owned(),
            selves: vec!["test".into()],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: BTreeMap::new(),
            tab: false,
            tab_colors: String::new(),
        };
        f.machine = crate::swarm::local_node_id(&config).unwrap();
        let mut create = fixtures::create();
        if let ops::Operation::Create { home, .. } = &mut create {
            *home = f.machine.clone();
        }
        f.h.run(&fixtures::person(), create).unwrap();
        f
    }

    fn bind(&mut self, member: &str) -> String {
        use sha2::{Digest, Sha256};
        let name = format!("codex/{member}");
        let launch = uuid::Uuid::new_v4().to_string();
        let run = format!("run-{member}");
        let session = zerus_team_plan::model::SessionKey {
            machine: self.machine.clone(),
            session_id: format!("launch_id:{launch}"),
        };
        self.h
            .run(
                &fixtures::service(),
                ops::Operation::SetMemberSession {
                    member: member.into(),
                    session,
                    account: "example".into(),
                    worktree: fixtures::fixture()
                        .members
                        .into_iter()
                        .find(|m| m.id == member)
                        .unwrap()
                        .worktree,
                    ownership_nonce: uuid::Uuid::new_v4().to_string(),
                },
            )
            .unwrap();
        let pid = std::process::id();
        let record = json!({"version":1,"name":name,"agent":"codex","run_id":run,"launch_id":launch,"pid":pid,"process_start":crate::platform::process_start_time(pid).unwrap(),"activity":"idle"});
        let path = self
            .h
            .temp
            .path()
            .join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
        std::fs::write(path, record.to_string()).unwrap();
        name
    }

    fn run(&self, args: &[&str], input: &str, session: Option<&str>) -> Value {
        let reply = self
            .h
            .temp
            .path()
            .join(format!("reply-{}.json", uuid::Uuid::new_v4()));
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tasks::teams::tests::cli_entry", "--nocapture"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.h.temp.path())
            .env("HGS_CONFIG_DIR", self.h.temp.path().join("config"))
            .env("HGS_STATE_DIR", self.h.temp.path())
            .env("HGS_SELF", "test")
            .env("HGS_TAB", "0")
            .env("HGS_TEAMS_TEST_REPLY", &reply)
            .env("HGS_TEAMS_TEST_ARGS", serde_json::to_string(args).unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(session) = session {
            command.env("HGS_SESSION", session).env(
                "HGS_RUN_ID",
                format!("run-{}", session.rsplit('/').next().unwrap()),
            );
        }
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap()
    }

    fn add(&self) -> Value {
        let out = self.run(
            &["add", "--project", "example", "--team", TEAM, "--json"],
            "A queued task",
            None,
        );
        assert_eq!(out["ok"], true, "{out}");
        out["value"]["task"].clone()
    }

    fn document(&self, task: &Value) -> TaskDoc {
        ProjectStore::open(&self.h.temp.path().join("replica"), "example-project")
            .unwrap()
            .load(task["uuid"].as_str().unwrap())
            .unwrap()
    }

    fn paused_send(
        &self,
        reference: &str,
        session: &str,
        pause: &std::path::Path,
    ) -> (std::process::Child, std::path::PathBuf) {
        let reply = self
            .h
            .temp
            .path()
            .join(format!("reply-{}.json", uuid::Uuid::new_v4()));
        let args = ["send", reference, "--session", session, "--json"];
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tasks::teams::tests::cli_entry", "--nocapture"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.h.temp.path())
            .env("HGS_CONFIG_DIR", self.h.temp.path().join("config"))
            .env("HGS_STATE_DIR", self.h.temp.path())
            .env("HGS_SELF", "test")
            .env("HGS_TAB", "0")
            .env("HGS_SESSION", session)
            .env(
                "HGS_RUN_ID",
                format!("run-{}", session.rsplit('/').next().unwrap()),
            )
            .env("HGS_TEST_ATTEMPT_PAUSE", pause)
            .env("HGS_TEAMS_TEST_REPLY", &reply)
            .env("HGS_TEAMS_TEST_ARGS", serde_json::to_string(&args).unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        (command.spawn().unwrap(), reply)
    }

    fn paused_command(
        &self,
        args: &[&str],
        input: &str,
        session: &str,
        pause: &std::path::Path,
        phase: &str,
    ) -> (std::process::Child, std::path::PathBuf) {
        let reply = self
            .h
            .temp
            .path()
            .join(format!("reply-{}.json", uuid::Uuid::new_v4()));
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tasks::teams::tests::cli_entry", "--nocapture"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.h.temp.path())
            .env("HGS_CONFIG_DIR", self.h.temp.path().join("config"))
            .env("HGS_STATE_DIR", self.h.temp.path())
            .env("HGS_SELF", "test")
            .env("HGS_TAB", "0")
            .env("HGS_SESSION", session)
            .env(
                "HGS_RUN_ID",
                format!("run-{}", session.rsplit('/').next().unwrap()),
            )
            .env("HGS_TEST_ATTEMPT_PAUSE", pause)
            .env("HGS_TEST_ATTEMPT_PAUSE_AT", phase)
            .env("HGS_TEAMS_TEST_REPLY", &reply)
            .env("HGS_TEAMS_TEST_ARGS", serde_json::to_string(args).unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        (child, reply)
    }

    fn observer(
        &self,
        task: &Value,
        ready: &std::path::Path,
    ) -> (std::process::Child, std::path::PathBuf) {
        let reply = self
            .h
            .temp
            .path()
            .join(format!("reply-{}.json", uuid::Uuid::new_v4()));
        let args = [
            "observe-team-fixture",
            "example-project",
            task["uuid"].as_str().unwrap(),
        ];
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", "tasks::teams::tests::cli_entry", "--nocapture"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.h.temp.path())
            .env("HGS_CONFIG_DIR", self.h.temp.path().join("config"))
            .env("HGS_STATE_DIR", self.h.temp.path())
            .env("HGS_SELF", "test")
            .env("HGS_TEAM_OBSERVE_READY", ready)
            .env("HGS_TEST_TEAM_OBSERVE_PAUSE", ready.with_extension("pause"))
            .env("HGS_TEAMS_TEST_REPLY", &reply)
            .env("HGS_TEAMS_TEST_ARGS", serde_json::to_string(&args).unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        (cmd.spawn().unwrap(), reply)
    }
}

#[test]
fn cli_duplicate_add_and_target_create_one_plan_node() {
    let f = Fixture::new();
    let id = uuid::Uuid::new_v4().to_string();
    let args = [
        "add",
        "--project",
        "example",
        "--team",
        TEAM,
        "--title",
        "A task",
        "--request-id",
        &id,
        "--json",
    ];
    let first = f.run(&args, "Details", None);
    let second = f.run(&args, "Details", None);
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(second["ok"], true, "{second}");
    assert_eq!(
        first["value"]["task"]["uuid"],
        second["value"]["task"]["uuid"]
    );
    let reference = format!("example/{}", first["value"]["task"]["id"].as_str().unwrap());
    for _ in 0..2 {
        let out = f.run(&["target", &reference, "team", TEAM, "--json"], "", None);
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(out["value"]["task"]["target"]["kind"], "any");
    }
    assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
}

#[test]
fn cli_agent_intake_uses_any_target_and_waits_for_admission() {
    let mut f = Fixture::new();
    let member = f.bind("m1");
    let out = f.run(
        &["add", "--project", "example", "--team", TEAM, "--json"],
        "A suggested task",
        Some(&member),
    );
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["value"]["task"]["target"]["kind"], "any");
    let nodes = f.h.store.queue(TEAM, 200, None).unwrap().rows;
    assert_eq!(nodes.len(), 1);
    assert!(!nodes[0].admitted);
}

#[test]
fn cli_preview_off_team_add_and_target_write_nothing() {
    let f = Fixture::new();
    let task = f.add();
    let before = f.document(&task).revision();
    std::fs::remove_file(f.h.temp.path().join("teams/preview")).unwrap();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    for args in [
        vec!["add", "--project", "example", "--team", TEAM, "--json"],
        vec!["target", &reference, "team", TEAM, "--json"],
    ] {
        let out = f.run(&args, "Another task", None);
        assert_eq!(out["ok"], false, "{out}");
        assert!(
            out["error"].as_str().unwrap().contains("teams_inactive"),
            "{out}"
        );
    }
    assert_eq!(f.document(&task).revision(), before);
    let tasks = ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
    assert_eq!(tasks.uuids().unwrap().len(), 1);
}

#[test]
fn cli_guarded_tasks_refuse_all_agent_control_actions() {
    let mut f = Fixture::new();
    let lead = f.bind("lead");
    let member = f.bind("m1");
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    for args in [
        vec!["send", &reference, "--json"],
        vec!["handoff", &reference, "--json"],
        vec!["stage", &reference, "done", "--json"],
        vec!["stage", &reference, "backlog", "--json"],
        vec!["target", &reference, "any", "--json"],
        vec!["start", &reference, "--json"],
        vec!["worker", &reference, "--json"],
        vec!["cancel", &reference, "--json"],
        vec!["move", &reference, "--top", "--json"],
        vec!["edit", &reference, "--json"],
        vec!["rename", &reference, "--json"],
        vec![
            "message",
            &reference,
            "--request-id",
            "00000000-0000-4000-8000-000000000001",
            "--mode",
            "reopen",
            "--json",
        ],
    ] {
        for actor in [&lead, &member] {
            let before = f.document(&task).revision();
            let result = f.run(&args, "Continue", Some(actor));
            assert_eq!(result["ok"], false, "{result}");
            assert!(
                result["error"].as_str().unwrap().contains("not_allowed"),
                "{result}"
            );
            assert_eq!(f.document(&task).revision(), before);
        }
    }
}

#[test]
fn cli_only_the_person_can_accept_a_reported_task_and_observation_is_explicit() {
    for as_lead in [false, true] {
        let mut f = Fixture::new();
        let lead = f.bind("lead");
        f.h.settings(|s| s.integration = zerus_team_plan::model::Integration::None);
        let task = f.add();
        {
            let tasks =
                ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
            let mut doc = tasks.load(task["uuid"].as_str().unwrap()).unwrap();
            let session = SessionKey {
                machine: f.machine.clone(),
                session: "codex/example".into(),
                session_id: "run_id:example".into(),
            };
            doc.record_send(&session, "example-request", &Actor::Person, now())
                .unwrap();
            doc.report("Ready", ReportKind::Ready, None, &Actor::Person, now())
                .unwrap();
            tasks.save(&mut doc).unwrap();
        }
        let reference = format!("example/{}", task["id"].as_str().unwrap());
        let result = f.run(
            &["stage", &reference, "done", "--json"],
            "",
            as_lead.then_some(lead.as_str()),
        );
        assert_eq!(result["ok"], !as_lead, "{result}");
        if as_lead {
            assert_eq!(f.document(&task).stage().unwrap(), Stage::Review);
        } else {
            assert_eq!(f.document(&task).stage().unwrap(), Stage::Done);
            assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
            crate::teams::tasks::observe(
                f.h.temp.path(),
                &zerus_team_plan::model::TaskRef {
                    project: "example-project".into(),
                    uuid: task["uuid"].as_str().unwrap().into(),
                },
            )
            .unwrap();
            assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 0);
        }
    }
}

#[test]
fn cli_another_computer_refuses_even_the_home_lead_but_allows_a_person() {
    let mut f = Fixture::new();
    let lead = f.bind("lead");
    let task = f.add();
    {
        let tasks =
            ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = tasks.load(task["uuid"].as_str().unwrap()).unwrap();
        doc.set_team(
            Some(&zerus_replica::TaskTeam {
                team: TEAM.into(),
                home: uuid::Uuid::new_v4().to_string(),
            }),
            &Actor::Person,
            now(),
        )
        .unwrap();
        tasks.save(&mut doc).unwrap();
    }
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    for args in [
        vec!["send", &reference],
        vec!["handoff", &reference],
        vec!["stage", &reference, "done"],
    ] {
        let result = f.run(&args, "", Some(&lead));
        assert_eq!(result["ok"], false, "{result}");
        assert!(
            result["error"].as_str().unwrap().contains("not_allowed"),
            "{result}"
        );
        let mut safe = args.clone();
        if args[0] != "stage" {
            safe.extend([
                "--session",
                "codex/unavailable",
                "--session-id",
                "launch_id:missing-fixture",
            ]);
        }
        let person = f.run(&safe, "", None);
        assert!(
            !person["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not_allowed"),
            "{person}"
        );
    }
}

#[test]
fn a_send_that_becomes_a_team_task_after_freeze_never_reserves_an_attempt() {
    let mut f = Fixture::new();
    let member = f.bind("m1");
    let ordinary = f.run(
        &["add", "--project", "example", "--json"],
        "Ordinary task",
        None,
    );
    let task = &ordinary["value"]["task"];
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let pause = f.h.temp.path().join("attempt-pause");
    std::fs::create_dir(&pause).unwrap();
    let (child, reply) = f.paused_send(&reference, &member, &pause);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while !pause.join("ready").exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = pause.join("ready").is_file();
    let joined = if ready {
        Some(f.run(&["target", &reference, "team", TEAM, "--json"], "", None))
    } else {
        None
    };
    std::fs::write(pause.join("release"), "go").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(ready, "{}", String::from_utf8_lossy(&output.stdout));
    assert_eq!(joined.unwrap()["value"]["team"]["queued"], true);
    let response: Value = serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap();
    let id = std::fs::read_to_string(pause.join("ready")).unwrap();
    assert!(
        f.document(task).attempt(&id).unwrap().is_none(),
        "a guarded task received an attempt: {response}"
    );
    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"].as_str().unwrap().contains("not_allowed"),
        "{response}"
    );
}

#[test]
fn preflight_capacity_and_routing_fail_before_creating_a_task() {
    let mut f = Fixture::new();
    f.h.settings(|s| s.caps.max_open_tasks = 1);
    f.add();
    let out = f.run(
        &["add", "--project", "example", "--team", TEAM, "--json"],
        "Over capacity",
        None,
    );
    assert_eq!(out["ok"], false, "{out}");
    assert!(out["error"].as_str().unwrap().contains("over_cap"));
    let tasks = ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
    assert_eq!(tasks.uuids().unwrap().len(), 1);
    drop(tasks);
    f.h.settings(|s| s.caps.max_open_tasks = 10);
    for option in ["--role", "--pipeline"] {
        let out = f.run(
            &[
                "add",
                "--project",
                "example",
                "--team",
                TEAM,
                option,
                "missing",
                "--json",
            ],
            "Unknown route",
            None,
        );
        assert_eq!(out["ok"], false, "{out}");
    }
    assert_eq!(
        ProjectStore::open(&f.h.temp.path().join("replica"), "example-project")
            .unwrap()
            .uuids()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn late_refusal_reports_the_existing_task_and_removes_its_marker() {
    let f = Fixture::new();
    f.h.store.db.as_ref().unwrap().execute_batch("CREATE TRIGGER reject_team_intake BEFORE INSERT ON live_tasks BEGIN SELECT RAISE(ABORT,'fixture team refusal'); END").unwrap();
    let out = f.run(
        &["add", "--project", "example", "--team", TEAM, "--json"],
        "Task survives team refusal",
        None,
    );
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["value"]["task_exists"], true);
    assert_eq!(out["value"]["team"]["queued"], false);
    assert_eq!(out["value"]["team"]["error"]["code"], "invalid_state");
    assert!(out["value"]["message"]
        .as_str()
        .unwrap()
        .contains("exists and was not queued"));
    assert_eq!(f.document(&out["value"]["task"]).team().unwrap(), None);
    assert!(f.h.store.queue(TEAM, 200, None).unwrap().rows.is_empty());
}

#[test]
fn an_observation_refusal_keeps_successful_intake_and_marks_it_pending() {
    let f = Fixture::new();
    f.h.store.db.as_ref().unwrap().execute_batch("CREATE TRIGGER reject_first_observation BEFORE UPDATE OF task_revision ON nodes BEGIN SELECT RAISE(ABORT,'fixture observation refusal'); END").unwrap();
    let out = f.run(
        &["add", "--project", "example", "--team", TEAM, "--json"],
        "Queued with pending observation",
        None,
    );
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["value"]["team"]["queued"], true);
    assert_eq!(out["value"]["team"]["observation_pending"], true);
    assert!(f.document(&out["value"]["task"]).team().unwrap().is_some());
    assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
}

#[test]
fn retarget_clears_the_marker_and_the_bare_team_word_remains_a_session() {
    let f = Fixture::new();
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let out = f.run(&["target", &reference, "team", "--json"], "", None);
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["value"]["task"]["target"]["session"], "team");
    let doc = f.document(&task);
    assert_eq!(doc.team().unwrap(), None);
    assert!(doc
        .view()
        .unwrap()
        .log
        .last()
        .unwrap()
        .text
        .starts_with("Removed from team "));
    for option in ["--role", "--pipeline"] {
        let out = f.run(
            &["add", "--project", "example", option, "developer", "--json"],
            "Wrong flags",
            None,
        );
        assert_eq!(out["ok"], false, "{out}");
        assert!(out["error"].as_str().unwrap().contains("require --team"));
    }
}

#[test]
fn invalid_and_extended_keys_still_get_and_list_as_tasks() {
    let f = Fixture::new();
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    for (raw, invalid) in [
        (
            json!({"team":TEAM,"home":f.machine,"future":true}).to_string(),
            false,
        ),
        ("not JSON".into(), true),
        ("false".into(), true),
        (
            json!({"team":TEAM,"home":f.machine,"future":"x".repeat(2049)}).to_string(),
            true,
        ),
    ] {
        {
            let store =
                ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
            let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
            doc.set_raw_team_for_test(&raw);
            store.save(&mut doc).unwrap();
        }
        let get = f.run(&["get", &reference, "--json"], "", None);
        assert_eq!(get["ok"], true, "{get}");
        assert_eq!(
            get["value"]["task"]["team_invalid"]
                .as_bool()
                .unwrap_or(false),
            invalid
        );
        let list = f.run(&["list", "--project", "example", "--json"], "", None);
        assert_eq!(list["ok"], true, "{list}");
        assert_eq!(
            list["value"]["tasks"].as_array().unwrap().len(),
            1,
            "{list}"
        );
    }
}

#[test]
fn a_foreign_home_marker_requires_a_person_to_clear_it_before_intake() {
    let mut f = Fixture::new();
    let member = f.bind("m1");
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let home = uuid::Uuid::new_v4().to_string();
    {
        let store =
            ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
        doc.set_team(
            Some(&zerus_replica::TaskTeam {
                team: TEAM.into(),
                home: home.clone(),
            }),
            &Actor::Person,
            now(),
        )
        .unwrap();
        store.save(&mut doc).unwrap();
    }
    let agent = f.run(
        &["target", &reference, "team", TEAM, "--json"],
        "",
        Some(&member),
    );
    assert_eq!(agent["ok"], false, "{agent}");
    let person = f.run(&["target", &reference, "team", TEAM, "--json"], "", None);
    assert_eq!(person["ok"], true, "{person}");
    assert_eq!(person["value"]["team"]["queued"], false);
    assert!(person["value"]["message"]
        .as_str()
        .unwrap()
        .contains("target REF any"));
    assert_eq!(f.document(&task).team().unwrap().unwrap().home, home);
    assert_eq!(
        f.run(&["target", &reference, "any", "--json"], "", None)["ok"],
        true
    );
    let queued = f.run(&["target", &reference, "team", TEAM, "--json"], "", None);
    assert_eq!(queued["value"]["team"]["queued"], true, "{queued}");
}

#[test]
fn service_observation_waits_for_the_project_lock_and_reads_the_later_document() {
    let f = Fixture::new();
    let task = f.add();
    let tasks = ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
    let mut doc = tasks.load(task["uuid"].as_str().unwrap()).unwrap();
    let ready = f.h.temp.path().join("observe-ready");
    let (child, reply) = f.observer(&task, &ready);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while !ready.exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let started = ready.exists();
    doc.set_title("Updated while the observer waits", &Actor::Person, now())
        .unwrap();
    tasks.save(&mut doc).unwrap();
    let revision = doc.revision();
    drop(tasks);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(started);
    let answer: Value = serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    let node = f.h.store.queue(TEAM, 200, None).unwrap().rows[0].id;
    let observation = f.h.store.task_observation(TEAM, node).unwrap().unwrap();
    assert_eq!(observation.revision, revision);
    assert_eq!(observation.title, "Updated while the observer waits");
}

fn restore_marker_while_paused(f: &Fixture, task: &Value, pause: &std::path::Path) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while !pause.join("ready").exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let ready = pause.join("ready").is_file();
    if ready {
        let store =
            ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
        doc.set_team(
            Some(&zerus_replica::TaskTeam {
                team: TEAM.into(),
                home: f.machine.clone(),
            }),
            &Actor::Person,
            now(),
        )
        .unwrap();
        store.save(&mut doc).unwrap();
    }
    std::fs::write(pause.join("release"), "go").unwrap();
    ready
}

fn guarded_reload_case(reopen: bool, legacy: bool) {
    let mut f = Fixture::new();
    let member = f.bind("m1");
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    {
        let store =
            ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
        // Simulate a remote copy whose team marker has not arrived yet.
        doc.set_team(None, &Actor::Person, now()).unwrap();
        if reopen {
            let bound =
                f.h.store
                    .member(TEAM, "m1")
                    .unwrap()
                    .unwrap()
                    .session
                    .unwrap();
            let key = SessionKey {
                machine: bound.machine,
                session_id: bound.session_id,
                session: member.clone(),
            };
            doc.record_send(&key, "fixture-send", &Actor::Person, now())
                .unwrap();
            doc.report("Ready", ReportKind::Ready, None, &Actor::Person, now())
                .unwrap();
        }
        store.save(&mut doc).unwrap();
        if legacy {
            let mut project = store.project().unwrap();
            project
                .set_cutover_hold(true, &Actor::Person, now())
                .unwrap();
            store.save_project(&project).unwrap();
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let args = if reopen {
        vec![
            "message",
            &reference,
            "--mode",
            "reopen",
            "--request-id",
            &id,
            "--json",
        ]
    } else {
        vec!["send", &reference, "--session", &member, "--json"]
    };
    let pause = f.h.temp.path().join("reload-pause");
    std::fs::create_dir(&pause).unwrap();
    let (child, reply) = f.paused_command(
        &args,
        if reopen { "Please revise" } else { "" },
        &member,
        &pause,
        if legacy { "legacy_reload" } else { "admission" },
    );
    let ready = restore_marker_while_paused(&f, &task, &pause);
    let before = f.document(&task).revision();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(ready, "{}", String::from_utf8_lossy(&output.stdout));
    let out: Value = serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap();
    let doc = f.document(&task);
    assert!(doc.attempt(&id).unwrap().is_none(), "{out}");
    assert!(
        !doc.view()
            .unwrap()
            .log
            .iter()
            .any(|e| e.request_id.as_deref() == Some(&id)),
        "{out}"
    );
    if legacy {
        assert_eq!(doc.revision(), before, "{out}");
    }
    assert_eq!(
        doc.stage().unwrap(),
        if reopen { Stage::Review } else { Stage::Todo }
    );
    assert_eq!(out["ok"], false, "{out}");
    assert!(
        out["error"].as_str().unwrap().contains("not_allowed"),
        "{out}"
    );
}

#[test]
fn send_legacy_reload_rechecks_a_new_team_marker() {
    guarded_reload_case(false, true);
}
#[test]
fn reopen_admission_rechecks_a_new_team_marker() {
    guarded_reload_case(true, false);
}
#[test]
fn reopen_legacy_reload_rechecks_a_new_team_marker() {
    guarded_reload_case(true, true);
}

#[test]
fn retarget_releases_the_live_node_before_an_agent_can_claim_the_keyless_task() {
    for mode in ["ours", "other_team", "other_home", "invalid"] {
        let mut f = Fixture::new();
        let member = f.bind("m1");
        let task = f.add();
        if mode != "ours" {
            let store =
                ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
            let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
            if mode == "invalid" {
                doc.set_raw_team_for_test("not JSON");
            } else {
                doc.set_team(
                    Some(&zerus_replica::TaskTeam {
                        team: if mode == "other_team" {
                            uuid::Uuid::new_v4().to_string()
                        } else {
                            TEAM.into()
                        },
                        home: if mode == "other_home" {
                            uuid::Uuid::new_v4().to_string()
                        } else {
                            f.machine.clone()
                        },
                    }),
                    &Actor::Person,
                    now(),
                )
                .unwrap();
            }
            store.save(&mut doc).unwrap();
        }
        let reference = format!("example/{}", task["id"].as_str().unwrap());
        let out = f.run(&["target", &reference, "any", "--json"], "", None);
        assert_eq!(out["ok"], true, "{mode}: {out}");
        assert!(f.document(&task).team().unwrap().is_none());
        assert!(
            f.h.store.queue(TEAM, 200, None).unwrap().rows.is_empty(),
            "{mode}: live node survived retarget"
        );
        let out = f.run(&["start", &reference, "--json"], "", Some(&member));
        assert_eq!(out["ok"], true, "{mode}: {out}");
        assert_eq!(f.document(&task).stage().unwrap(), Stage::Active);
    }
}

#[test]
fn refused_node_release_leaves_the_key_and_retarget_unchanged() {
    let f = Fixture::new();
    let task = f.add();
    let before = f.document(&task).revision();
    f.h.store.db.as_ref().unwrap().execute_batch("CREATE TRIGGER reject_override BEFORE DELETE ON live_tasks BEGIN SELECT RAISE(ABORT,'fixture release refusal'); END").unwrap();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let out = f.run(&["target", &reference, "any", "--json"], "", None);
    assert_eq!(out["ok"], false, "{out}");
    assert_eq!(f.document(&task).revision(), before);
    assert_eq!(f.document(&task).team().unwrap().unwrap().team, TEAM);
    assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
}

#[test]
fn observation_keeps_the_project_lock_from_load_through_record() {
    let f = Fixture::new();
    let task = f.add();
    let ready = f.h.temp.path().join("observe-after-load");
    let pause = ready.with_extension("pause");
    std::fs::create_dir(&pause).unwrap();
    let (child, reply) = f.observer(&task, &ready);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !pause.join("ready").exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let loaded = pause.join("ready").exists();
    // An attempted write between load and record must wait for that lock. If a
    // mutant drops it, write and observe a newer document before releasing it.
    let writer = ProjectStore::open_within(
        &f.h.temp.path().join("replica"),
        "example-project",
        std::time::Duration::from_millis(100),
    )
    .unwrap();
    let lock_held = writer.is_none();
    if let Some(store) = writer {
        let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
        doc.set_title("Newer write between load and record", &Actor::Person, now())
            .unwrap();
        store.save(&mut doc).unwrap();
        drop(store);
        crate::teams::tasks::observe(
            f.h.temp.path(),
            &zerus_team_plan::model::TaskRef {
                project: "example-project".into(),
                uuid: task["uuid"].as_str().unwrap().into(),
            },
        )
        .unwrap();
    }
    std::fs::write(pause.join("release"), "go").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(loaded);
    let out: Value = serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap();
    assert_eq!(out["ok"], true, "{out}");
    let node = f.h.store.queue(TEAM, 200, None).unwrap().rows[0].id;
    assert_eq!(
        f.h.store
            .task_observation(TEAM, node)
            .unwrap()
            .unwrap()
            .revision,
        f.document(&task).revision(),
        "an older read overwrote the newer observation"
    );
    assert!(
        lock_held,
        "observation released the project lock before recording"
    );
}

#[test]
fn a_legacy_holder_acknowledges_and_reports_while_a_non_holder_cannot_cancel() {
    let mut f = Fixture::new();
    let lead = f.bind("lead");
    let member = f.bind("m1");
    let task = f.add();
    {
        let store =
            ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = store.load(task["uuid"].as_str().unwrap()).unwrap();
        let bound =
            f.h.store
                .member(TEAM, "m1")
                .unwrap()
                .unwrap()
                .session
                .unwrap();
        doc.record_send(
            &SessionKey {
                machine: bound.machine,
                session_id: bound.session_id,
                session: member.clone(),
            },
            "legacy-dispatch",
            &Actor::Person,
            now(),
        )
        .unwrap();
        store.save(&mut doc).unwrap();
    }
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let before = f.document(&task).revision();
    let current = f.document(&task).view().unwrap();
    let holder = current.session.as_ref().unwrap();
    let guard = json!({"schema":1,"dispatch_id":current.execution_id().unwrap(),"stage_at":current.stage_at,"machine":holder.machine,"session_id":holder.session_id,"reason":"Stop"}).to_string();
    for action in ["cancel", "cancel-execution"] {
        let out = f.run(&[action, &reference, "--json"], &guard, Some(&lead));
        assert_eq!(out["ok"], false, "{out}");
        assert!(
            out["error"].as_str().unwrap().contains("not_allowed"),
            "{out}"
        );
        assert_eq!(f.document(&task).revision(), before);
    }
    let out = f.run(&["start", &reference, "--json"], "", Some(&member));
    assert_eq!(out["ok"], true, "{out}");
    let out = f.run(&["report", &reference, "--json"], "Ready", Some(&member));
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(f.document(&task).stage().unwrap(), Stage::Review);
}

#[test]
fn home_marked_tasks_follow_ordinary_commands_while_the_preview_is_off() {
    let mut f = Fixture::new();
    let member = f.bind("m1");
    let task = f.add();
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    std::fs::remove_file(f.h.temp.path().join("teams/preview")).unwrap();
    for (args, text) in [
        (vec!["stage", &reference, "backlog", "--json"], ""),
        (vec!["rename", &reference, "--json"], "Dormant local team"),
    ] {
        let out = f.run(&args, text, Some(&member));
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(f.document(&task).team().unwrap().unwrap().team, TEAM);
    }
    assert_eq!(f.document(&task).stage().unwrap(), Stage::Backlog);
}

#[test]
fn a_task_reenters_as_a_new_node_after_a_persons_retarget() {
    let f = Fixture::new();
    let task = f.add();
    let old = f.h.store.queue(TEAM, 200, None).unwrap().rows[0].id;
    let reference = format!("example/{}", task["id"].as_str().unwrap());
    let out = f.run(&["target", &reference, "any", "--json"], "", None);
    assert_eq!(out["ok"], true, "{out}");
    assert!(f.h.store.queue(TEAM, 200, None).unwrap().rows.is_empty());
    assert_eq!(
        f.h.store.task_observation(TEAM, old).unwrap().unwrap().key,
        ops::KeyState::Absent
    );
    let out = f.run(&["target", &reference, "team", TEAM, "--json"], "", None);
    assert_eq!(out["value"]["team"]["queued"], true, "{out}");
    let new = f.h.store.queue(TEAM, 200, None).unwrap().rows[0].id;
    assert_ne!(old, new);
    assert_eq!(
        f.h.store.task_observation(TEAM, new).unwrap().unwrap().key,
        ops::KeyState::Ours
    );
    crate::teams::tasks::observe(
        f.h.temp.path(),
        &zerus_team_plan::model::TaskRef {
            project: "example-project".into(),
            uuid: task["uuid"].as_str().unwrap().into(),
        },
    )
    .unwrap();
    assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows[0].id, new);
}
