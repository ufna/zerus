use super::*;
use std::process::{Child, Command, Stdio};

const PARENT: &str = "codex/example/parent";
const CHILD: &str = "codex/example/child";
const MACHINE: &str = "11111111-1111-4111-8111-111111111111";
const PARENT_ID: &str = "22222222-2222-4222-8222-222222222222";
const CHILD_ID: &str = "33333333-3333-4333-8333-333333333333";
const CHAT: &str = "44444444-4444-4444-8444-444444444444";
const FIXTURE: &str = "ZERUS_POLICY_ADVISORY_FIXTURE";

struct OfflineChild(Child);
impl Drop for OfflineChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn parent_read_is_body_free_exact_and_stable_across_processes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("codex");
    fs::create_dir_all(home.join("sessions")).unwrap();
    let path = home.join(format!("sessions/rollout-{CHAT}.jsonl"));
    let at = now() as i64 - 20;
    let stamp = |seconds| {
        chrono::DateTime::from_timestamp(seconds, 0)
            .unwrap()
            .to_rfc3339()
    };
    fs::write(&path, format!("{}\n{}\n{}\n",
        json!({"type":"session_meta","payload":{"id":CHAT}}),
        json!({"timestamp":stamp(at-5),"type":"event_msg",
            "payload":{"type":"task_started","turn_id":"example-failed-turn"}}),
        json!({"timestamp":stamp(at),"type":"event_msg","payload":{
            "type":"task_complete","turn_id":"example-failed-turn",
            "error":{"message":"private refusal prompt marker","codex_error_info":"cyber_policy"}
        }}),
    )).unwrap();
    // No model or terminal. A fixture process holds the synthetic history.
    let child = OfflineChild(
        Command::new("sleep")
            .arg("120")
            .stdin(Stdio::from(fs::File::open(&path).unwrap()))
            .spawn()
            .unwrap(),
    );
    for mode in ["setup", "restart", "controls"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "state::parent_link::advisory::tests::fixture",
                "--ignored",
                "--test-threads=1",
            ])
            .env(FIXTURE, mode)
            .env("ZERUS_POLICY_PARENT_PID", std::process::id().to_string())
            .env("ZERUS_POLICY_CHILD_PID", child.0.id().to_string())
            .env("ZERUS_POLICY_EVENT_AT", at.to_string())
            .env("HGS_STATE_DIR", temp.path())
            .env("HGS_CONFIG_DIR", temp.path().join("config"))
            .env("HGS_SESSION", PARENT)
            .env("HGS_SESSION_ID", PARENT_ID)
            .env("HGS_RUN_ID", "example-parent-run")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn read_parent() -> Value {
    super::super::apply(
        CHILD,
        super::super::Request {
            action: "read".into(),
            expected_run_id: "example-child-run".into(),
            expected_conversation_id: CHAT.into(),
            replace: false,
            parent: None,
            project: None,
        },
    )
    .unwrap()
}

#[test]
#[ignore = "private process-ancestry and native history fixture"]
fn fixture() {
    let Some(mode) = nonempty_env(FIXTURE) else {
        return;
    };
    let parse = |key| nonempty_env(key).unwrap().parse::<u32>().unwrap();
    let parent_pid = parse("ZERUS_POLICY_PARENT_PID");
    let child_pid = parse("ZERUS_POLICY_CHILD_PID");
    let at = i64::from(parse("ZERUS_POLICY_EVENT_AT"));
    let expected = root().join("advisory.expected.json");
    if mode == "setup" {
        let config =
            crate::config::Config::load().unwrap_or_else(|error| panic!("{}", error.message));
        crate::swarm::save_test_node_id(&config, MACHINE).unwrap();
        let mut parent = json!({"version":1,"agent":"codex","name":PARENT,"session_id":PARENT_ID,
            "run_id":"example-parent-run","conversation_id":"example-parent-chat",
            "pid":parent_pid,"process_start":process_start(parent_pid),"state":"running"});
        write(&mut parent).unwrap();
        let mut child = json!({"version":1,"agent":"codex","name":CHILD,"session_id":CHILD_ID,
            "run_id":"example-child-run","run_identity_version":1,"conversation_id":CHAT,
            "startup_kind":"new","created":at-10,
            "state":"running","pid":child_pid,"process_start":process_start(child_pid),
            "agent_home":root().join("codex"),"last_main_progress_at":at-4,"turn_started":at-5,
            "transcript":root().join(format!("codex/sessions/rollout-{CHAT}.jsonl"))});
        child["parent"] = link_of(&parent, MACHINE, "example", "launch").unwrap();
        child["parent"]["at"] = json!(at - 10);
        write(&mut child).unwrap();

        let reply = read_parent();
        let observed = &reply["policy_advisory"];
        assert_eq!(observed["scope"], "session");
        assert_eq!(
            observed["child"],
            json!({"session_id":CHILD_ID,"run_id":"example-child-run","conversation_id":CHAT})
        );
        assert_eq!(observed["event"]["turn_id"], "example-failed-turn");
        assert!(observed["event"]["event_id"].is_string());
        assert!(!reply.to_string().contains("private refusal prompt marker"));
        assert_eq!(read_parent()["policy_advisory"], *observed);
        fs::write(&expected, observed.to_string()).unwrap();
    } else if mode == "restart" {
        let before: Value = serde_json::from_slice(&fs::read(expected).unwrap()).unwrap();
        assert_eq!(read_parent()["policy_advisory"], before);
    } else {
        std::env::remove_var("HGS_SESSION_ID");
        assert!(read_parent().get("policy_advisory").is_none());
        std::env::set_var("HGS_SESSION_ID", PARENT_ID);
        let mut child = read(CHILD).unwrap();
        let original = child["parent"].clone();
        for (field, changed) in [
            ("source", json!("manual")),
            ("at", json!(at)),
            ("run_id", json!("example-new-parent-run")),
            ("conversation_id", json!("example-new-parent-chat")),
        ] {
            child["parent"] = original.clone();
            child["parent"][field] = changed;
            write(&mut child).unwrap();
            assert!(read_parent().get("policy_advisory").is_none(), "{field}");
        }
        child["parent"] = original;
        child["startup_kind"] = json!("resume");
        write(&mut child).unwrap();
        assert!(
            read_parent().get("policy_advisory").is_none(),
            "resumed history"
        );
        child["startup_kind"] = json!("new");
        child["expected_id"] = json!(false);
        write(&mut child).unwrap();
        assert!(
            read_parent().get("policy_advisory").is_none(),
            "invalid expected conversation"
        );
        child.as_object_mut().unwrap().remove("expected_id");
        child["last_main_progress_at"] = json!(at + 1);
        write(&mut child).unwrap();
        assert!(
            read_parent().get("policy_advisory").is_none(),
            "newer progress"
        );
    }
    assert!(
        !root().join("coordination").exists(),
        "parent read altered the wait journal"
    );
}
