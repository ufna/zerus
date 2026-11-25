//! The ordinary CLI cannot create a team without native Person confirmation,
//! even with a current template digest and renderer `confirmed: true`.
//! The source-built `HGS_TEST_BIN` runs only against private fixture data.
//!
//! The admitted-writer template and permission positives remain in
//! `src/teams/cli/tests/create_binding.rs`. No Person-only setup command or
//! detached process is needed to exercise this production refusal.
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zerus_client_core::teams::action::{run_action, TeamCall, TeamOutcome};

const MACHINE: &str = "00000000-0000-4000-8000-0000000000aa";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
// Pinned by the CLI producer's catalog::digest tests, not computed by a client.
const SHOWN_DIGEST: &str =
    include_str!("../../../../crates/team-plan/fixtures/template-create-digest.txt");

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

struct Private {
    dir: tempfile::TempDir,
    hgs: PathBuf,
}

impl Private {
    fn new() -> Self {
        let binary = candidate_cli();
        let dir = tempfile::tempdir().unwrap();
        for folder in ["config", "state/teams"] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        // Input fixture only: enable the private preview without asking a
        // Person-only setup command to establish the test runner's identity.
        std::fs::write(dir.path().join("state/teams/preview"), "on\n").unwrap();
        let hgs = dir.path().join("hgs-private");
        let script = format!(
            "#!/bin/sh\nexec env -i PATH=\"$PATH\" HOME={0} HGS_CONFIG_DIR={1} HGS_STATE_DIR={2} HGS_SELF='example-local' HGS_PEERS='' {3} \"$@\"\n",
            shell_word(dir.path()),
            shell_word(&dir.path().join("config")),
            shell_word(&dir.path().join("state")),
            shell_word(&binary),
        );
        crate::write_executable(&hgs, script);
        Self { dir, hgs }
    }

    fn create(&self, team: &str, digest: &str) -> TeamOutcome {
        let call: TeamCall = serde_json::from_value(json!({
            "team": team, "team_name": "Confirmed", "request_id": uuid::Uuid::new_v4().to_string(),
            "confirmed": true,
            "action": {
                "kind": "create", "template": "bugfix", "project": "example-project", "name": "Confirmed", "goal": "",
                "home": MACHINE, "integration_branch": "zerus/team/confirmed", "base_commit": COMMIT, "integration": "branch",
                "expected_template": digest,
                "delegations": {"accept_results": true, "answer_members": true, "confirm_trust": true, "resume_exited": true, "merge_accepted": false},
            },
        }))
        .unwrap();
        run_action(&self.hgs, &call)
    }
}

#[test]
fn the_command_line_refuses_creation_without_native_person_confirmation() {
    let hgs = Private::new();
    let stale = "0".repeat(64);
    assert_ne!(SHOWN_DIGEST.trim(), stale);
    for (team, digest) in [
        ("00000000-0000-4000-8000-00000000c0de", stale.as_str()),
        ("00000000-0000-4000-8000-00000000c0df", SHOWN_DIGEST.trim()),
    ] {
        match hgs.create(team, digest) {
            TeamOutcome::Failed { code, .. } => assert_eq!(code, "person_gesture_required"),
            other => panic!("a create without native confirmation was not refused: {other:?}"),
        }
        let entries: Vec<_> = std::fs::read_dir(hgs.dir.path().join("state/teams"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["preview"], "no team store or receipt was created");
    }
}
