//! The caller proof: its parsing, its bound, and a real process tree. The
//! process cases start a command below a detached process that carries session
//! markers, with or without the live record that proves it.
use super::*;
use serde_json::json;

#[test]
fn the_immutable_identity_prefers_a_launch_id() {
    assert_eq!(
        identity(&json!({"launch_id": "l", "run_id": "r"})),
        "launch_id:l"
    );
    assert_eq!(identity(&json!({"run_id": "r"})), "run_id:r");
    assert_eq!(
        identity(&json!({"launch_id": "", "run_id": "r"})),
        "run_id:r"
    );
    assert_eq!(identity(&json!({})), "");
}

#[test]
fn markers_are_read_from_nul_separated_entries_and_need_both() {
    let both = b"HGS_SESSION=claude/example/one\0HGS_RUN_ID=run-1\0PATH=/bin";
    assert_eq!(
        markers(both),
        Some(("claude/example/one".into(), "run-1".into()))
    );
    assert_eq!(markers(b"HGS_SESSION=claude/example/one\0"), None);
    assert_eq!(markers(b"HGS_RUN_ID=run-1\0"), None);
    assert_eq!(markers(b"HGS_SESSION=\0HGS_RUN_ID=run-1\0"), None);
    assert_eq!(markers(b""), None);
}

#[test]
fn the_ancestor_walk_starts_at_this_command_and_is_bounded() {
    // The test runner itself can have a protected same-user ancestor; that is
    // a refusal, never a truncated successful walk. Topology is tested below.
    if let Ok(chain) = ancestors() {
        assert_eq!(chain.first().map(|row| row.pid), Some(std::process::id()));
        assert!(chain.len() <= 64);
    }
}

/// The child that reports what the proof decides. It does nothing unless the
/// parent asks for it, so an ordinary test run never prints.
#[test]
fn probe() {
    if std::env::var("HGS_TEAMS_CALLER_PROBE").as_deref() != Ok("1") {
        return;
    }
    let config = crate::config::Config::load().expect("the probe's configuration");
    if let Ok(words) = std::env::var("HGS_TEAMS_DISPATCH_PROBE") {
        let args = words
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let exit = crate::teams::dispatch(&config, &args, false).unwrap();
        println!("DISPATCH {exit}");
        println!(
            "READS {}",
            crate::state::session_graph::caller_binding_reads()
        );
        return;
    }
    if std::env::var_os("HGS_TEAMS_PANIC_PROBE").is_some() {
        let before: std::collections::BTreeMap<_, _> = std::env::vars_os().collect();
        let result = std::panic::catch_unwind(|| {
            prove_read(
                &config,
                "borrowed",
                "borrowed-run",
                None,
                |name, run, _session| {
                    assert_eq!((name, run), ("borrowed", "borrowed-run"));
                    panic!("injected explicit binding read panic");
                },
            )
        });
        assert!(result.is_err());
        assert_eq!(before, std::env::vars_os().collect());
        println!("PANIC_ENV_UNCHANGED");
    }
    if let Ok(chain) = ancestors() {
        for row in chain {
            println!(
                "ANCESTOR pid={} parent={} top={} environment_readable={}",
                row.pid,
                row.identity.parent,
                row.top,
                environment(row.pid).is_ok()
            );
        }
    }
    let answer = match caller(&config) {
        Caller::Person => "person".to_owned(),
        Caller::Session(key) => format!("session:{}", key.session_id),
        Caller::Refused(problem) => {
            println!("DETAIL {}", problem.message);
            format!("refused:{}", problem.code)
        }
    };
    println!("PROBE {answer}");
    println!(
        "READS {}",
        crate::state::session_graph::caller_binding_reads()
    );
    if let Ok(action) = std::env::var("HGS_TEAMS_PREVIEW_PROBE") {
        let exit = crate::teams::dispatch(&config, &["preview".into(), action], false).unwrap();
        println!("PREVIEW {exit}");
    }
}

#[cfg(target_os = "linux")]
fn catalog(config: &std::path::Path) {
    std::fs::create_dir_all(config.join("swarm")).unwrap();
    let value = json!({"node_id":"00000000-0000-4000-8000-0000000000a1",
        "catalog":{"schema":1,"swarm_id":"00000000-0000-4000-8000-0000000000a2","operations":{}}});
    std::fs::write(config.join("swarm/catalog.json"), value.to_string()).unwrap();
}

// The unqualified macOS context is checked below. Keep the shared process
// fixture sources for a Mac qualification candidate before lifting its gate.
#[cfg(target_os = "linux")]
mod regressions;

#[cfg(target_os = "linux")]
mod process {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    const NAME: &str = "claude/example/probe";
    const RUN: &str = "run-probe";

    /// One detached command tree: its state directory, the live process that
    /// carries the markers, and the output of the `hgs team`-side proof below
    /// it.
    struct Probe {
        _temp: tempfile::TempDir,
        state: PathBuf,
        pid: u32,
        output: PathBuf,
        flag: PathBuf,
        markers_kept: bool,
    }

    impl Probe {
        /// Starts the detached tree with the markers its own environment holds,
        /// and strips them from the command below it unless `markers_kept`.
        fn start(markers_kept: bool) -> Self {
            let temp = tempfile::tempdir().expect("a temporary folder");
            let state = temp.path().join("state");
            let config = temp.path().join("config");
            std::fs::create_dir_all(&state).unwrap();
            std::fs::create_dir_all(&config).unwrap();
            let state = std::fs::canonicalize(state).unwrap();
            let config = std::fs::canonicalize(config).unwrap();
            catalog(&config);
            let pid_file = temp.path().join("pid");
            let flag = temp.path().join("go");
            let output = temp.path().join("output");
            let strip = if markers_kept {
                ""
            } else {
                "env -u HGS_SESSION -u HGS_RUN_ID -u HGS_SESSION_ID "
            };
            let exe = std::env::current_exe().expect("this test binary");
            let inner = format!(
                "sh -c 'echo $$ > {}; while [ ! -f {} ]; do sleep 0.01; done; {}{} --exact teams::caller::tests::probe --nocapture > {} 2>&1'",
                pid_file.display(),
                flag.display(),
                strip,
                exe.display(),
                output.display()
            );
            let status = Command::new("sh")
                .arg("-c")
                .arg(format!("{inner} & exit 0"))
                .env("HGS_SESSION", NAME)
                .env("HGS_RUN_ID", RUN)
                .env("HGS_TEAMS_CALLER_PROBE", "1")
                .env("HGS_STATE_DIR", &state)
                .env("HGS_CONFIG_DIR", &config)
                .env_remove("HGS_SESSION_ID")
                .status()
                .expect("a shell");
            assert!(status.success());
            wait_for(&pid_file, Duration::from_secs(5)).expect("the detached process's pid");
            let pid: u32 = std::fs::read_to_string(&pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            Self {
                _temp: temp,
                state,
                pid,
                output,
                flag,
                markers_kept,
            }
        }

        fn state(&self) -> &Path {
            &self.state
        }

        /// The saved binding that proves this process is a live session.
        fn record(&self, process_start: &str) -> PathBuf {
            let path = self
                .state
                .join(format!("{:x}.json", Sha256::digest(NAME.as_bytes())));
            let record = json!({
                "version": 1,
                "name": NAME,
                "agent": "claude",
                "run_id": RUN,
                "launch_id": "00000000-0000-4000-8000-0000000000c1",
                "session_id": "00000000-0000-4000-8000-0000000000c1",
                "conversation_id": "00000000-0000-4000-8000-0000000000c2",
                "pid": self.pid,
                "process_start": process_start,
                "state": "running",
                "pane": "%0"
            });
            std::fs::write(&path, serde_json::to_string(&record).unwrap()).unwrap();
            path
        }

        fn born(&self) -> String {
            for _ in 0..100 {
                if let Some(start) = crate::platform::process_start_time(self.pid) {
                    return start;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("the detached process has no start time");
        }

        /// Lets the command below run, then reads the line the proof printed.
        fn answer(&self) -> String {
            std::fs::write(&self.flag, b"go").unwrap();
            let until = Instant::now() + Duration::from_secs(30);
            let mut text = String::new();
            while Instant::now() < until {
                text = std::fs::read_to_string(&self.output).unwrap_or_default();
                if let Some(answer) = text.lines().find_map(|line| line.strip_prefix("PROBE ")) {
                    return answer.trim().to_owned();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("the probe printed no answer: {text}");
        }
    }

    fn wait_for(path: &Path, limit: Duration) -> Option<()> {
        let until = Instant::now() + limit;
        while Instant::now() < until {
            if path.metadata().is_ok_and(|meta| meta.len() > 0) {
                return Some(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// A process that is not above this command, recorded as another session.
    fn sibling_record(state: &Path) -> std::process::Child {
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("a sibling process");
        let name = "codex/example/sibling";
        let path = state.join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
        let record = json!({
            "version": 1,
            "name": name,
            "agent": "codex",
            "run_id": "run-sibling",
            "launch_id": "00000000-0000-4000-8000-0000000000c3",
            "session_id": "00000000-0000-4000-8000-0000000000c3",
            "conversation_id": "00000000-0000-4000-8000-0000000000c4",
            "pid": child.id(),
            "process_start": crate::platform::process_start_time(child.id()).unwrap_or_default(),
            "state": "running",
            "pane": "%1"
        });
        std::fs::write(&path, serde_json::to_string(&record).unwrap()).unwrap();
        child
    }

    #[test]
    fn an_unmarked_command_under_a_live_session_agent_is_refused() {
        let probe = Probe::start(false);
        assert!(!probe.markers_kept);
        probe.record(&probe.born());
        assert_eq!(probe.answer(), "refused:not_allowed");
    }

    #[test]
    fn an_unmarked_command_with_an_established_stale_agent_is_the_person() {
        let probe = Probe::start(false);
        probe.record("stale-birth");
        let mut sibling = sibling_record(probe.state());
        // The sibling is recorded and live, so a lookup by "any live record"
        // would refuse; only an ancestor may.
        assert_eq!(probe.answer(), "person");
        let _ = sibling.kill();
        let _ = sibling.wait();
    }

    #[test]
    fn a_stale_record_does_not_match_the_reused_process_id() {
        let probe = Probe::start(false);
        probe.record("0");
        assert_eq!(probe.answer(), "person");
    }

    #[test]
    fn a_marked_command_with_a_live_record_is_that_session() {
        let probe = Probe::start(true);
        probe.record(&probe.born());
        assert_eq!(
            probe.answer(),
            "session:launch_id:00000000-0000-4000-8000-0000000000c1"
        );
    }

    #[test]
    fn a_marked_command_with_no_live_record_is_refused() {
        let probe = Probe::start(true);
        assert_eq!(probe.answer(), "refused:invalid_state");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_refuses_both_callers_and_preview_mutations_until_qualified() {
    for marked in [false, true] {
        for action in ["on", "off"] {
            let temp = tempfile::tempdir().unwrap();
            let state = temp.path().join("state");
            let config = temp.path().join("config");
            std::fs::create_dir(&state).unwrap();
            std::fs::create_dir(&config).unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "teams::caller::tests::probe", "--nocapture"])
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", temp.path())
                .env("HGS_STATE_DIR", &state)
                .env("HGS_CONFIG_DIR", &config)
                .env("HGS_TEAMS_CALLER_PROBE", "1")
                .env("HGS_TEAMS_PREVIEW_PROBE", action);
            if marked {
                command
                    .env("HGS_SESSION", "claude/example/unavailable")
                    .env("HGS_RUN_ID", "run-unavailable");
            }
            let output = command.output().unwrap();
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                text.contains("PROBE refused:not_allowed")
                    && text.contains("not available on this platform")
                    && text.contains("PREVIEW 1"),
                "{text}"
            );
            assert!(!crate::teams::enabled(&state));
        }
    }
}
