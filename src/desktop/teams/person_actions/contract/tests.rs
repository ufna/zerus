//! The contract Person steps run the native Person actions in a marked
//! private root, and refuse any other root. Rows come from the product's own
//! writers and are read back through the store's own readers.
use super::*;
use crate::teams::store::Store;
use std::time::Duration;
use zerus_client_core::teams::action::TeamAction;
use zerus_replica::{Actor as TaskActor, Position, ProjectStore, Stage, Target};

const MACHINE: &str = "00000000-0000-4000-8000-0000000000aa";
const PROJECT: &str = "example-project";
const TEAM: &str = "00000000-0000-4000-8000-00000000c182";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const NONCE: &str = "0f3c2b1a-contract-nonce";

/// A private root as the contract harness makes one: its state with the
/// Teams preview on, its configuration and home inside, and its marker.
struct Root {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: Config,
    home: PathBuf,
    /// The account's own home, apart from the root.
    person: PathBuf,
    _person: tempfile::TempDir,
}

fn root() -> Root {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = platform::canonical(temp.path()).unwrap();
    // As the harness's mkdtemp root is.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = root.join("state");
    platform::private_dir(&state.join("teams")).unwrap();
    std::fs::write(state.join("teams/preview"), "on\n").unwrap();
    let dir = root.join("cfg");
    platform::private_dir(&dir).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(root.join(ROOT_FILE), format!("{MARKER}\n{NONCE}\n")).unwrap();
    let config = Config {
        home: home.display().to_string(),
        dir,
        state,
        selves: vec![],
        peers: vec![],
        base_peers: vec![],
        peers_from_environment: false,
        machines: Default::default(),
        tab: false,
        tab_colors: String::new(),
    };
    crate::swarm::fixture_project(&config, MACHINE, PROJECT);
    let person = tempfile::tempdir().unwrap();
    Root {
        root,
        config,
        home,
        person: platform::canonical(person.path()).unwrap(),
        _person: person,
        _temp: temp,
    }
}

impl Root {
    /// One step file, as the harness writes it, run through the entry's path.
    fn step(&self, action: Value) -> Result<Value, String> {
        let value = json!({"root": self.root, "nonce": NONCE, "action": action});
        let path = self
            .root
            .join(format!("step-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, value.to_string()).unwrap();
        step(&path, &self.config, &self.home, &self.person)
    }
    fn admit(&self) -> Result<(), String> {
        admit(&self.root, NONCE, &self.config, &self.home, &self.person)
    }
    fn store(&self) -> Store {
        Store::open(&self.config.state).unwrap()
    }
    fn plan(&self, revision: u64, change: TeamAction) -> Value {
        let plan = wire::Plan {
            request_id: uuid::Uuid::new_v4().to_string(),
            team: TEAM.into(),
            expected_revision: revision,
            action: wire::Action::Team {
                change: Box::new(change),
            },
        };
        json!({"kind": "team", "plan": plan})
    }
    /// The Create the New team form builds for the built-in template.
    fn create(&self) -> Value {
        let catalog = crate::teams::catalog::load(&self.config.dir).unwrap();
        let bundle = zerus_team_plan::bundle(&catalog, "bugfix").unwrap();
        self.plan(
            0,
            TeamAction::Create {
                template: "bugfix".into(),
                project: PROJECT.into(),
                name: "Contract fixture".into(),
                goal: "Real-agent contract.".into(),
                home: MACHINE.into(),
                integration_branch: "zerus/team/contract-fixture".into(),
                base_commit: COMMIT.into(),
                integration: bundle.template.settings.integration,
                expected_template: crate::teams::catalog::template_digest(&bundle).unwrap(),
                delegations: serde_json::from_value(json!({"accept_results":false,
                    "answer_members":false,"confirm_trust":false,"resume_exited":false,
                    "merge_accepted":false}))
                .unwrap(),
            },
        )
    }
    /// A plain task of the project, as the person's ordinary task add saves it.
    fn task(&self) -> (String, String) {
        let project = ProjectStore::open_within(
            &self.config.state.join("replica"),
            PROJECT,
            Duration::from_secs(5),
        )
        .unwrap()
        .unwrap();
        let mut document = project
            .new_task(
                "Contract fixture task",
                Stage::Todo,
                Target::Any,
                TaskActor::Person,
                &Position::End,
                "example",
                1,
            )
            .unwrap();
        project.save(&mut document).unwrap();
        (document.uuid().to_owned(), document.revision())
    }
}

/// Create, a task sent to the team and Stop, each the native Person action
/// the window confirms, against the store the product writes: the team is
/// created at revision 1 by the Person actor of the window, the task becomes
/// its node, and the team stops.
#[test]
fn contract_steps_create_send_a_task_and_stop_through_the_native_actions() {
    let r = root();
    let created = r.step(r.create()).unwrap();
    assert_eq!(
        (created["ok"].clone(), created["revision"].clone()),
        (json!(true), json!(1)),
        "{created}"
    );
    assert_eq!(created["actor"]["origin"], "desktop_ui", "{created}");
    let team = r.store().team(TEAM).unwrap().expect("the created team");
    assert_eq!((team.name.as_str(), team.revision), ("Contract fixture", 1));
    // A repeated Create is not a second team: the original is spent.
    assert_ne!(
        r.step(r.create()).ok().and_then(|v| v["ok"].as_bool()),
        Some(true)
    );

    let (uuid, _) = r.task();
    let sent = r
        .step(json!({"kind": "task", "draft": wire::TaskDraft {
            request_id: uuid::Uuid::new_v4().to_string(),
            team: TEAM.into(),
            action: wire::Action::Intake {
                project: PROJECT.into(),
                task: uuid.clone(),
                revision: String::new(),
                role: None,
                pipeline: None,
            },
        }}))
        .unwrap();
    assert_eq!(sent["ok"], true, "{sent}");
    assert_eq!(sent["task"]["uuid"], uuid.as_str(), "{sent}");
    assert_eq!(sent["team"]["queued"], true, "{sent}");
    let store = r.store();
    let nodes = store.queue(TEAM, 50, None).unwrap().rows;
    assert!(
        nodes
            .iter()
            .any(|node| serde_json::to_value(node).unwrap()["task"]["uuid"] == uuid.as_str()),
        "the task is a node of the team's queue"
    );

    let revision = r.store().team(TEAM).unwrap().unwrap().revision;
    let stopped = r.step(r.plan(revision, TeamAction::Stop {})).unwrap();
    assert_eq!(stopped["ok"], true, "{stopped}");
    assert_eq!(
        r.store().team(TEAM).unwrap().unwrap().status,
        zerus_team_plan::model::TeamStatus::Stopped
    );
}

/// A stale revision is refused at the window's review, which answers only
/// that the action's evidence is unavailable; nothing changes.
#[test]
fn a_stale_stop_is_refused_and_changes_nothing() {
    let r = root();
    r.step(r.create()).unwrap();
    let refused = r.step(r.plan(0, TeamAction::Stop {})).unwrap();
    assert_eq!(
        (refused["ok"].clone(), refused["code"].clone()),
        (
            json!(false),
            json!("team_person_action_evidence_unavailable")
        ),
        "{refused}"
    );
    assert_eq!(r.store().team(TEAM).unwrap().unwrap().revision, 1);
    assert_ne!(
        r.store().team(TEAM).unwrap().unwrap().status,
        zerus_team_plan::model::TeamStatus::Stopped
    );
}

/// Only the harness's marked private root is admitted; nothing is written
/// for any other.
#[test]
fn only_the_harness_marked_private_root_is_admitted() {
    use std::os::unix::fs::PermissionsExt;
    let r = root();
    r.admit().unwrap();
    type Change = fn(&mut Root) -> Option<String>;
    let refusals: &[(&str, Change)] = &[
        ("no marker", |r| {
            std::fs::remove_file(r.root.join(ROOT_FILE)).unwrap();
            None
        }),
        ("another nonce", |r| {
            std::fs::write(
                r.root.join(ROOT_FILE),
                format!("{MARKER}\nanother-nonce-0000\n"),
            )
            .unwrap();
            None
        }),
        ("a marker link", |r| {
            let elsewhere = r.person.join("marker");
            std::fs::write(&elsewhere, format!("{MARKER}\n{NONCE}\n")).unwrap();
            std::fs::remove_file(r.root.join(ROOT_FILE)).unwrap();
            std::os::unix::fs::symlink(&elsewhere, r.root.join(ROOT_FILE)).unwrap();
            None
        }),
        ("an open root", |r| {
            std::fs::set_permissions(&r.root, std::fs::Permissions::from_mode(0o755)).unwrap();
            None
        }),
        ("the state outside", |r| {
            r.config.state = r.person.clone();
            None
        }),
        ("the configuration outside", |r| {
            r.config.dir = r.person.clone();
            None
        }),
        ("the home outside", |r| {
            r.home = r.person.clone();
            None
        }),
        ("the state is the root", |r| {
            r.config.state = r.root.clone();
            None
        }),
        ("the root holds the account's home", |r| {
            let inside = r.root.join("home/person");
            std::fs::create_dir_all(&inside).unwrap();
            r.person = inside;
            None
        }),
        ("a short nonce", |_| Some("short".into())),
    ];
    for (what, change) in refusals {
        let mut r = root();
        let nonce = change(&mut r);
        let result = admit(
            &r.root,
            nonce.as_deref().unwrap_or(NONCE),
            &r.config,
            &r.home,
            &r.person,
        );
        assert!(result.is_err(), "{what}");
    }
    // A root in the account's own default state folder.
    let person = tempfile::tempdir().unwrap();
    let person = platform::canonical(person.path()).unwrap();
    let inside = person.join(".local/state/hgs/fixture");
    platform::private_dir(&inside).unwrap();
    std::fs::set_permissions(&inside, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(inside.join(ROOT_FILE), format!("{MARKER}\n{NONCE}\n")).unwrap();
    for sub in ["state", "cfg", "home"] {
        platform::private_dir(&inside.join(sub)).unwrap();
    }
    let mut config = r.config.clone();
    config.state = inside.join("state");
    config.dir = inside.join("cfg");
    assert!(admit(&inside, NONCE, &config, &inside.join("home"), &person).is_err());
    // A refused root reaches no writer: the step answers before any store.
    let r = root();
    std::fs::remove_file(r.root.join(ROOT_FILE)).unwrap();
    assert!(r.step(r.create()).is_err());
    assert!(r.store().team(TEAM).unwrap().is_none());
}

/// The steps exactly as tests/teams_native_fixture.py builds them (committed by
/// its `TeamsPersonSteps`) are the shapes this entry reads and runs. Only the
/// values change: this store's template, machine, task and revision.
#[test]
fn the_fixtures_committed_steps_run_in_order() {
    let r = root();
    let steps: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/teams-contract-person-steps.json"
    ))
    .unwrap();
    assert_eq!(steps.len(), 3, "create, a task, stop");
    let (task, _) = r.task();
    let catalog = crate::teams::catalog::load(&r.config.dir).unwrap();
    let bundle = zerus_team_plan::bundle(&catalog, "bugfix").unwrap();
    let digest = crate::teams::catalog::template_digest(&bundle).unwrap();
    let mut values = Vec::new();
    for mut step in steps {
        let text = step.to_string();
        assert!(
            text.contains("@request@") && text.contains("@team@"),
            "{text}"
        );
        step = serde_json::from_str(
            &text
                .replace("@request@", &uuid::Uuid::new_v4().to_string())
                .replace("@team@", TEAM),
        )
        .unwrap();
        // Read-only indexing first: indexing a missing key mutably would add it.
        let kind = (
            step["kind"].clone(),
            step["plan"]["action"]["change"]["kind"].clone(),
        );
        if kind == (json!("team"), json!("create")) {
            let change = step["plan"]["action"]["change"].as_object_mut().unwrap();
            change.insert("template".into(), json!("bugfix"));
            change.insert("expected_template".into(), json!(digest));
            change.insert("home".into(), json!(MACHINE));
            change.insert("project".into(), json!(PROJECT));
        } else if kind == (json!("team"), json!("stop")) {
            step["plan"]["expected_revision"] =
                json!(r.store().team(TEAM).unwrap().unwrap().revision);
        } else {
            assert_eq!(kind.0, "task", "{step}");
            let action = step["draft"]["action"].as_object_mut().unwrap();
            action.insert("project".into(), json!(PROJECT));
            action.insert("task".into(), json!(task));
        }
        let reply = r.step(step).unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        values.push(reply);
    }
    assert_eq!(values[1]["team"]["queued"], true);
    assert_eq!(
        r.store().team(TEAM).unwrap().unwrap().status,
        zerus_team_plan::model::TeamStatus::Stopped
    );
}
