//! The create binding against the command line itself: the `hgs` built from
//! this source (`HGS_TEST_BIN`), in a private home, configuration and state,
//! with nothing of the caller's environment. The catalog is read and a create
//! is sent through the core's own functions, as the desktop does: a create
//! whose template changed since it was shown is refused before anything is
//! written; once shown again, the team gets the permissions the create
//! carried, not the template's.
//!
//! Turning the preview on, saving a template and creating are person-only:
//! the command line refuses them inside an agent's process tree. From an
//! agent session, run this suite under `systemd-run --user`.
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zerus_client_core::actions::{Exit, Invocation};
use zerus_client_core::teams::action::{run_action, TeamCall, TeamOutcome};
use zerus_client_core::teams::catalog::{form_catalog, saved_receipt};

const MACHINE: &str = "00000000-0000-4000-8000-0000000000aa";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The candidate command line, as the drive-folder tests take it.
fn candidate_cli() -> PathBuf {
    let value = std::env::var_os("HGS_TEST_BIN").unwrap_or_else(|| {
        panic!(
            "set HGS_TEST_BIN to the hgs built from this source; scripts/ci/client-native.sh does"
        )
    });
    let path = PathBuf::from(value);
    let metadata = std::fs::metadata(&path)
        .unwrap_or_else(|error| panic!("HGS_TEST_BIN {}: {error}", path.display()));
    assert!(
        path.is_absolute() && metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        "HGS_TEST_BIN is not an absolute executable: {}",
        path.display()
    );
    path.canonicalize().unwrap()
}

fn shell_word(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// A private `hgs`: its own home, configuration and state folders (made
/// before the first call: the caller proof reads the state root), and only
/// PATH from the test's environment.
struct Private {
    dir: PathBuf,
    hgs: PathBuf,
}

impl Private {
    fn new() -> Self {
        let binary = candidate_cli();
        let dir = std::env::temp_dir().join(format!("zerus-team-create-{}", uuid::Uuid::new_v4()));
        for folder in ["config", "state"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
        }
        let hgs = dir.join("hgs-private");
        let script = format!(
            "#!/bin/sh\nexec env -i PATH=\"$PATH\" HOME={0} HGS_CONFIG_DIR={1} HGS_STATE_DIR={2} HGS_SELF='example-local' HGS_PEERS='' {3} \"$@\"\n",
            shell_word(&dir),
            shell_word(&dir.join("config")),
            shell_word(&dir.join("state")),
            shell_word(&binary),
        );
        crate::write_executable(&hgs, script);
        Self { dir, hgs }
    }

    /// One answer of the command line; a refusal is the test's failure, with
    /// the person-only hint when the caller is not a person.
    fn read(&self, args: &[&str], stdin: Option<Value>) -> Value {
        let invocation = Invocation {
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            stdin: stdin.map(|value| value.to_string().into_bytes()),
            timeout: Duration::from_secs(30),
            request_id: None,
        };
        let Exit::Code {
            code,
            stdout,
            stderr,
        } = zerus_client_core::run(&self.hgs, &invocation)
        else {
            panic!("{args:?} did not end normally");
        };
        let reply: Value = serde_json::from_slice(&stdout).unwrap_or_else(|_| {
            panic!(
                "{args:?} exit {code}: {}{stderr}",
                String::from_utf8_lossy(&stdout)
            )
        });
        assert!(
            code == 0 && reply["ok"] == true,
            "{args:?} exit {code}: {reply}\n(person-only commands need a caller outside any agent's process tree; from an agent session run this suite under systemd-run --user)"
        );
        reply
    }

    /// The form's catalog, read through the core as the desktop reads it.
    fn template(&self, id: &str) -> Value {
        let form = form_catalog(&self.read(&["team", "catalog", "show", "--json"], None)).unwrap();
        form["catalog"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no template {id}"))
    }

    fn save(&self, row: &Value) {
        // As the form does: the template's own fields only, never the row's
        // digest or the catalog read's facts.
        let saved = Value::Object(
            row.as_object()
                .unwrap()
                .iter()
                .filter(|(key, _)| {
                    [
                        "id",
                        "name",
                        "description",
                        "lead",
                        "roster",
                        "default_pipeline",
                        "settings",
                    ]
                    .contains(&key.as_str())
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let reply = self.read(&["team", "template", "save", "--json"], Some(saved));
        saved_receipt(&reply, row["id"].as_str().unwrap()).unwrap();
    }

    fn create(&self, team: &str, digest: &str, merge_accepted: bool) -> TeamOutcome {
        let call: TeamCall = serde_json::from_value(json!({
            "team": team, "team_name": "Confirmed", "request_id": uuid::Uuid::new_v4().to_string(),
            "action": {
                "kind": "create", "template": "confirmed", "project": "example-project", "name": "Confirmed", "goal": "",
                "home": MACHINE, "integration_branch": "zerus/team/confirmed", "base_commit": COMMIT, "integration": "branch",
                "expected_template": digest,
                "delegations": {"accept_results": true, "answer_members": true, "confirm_trust": true, "resume_exited": true, "merge_accepted": merge_accepted},
            },
        }))
        .unwrap();
        run_action(&self.hgs, &call)
    }
}

impl Drop for Private {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn the_command_line_refuses_a_changed_template_and_takes_the_shown_permissions() {
    let hgs = Private::new();
    hgs.read(&["team", "preview", "on", "--json"], None);
    // The person's own template, merging accepted work only with approval.
    let mut template = hgs.template("bugfix");
    template["id"] = json!("confirmed");
    template["name"] = json!("Confirmed");
    template["settings"]["delegations"]["merge_accepted"] = json!(false);
    hgs.save(&template);
    let shown = hgs.template("confirmed");
    let shown_digest = shown["digest"]
        .as_str()
        .expect("catalog show gives a digest")
        .to_owned();
    assert_eq!(shown["settings"]["delegations"]["merge_accepted"], false);
    // Another window replaces it under the same id.
    template["settings"]["delegations"]["merge_accepted"] = json!(true);
    hgs.save(&template);
    let first = "00000000-0000-4000-8000-00000000c0de";
    match hgs.create(first, &shown_digest, false) {
        TeamOutcome::Failed { code, detail, .. } => {
            assert_eq!(code, "template_changed");
            assert_eq!(
                detail,
                "The template changed since it was shown. Review it again."
            );
        }
        other => panic!("a changed template was not refused: {other:?}"),
    }
    let list = hgs.read(&["team", "list", "--json"], None);
    assert_eq!(list["rows"], json!([]), "nothing was written");
    // Shown again: the template now merges by itself; the person keeps its
    // approval for this team, without saving a template.
    let now = hgs.template("confirmed");
    assert_eq!(now["settings"]["delegations"]["merge_accepted"], true);
    assert_ne!(now["digest"], json!(shown_digest));
    let second = "00000000-0000-4000-8000-00000000c0df";
    let outcome = hgs.create(second, now["digest"].as_str().unwrap(), false);
    assert!(matches!(outcome, TeamOutcome::Done { .. }), "{outcome:?}");
    let status = hgs.read(&["team", "status", "--team", second, "--json"], None);
    assert_eq!(
        status["team"]["settings"]["delegations"],
        json!({"accept_results": true, "answer_members": true, "confirm_trust": true, "resume_exited": true, "merge_accepted": false})
    );
}
