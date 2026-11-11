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
    let result = if args[0] == "swarm" {
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
fn cli_each_bypass_is_refused_before_task_changes_with_and_without_the_key() {
    for keyed in [false, true] {
        let mut f = Fixture::new();
        let lead = f.bind("lead");
        let member = f.bind("m1");
        let task = f.add();
        if !keyed {
            let tasks =
                ProjectStore::open(&f.h.temp.path().join("replica"), "example-project").unwrap();
            let mut doc = tasks.load(task["uuid"].as_str().unwrap()).unwrap();
            doc.set_team(None, &Actor::Person, 3).unwrap();
            tasks.save(&mut doc).unwrap();
        }
        let reference = format!("example/{}", task["id"].as_str().unwrap());
        for args in [
            vec!["send", &reference, "--json"],
            vec!["handoff", &reference, "--json"],
            vec!["stage", &reference, "done", "--json"],
        ] {
            let before = f.document(&task).revision();
            let refused = f.run(&args, "", Some(&member));
            assert_eq!(refused["ok"], false, "{refused}");
            assert!(
                refused["error"].as_str().unwrap().contains("team"),
                "{refused}"
            );
            assert_eq!(f.document(&task).revision(), before);
            let mut safe = args.clone();
            if args[0] != "stage" {
                // Resolve a deliberately absent immutable target before any
                // native delivery path can be reached by an allowed caller.
                safe.extend([
                    "--session",
                    "codex/unavailable",
                    "--session-id",
                    "launch_id:missing-fixture",
                ]);
            }
            for session in [None, Some(lead.as_str())] {
                let allowed = f.run(&safe, "", session);
                assert!(
                    !allowed["error"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("not_allowed"),
                    "{allowed}"
                );
            }
        }
    }
}

#[test]
fn cli_home_lead_and_person_can_accept_a_reported_task() {
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
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(f.document(&task).stage().unwrap(), Stage::Done);
        assert_eq!(f.h.store.queue(TEAM, 200, None).unwrap().rows.len(), 0);
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
