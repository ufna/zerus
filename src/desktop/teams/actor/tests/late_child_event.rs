//! zerus-63rru.88 reproduction, committed and ignored (red by design): a
//! delayed PreToolUse or PostToolUse of a child, applied through the real
//! hook writer — a child process that runs `__state hook`, which takes the
//! record lock — after that child's SubagentStop, sets the child working
//! again. The member's row then reads one active child, busy, with no
//! unknown fact. The fix must keep the child finished: an event older than
//! the child's recorded end never moves it to working.
use super::super::fleet::Facts;
use crate::state::listing_fixture::{confirmed, launched_record, listed_live_row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::{Command, Stdio};

fn hook(envs: &[(&str, &str)], event: &Value) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "desktop::teams::actor::tests::late_child_event::hook_entry",
            "--ignored",
            "--nocapture",
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HGS_LATE_CHILD", "1")
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the hook child");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("the hook ran");
    assert!(
        out.status.success(),
        "hook {event}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The real `__state hook` entry, run as a child so the record lock is the
/// production one. The event arrives on stdin.
#[test]
#[ignore = "run by the reproduction's hook children (zerus-63rru.88)"]
fn hook_entry() {
    if std::env::var("HGS_LATE_CHILD").is_err() {
        return;
    }
    eprintln!(
        "ENTRY env session={:?} run={:?} state={:?}",
        std::env::var("HGS_SESSION"),
        std::env::var("HGS_RUN_ID"),
        std::env::var("HGS_STATE_DIR")
    );
    let code = match crate::state::dispatch(&["hook".to_owned()]) {
        Ok(code) => {
            eprintln!("ENTRY dispatch ok {code}");
            code
        }
        Err(e) => {
            eprintln!("ENTRY dispatch err {e}");
            3
        }
    };
    std::process::exit(code);
}

fn claude(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id,"agent_type":"general-purpose",
        "session_id":"conversation-claude",
        "last_assistant_message":"the child's result","error_message":"the child failed"})
}

fn codex(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id,"session_id":"conversation-codex",
        "error_message":"the child failed"})
}

/// A private tmux pane that stands for the member's live terminal, in the
/// fixture's own socket directory, torn down when dropped.
struct Terminal {
    dir: std::path::PathBuf,
}
impl Terminal {
    fn new(dir: std::path::PathBuf) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let terminal = Self { dir };
        let sockets = terminal.socket().parent().unwrap().to_owned();
        std::fs::create_dir_all(&sockets).unwrap();
        for dir in [&terminal.dir, &sockets] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        terminal
    }
    fn socket(&self) -> std::path::PathBuf {
        let uid = unsafe { libc::getuid() };
        self.dir.join(format!("tmux-{uid}")).join("default")
    }
    fn tmux(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .arg("-S")
            .arg(self.socket())
            .args(args)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(self.socket())
            .arg("kill-server")
            .output();
    }
}

/// The reproduction: writes the record through the state's own writer, gives
/// it a live pane in a private tmux server, applies Start, Stop and then the
/// delayed tool event through three real hook children (the lock path), and
/// reads the row with the actor's facts. Red until the fix: the delayed
/// event sets the child working for ever.
#[test]
#[ignore = "zerus-63rru.88: red by design; a delayed child tool event applied after its SubagentStop must not set the child working"]
fn a_delayed_child_tool_event_after_its_stop_must_not_set_it_working() {
    let state = std::env::var("HGS_STATE_DIR").unwrap();
    let cfg = std::env::var("HGS_CONFIG_DIR").unwrap();
    let tmux_root = std::env::var("TMUX_TMPDIR").unwrap();
    for (agent, builder, kinds) in [
        (
            "claude",
            claude as fn(&str, &str) -> Value,
            &["PreToolUse", "PostToolUse"][..],
        ),
        (
            "codex",
            codex as fn(&str, &str) -> Value,
            &["PreToolUse", "PostToolUse"][..],
        ),
    ] {
        let name = format!("{agent}/example/member");
        let mut record = launched_record(
            agent,
            &name,
            "00000000-0000-4000-8000-0000000000a1",
            "00000000-0000-4000-8000-0000000000a2",
            &format!("run-{agent}"),
            "/",
            "account-example",
        );
        confirmed(&mut record, &format!("conversation-{agent}"));
        let terminal =
            Terminal::new(std::path::Path::new(&tmux_root).join(format!("late-{agent}")));
        terminal.tmux(&[
            "new-session",
            "-d",
            "-s",
            &format!("late-{agent}"),
            "-x",
            "80",
            "-y",
            "24",
            "exec sleep 600",
        ]);
        let pane = terminal.tmux(&[
            "display-message",
            "-p",
            "-t",
            &format!("late-{agent}"),
            "#{pane_id}",
        ]);
        let run = format!("run-{agent}");
        terminal.tmux(&["set-option", "-p", "-t", &pane, "@hgs_run", &run]);
        let pid: u32 = terminal
            .tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"])
            .parse()
            .unwrap();
        record["pane"] = json!(pane);
        record["terminal_name"] = json!(format!("late-{agent}"));
        record["pid"] = json!(pid);
        record["process_start"] = json!(crate::platform::process_start_time(pid).unwrap());
        crate::state::inspection_for_test(&mut record).unwrap();
        let id = if agent == "claude" { "agent-a1" } else { "d1" };
        let envs = [
            ("HGS_SESSION", name.as_str()),
            ("HGS_RUN_ID", run.as_str()),
            ("HGS_STATE_DIR", state.as_str()),
            ("HGS_CONFIG_DIR", cfg.as_str()),
            ("TMUX_TMPDIR", terminal.dir.to_str().unwrap()),
        ];
        hook(&envs, &builder("SubagentStart", id));
        hook(&envs, &builder("SubagentStop", id));
        for kind in kinds {
            hook(&envs, &builder(kind, id));
            // The record the hooks wrote, read back from the product's own
            // file.
            let path = std::path::Path::new(&state)
                .join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
            let current: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let row = listed_live_row(current.clone()).expect("a listed row");
            let facts = Facts::of(&row);
            eprintln!(
                "{agent} {kind} after the delayed event: child={:?} facts=children:{:?} activity:{:?} phase:{:?}",
                current["subagents"][id],
                facts.children,
                facts.activity,
                facts.phase,
            );
            assert_eq!(
                current["subagents"][id]["state"], "finished",
                "{agent} {kind}: the delayed event set the child working again"
            );
            assert_ne!(
                facts.children,
                Some(1),
                "{agent} {kind}: the member reads one active child after a delayed event"
            );
        }
    }
}
