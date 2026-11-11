use super::*;
use crate::desktop::teams::tests::plan_fixture;
#[test]
fn native_arguments_preserve_launch_identity_account_and_role_choices() {
    for agent in ["claude", "codex", "kimi", "dsh", "opencode"] {
        let mut p = plan_fixture();
        p.agent = agent.into();
        p.model = if agent == "dsh" { "" } else { "fixture-model" }.into();
        p.effort = if matches!(agent, "claude" | "codex" | "opencode") {
            "high"
        } else {
            ""
        }
        .into();
        let tree = Worktree {
            path: p.placement.destination.clone(),
            ownership_nonce: "synthetic-nonce".into(),
        };
        let args = arguments::launch(&p, &tree).unwrap();
        assert!(args.windows(2).any(|v| v == ["--launch-id", &p.launch_id]));
        assert!(args.windows(2).any(|v| v == ["--account", &p.account]));
        assert_eq!(
            args.contains(&"-d".into()),
            !matches!(agent, "dsh" | "opencode")
        );
        if agent == "codex" {
            assert!(args.contains(&"model_reasoning_effort=\"high\"".into()));
        }
        if agent == "claude" {
            assert!(args.windows(2).any(|v| v == ["--effort", "high"]));
        }
        if agent == "opencode" {
            assert!(!args.contains(&"--".into()));
        }
        if agent == "kimi" {
            assert!(args.windows(2).any(|v| v == ["--model", "fixture-model"]));
        }
    }
}
#[test]
fn unsupported_native_choices_are_refused_instead_of_ignored() {
    let mut p = plan_fixture();
    let tree = Worktree {
        path: p.placement.destination.clone(),
        ownership_nonce: "nonce".into(),
    };
    p.agent = "kimi".into();
    p.effort = "high".into();
    assert!(arguments::launch(&p, &tree).is_err());
    p.agent = "dsh".into();
    p.effort.clear();
    p.model = "fixture-model".into();
    assert!(arguments::launch(&p, &tree).is_err());
}
#[test]
fn usage_inspection_checks_identity_authentication_and_reset_units() {
    let v = json!({"id":"a","provider":"claude","auth_status":"signed_in","status":"rate_limited","windows":[{"used_percent":100,"resets_at":"2030-01-01T00:00:00Z"}]});
    let a = arguments::usage(&v, "claude", "a").unwrap();
    assert!(a.signed_in);
    assert_eq!(a.windows[0].resets_at, Some(1893456000));
    assert!(arguments::usage(&v, "codex", "a").is_err());
    assert!(arguments::usage(&v, "claude", "b").is_err());
    let mut v = v;
    v["auth_status"] = json!("signed_out");
    v["status"] = json!("signed_out");
    assert!(!arguments::usage(&v, "claude", "a").unwrap().signed_in);
    v["windows"][0]["used_percent"] = Value::Null;
    assert!(arguments::usage(&v, "claude", "a").is_err());
}
#[test]
fn unknown_authentication_is_not_an_unmeasured_signed_in_account() {
    let v = json!({"id":"a","provider":"codex","status":"unavailable","windows":[]});
    assert!(!arguments::usage(&v, "codex", "a").unwrap().signed_in);
}
#[test]
fn worktree_arguments_use_only_the_saved_generated_placement() {
    let p = plan_fixture();
    let args = arguments::worktree(&p);
    for (key, value) in [
        ("--path", &p.placement.source),
        ("--branch", &p.placement.branch),
        ("--destination", &p.placement.destination),
        ("--base", &p.base_commit),
    ] {
        assert!(args.windows(2).any(|v| v == [key, value]));
    }
    assert!(!args
        .iter()
        .any(|v| v == "reset" || v == "push" || v == "fetch" || v == "init"));
}
#[test]
fn repository_observation_does_not_initialize_an_ordinary_folder() {
    let temp = tempfile::tempdir().unwrap();
    assert!(observation::common_dir(temp.path()).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn a_verified_api_key_without_quota_windows_is_available_as_an_unmeasured_account() {
    let v = json!({"id":"a","provider":"codex","status":"unavailable","identity":{"auth_method":"apiKey"},"windows":[]});
    let account = arguments::usage(&v, "codex", "a").unwrap();
    assert!(account.signed_in);
    assert!(account.windows.is_empty());
}
