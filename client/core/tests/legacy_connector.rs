#![cfg(unix)]
use std::{
    fs,
    path::{Path, PathBuf},
};
use zerus_client_core::mobile::legacy::{self, windows};
struct Fixture {
    root: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "legacy-manager-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let exe = root.join("connector");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_legacy_manager.py"),
            &exe,
        )
        .unwrap();
        fs::set_permissions(exe, fs::Permissions::from_mode(0o700)).unwrap();
        let config = root.join("config.json");
        fs::write(&config,serde_json::json!({"configured":true,"running":false,"revision":"a".repeat(64),"generation":null,"server_url":"https://relay.example.invalid","state_dir":"example-receipts","startup_at_login":false}).to_string()).unwrap();
        Self { root, config }
    }
    fn exe(&self) -> PathBuf {
        self.root.join("connector")
    }
    fn calls(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root.join("calls"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn windows_save_uses_stdin_after_exact_revision_and_never_echoes_the_token() {
    let f = Fixture::new();
    let old = windows::status(&f.exe(), &f.config).unwrap();
    let (new, _) = windows::apply(
        &f.exe(),
        &f.config,
        &old,
        "save",
        Some(legacy::Edit {
            relay: "https://other.example.invalid".into(),
            replacement: "transient-fixture".into(),
            directory: PathBuf::new(),
        }),
    )
    .unwrap();
    assert_eq!(new.server_url, "https://other.example.invalid");
    let calls = f.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1]["action"], "status");
    assert_eq!(
        calls[2]["keys"],
        serde_json::json!(["node_token", "revision", "server_url"])
    );
    assert!(!fs::read_to_string(f.root.join("calls"))
        .unwrap()
        .contains("transient-fixture"));
}
#[test]
fn windows_start_and_generation_bound_stop_use_existing_manager_once() {
    let f = Fixture::new();
    let old = windows::status(&f.exe(), &f.config).unwrap();
    let (running, _) = windows::apply(&f.exe(), &f.config, &old, "start", None).unwrap();
    assert!(running.running);
    let (stopped, _) = windows::apply(&f.exe(), &f.config, &running, "stop", None).unwrap();
    assert!(!stopped.running);
    assert_eq!(
        f.calls().iter().filter(|v| v["action"] == "stop").count(),
        1
    );
}
#[test]
fn a_stale_snapshot_refuses_before_any_effect() {
    let f = Fixture::new();
    let old = windows::status(&f.exe(), &f.config).unwrap();
    let mut next: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config).unwrap()).unwrap();
    next["revision"] = serde_json::json!("b".repeat(64));
    fs::write(&f.config, next.to_string()).unwrap();
    assert_eq!(
        windows::apply(&f.exe(), &f.config, &old, "start", None)
            .err()
            .unwrap(),
        legacy::CHANGED
    );
    assert!(f.calls().iter().all(|v| v["action"] == "status"));
}
#[test]
fn unknown_effect_and_untrusted_status_never_surface_output_or_replay() {
    let f = Fixture::new();
    let old = windows::status(&f.exe(), &f.config).unwrap();
    fs::write(f.root.join("mode"), "unknown").unwrap();
    assert_eq!(
        windows::apply(&f.exe(), &f.config, &old, "start", None)
            .err()
            .unwrap(),
        windows::UNCONFIRMED
    );
    assert_eq!(
        f.calls().iter().filter(|v| v["action"] == "start").count(),
        1
    );
    fs::write(f.root.join("mode"), "bad-status").unwrap();
    assert_eq!(
        windows::status(&f.exe(), &f.config).err().unwrap(),
        windows::UNCONFIRMED
    );
}
