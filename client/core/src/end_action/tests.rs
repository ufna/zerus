use super::*;
use serde_json::json;

const REQUEST: &str = "11111111-1111-4111-8111-111111111111";
const ARCHIVE: &str = "22222222-2222-4222-8222-222222222222";
fn capture(action: Action) -> Captured {
    Captured {
        target: Target {
            computer: "example-peer".into(),
            name: "codex/example/review".into(),
            run_id: "run-1".into(),
            conversation_id: "conversation-1".into(),
        },
        state: match action {
            Action::ForgetArchive { .. } => "archived",
            Action::Archive | Action::Forget => "paused",
            _ => "running",
        }
        .into(),
        action,
    }
}
fn record(c: &Captured) -> Value {
    json!({"name":c.target.name,"run_id":c.target.run_id,"conversation_id":c.target.conversation_id,"state":c.state,"archive_id":c.archive_id().unwrap_or("")})
}
fn exited(value: &Value) -> Exit {
    Exit::Code {
        code: 0,
        stdout: serde_json::to_vec(value).unwrap(),
        stderr: String::new(),
    }
}
fn ends() -> Vec<Action> {
    vec![
        Action::Archive,
        Action::Terminate,
        Action::Kill,
        Action::Forget,
        Action::ForgetBinding,
        Action::ForgetArchive {
            archive_id: ARCHIVE.into(),
        },
        Action::ClearContext {
            dismiss_prompt: true,
        },
    ]
}

#[test]
fn window_is_monotonic_one_use_and_never_releases_before_three_seconds() {
    let at = Instant::now();
    let mut window = Window::new(at);
    assert_eq!(window.remaining(at), Duration::from_secs(3));
    assert!(!window.take_elapsed(at + UNDO_WINDOW - Duration::from_nanos(1)));
    assert!(window.take_elapsed(at + UNDO_WINDOW));
    assert!(!window.take_elapsed(at + Duration::from_secs(60)));
    assert_eq!(
        window.remaining(at + Duration::from_secs(60)),
        Duration::ZERO
    );
}

#[test]
fn undo_close_and_quit_are_terminal_and_delayed_wakes_do_not_restart_the_window() {
    let at = Instant::now();
    for when in [
        Duration::ZERO,
        UNDO_WINDOW - Duration::from_nanos(1),
        UNDO_WINDOW,
    ] {
        let mut window = Window::new(at);
        assert_eq!(
            window.remaining(at + when),
            UNDO_WINDOW.saturating_sub(when)
        );
        window.cancel();
        assert!(!window.take_elapsed(at + Duration::from_secs(60)));
    }
    let mut hidden = Window::new(at);
    assert!(
        hidden.take_elapsed(at + Duration::from_secs(12)),
        "a late native wake uses the original deadline"
    );
}

#[test]
fn only_the_selected_end_actions_with_bounded_known_identity_are_captured() {
    for action in ends() {
        assert!(capture(action).valid());
    }
    for action in [
        Action::Pause,
        Action::Resume,
        Action::Interrupt { turn_started: 1.0 },
        Action::StartFresh,
        Action::CompactContext,
        Action::Restore {
            archive_id: ARCHIVE.into(),
        },
    ] {
        assert!(!capture(action).valid());
    }
    let good = capture(Action::Archive);
    for field in ["computer", "name", "run_id", "conversation_id"] {
        let mut value = serde_json::to_value(&good).unwrap();
        value["target"][field] = json!("x".repeat(1025));
        assert!(!serde_json::from_value::<Captured>(value).unwrap().valid());
        let mut value = serde_json::to_value(&good).unwrap();
        value["target"][field] = json!("x\ny");
        assert!(!serde_json::from_value::<Captured>(value).unwrap().valid());
    }
    let mut missing = good.clone();
    missing.target.run_id.clear();
    assert!(!missing.valid());
    for state in ["unknown", "running", "archived"] {
        let mut changed = good.clone();
        changed.state = state.into();
        assert!(!changed.valid());
    }
    let mut archived = capture(Action::ForgetArchive {
        archive_id: "name-only".into(),
    });
    assert!(!archived.valid());
    archived.action = Action::Forget;
    assert!(!archived.valid());
}

#[test]
fn metadata_reads_skip_history_and_refuse_every_changed_identity_without_a_replacement() {
    for action in ends() {
        let c = capture(action);
        let inv = c.inspection().unwrap();
        assert_eq!(&inv.args[..3], ["@example-peer", "inspect", &c.target.name]);
        assert!(inv
            .args
            .windows(2)
            .any(|a| a == ["--after", &i64::MAX.to_string()]));
        assert!(inv.args.iter().any(|a| a == "--skip-processes"));
        assert_eq!(
            inv.args.iter().any(|a| a == "--archive"),
            c.archive_id().is_some()
        );
        assert!(c.checked_inspection(exited(&record(&c))).is_ok());
        for field in ["name", "run_id", "conversation_id", "state", "archive_id"] {
            let mut changed = record(&c);
            changed[field] = json!("replacement");
            assert!(
                c.checked_inspection(exited(&changed)).is_err(),
                "{field}: {:?}",
                c.action
            );
        }
        // Sending changes activity, never the captured session identity.
        let mut sending = record(&c);
        sending["phase"] = json!("working");
        sending["last_message"] = json!("a new message");
        assert!(c.checked_inspection(exited(&sending)).is_ok());
    }
    let c = capture(Action::Archive);
    let mut opaque = record(&c);
    opaque["current_provider_detail"] = json!("x".repeat(256 * 1024));
    assert!(c.checked_inspection(exited(&opaque)).is_ok());
    assert!(c.checked_inspection(Exit::TimedOut).is_err());
    assert!(c.checked_inspection(exited(&Value::Null)).is_err());
    let mut large = record(&c);
    large["extra"] = json!("x".repeat(METADATA_LIMIT));
    assert!(c.checked_inspection(exited(&large)).is_err());
}

#[test]
fn end_calls_reuse_scoped_envelopes_and_never_name_only_mutations() {
    for action in ends() {
        let c = capture(action.clone());
        let inv = c.invocation(REQUEST).unwrap();
        let body: Value = serde_json::from_slice(inv.stdin.as_deref().unwrap()).unwrap();
        assert_eq!(body["request_id"], REQUEST);
        assert_eq!(body["expected_run_id"], c.target.run_id);
        assert_eq!(body["expected_conversation_id"], c.target.conversation_id);
        assert_eq!(inv.request_id.as_deref(), Some(REQUEST));
        assert_eq!(inv.args[0], "@example-peer");
        if matches!(action, Action::ClearContext { .. }) {
            assert_eq!(inv.args[1], "clear-context");
            assert_eq!(body["dismiss_prompt"], true);
        } else {
            assert_eq!(inv.args[1], "session-action");
            assert_eq!(
                body["action"],
                match action {
                    Action::Archive => "archive",
                    Action::Terminate => "terminate",
                    _ => "forget",
                }
            );
        }
        assert_eq!(body["archive_id"].as_str(), c.archive_id());
        assert!(c.invocation("not-a-uuid").is_err());
    }
    let mut shell = capture(Action::Terminate);
    shell.target.name = "sh/example/shell".into();
    shell.target.conversation_id.clear();
    assert!(
        shell.invocation(REQUEST).is_ok(),
        "an empty native conversation must not erase a known shell run"
    );
    shell.action = Action::ForgetBinding;
    assert!(!shell.valid());
}

#[test]
fn only_exact_completed_receipts_settle_the_action_and_cleanup_evidence_survives() {
    for action in ends() {
        let c = capture(action.clone());
        let inv = c.invocation(REQUEST).unwrap();
        let mut receipt = json!({"name":c.target.name,"run_id":c.target.run_id,"conversation_id":c.target.conversation_id,"request_id":REQUEST,"status":if matches!(action,Action::ClearContext{..}) {"confirmed"}else{"completed"}});
        if let Some(id) = c.archive_id() {
            receipt["archive_id"] = json!(id);
        }
        assert!(matches!(
            c.completed(&inv, exited(&receipt)),
            Outcome::Done { .. }
        ));
        for field in ["name", "run_id", "conversation_id", "request_id", "status"] {
            let mut bad = receipt.clone();
            bad[field] = json!("replacement");
            assert!(!matches!(
                c.completed(&inv, exited(&bad)),
                Outcome::Done { .. }
            ));
        }
        if c.archive_id().is_some() {
            let mut bad = receipt.clone();
            bad["archive_id"] = json!(REQUEST);
            assert!(!matches!(
                c.completed(&inv, exited(&bad)),
                Outcome::Done { .. }
            ));
        }
        assert!(!matches!(
            c.completed(&inv, Exit::TimedOut),
            Outcome::Done { .. }
        ));
        assert!(!matches!(
            c.completed(&inv, exited(&Value::Null)),
            Outcome::Done { .. }
        ));
        if matches!(action, Action::Archive | Action::Terminate) {
            let mut exit = exited(&receipt);
            if let Exit::Code { stderr, .. } = &mut exit {
                *stderr = "ZERUS_WORKTREE_CLEANUP {\"status\":\"kept\",\"path\":\"/srv/example/work\",\"common_dir\":\"/srv/example/.git\"}\n".into();
            }
            assert!(matches!(
                c.completed(&inv, exit),
                Outcome::Done {
                    cleanup: Some(_),
                    ..
                }
            ));
        }
    }
}
