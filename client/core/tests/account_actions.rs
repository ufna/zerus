#![cfg(unix)]
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zerus_client_core::{
    account_actions::{Binding, Kind},
    ComputerId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn setup() -> (Scratch, Binding) {
    let dir = std::env::temp_dir().join(format!(
        "zerus-account-edit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_hgs_account_edits.py"),
        dir.join("hgs"),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir.join("hgs"), fs::Permissions::from_mode(0o700)).unwrap();
    let catalog = json!({"revision":"r1","profiles":[{"id":"work","provider":"codex","home":"/accounts/example","label":"Example","auth_revision":"a1"}]});
    fs::write(dir.join("catalog.json"), catalog.to_string()).unwrap();
    let binding = Binding::capture(
        ComputerId("studio".into()),
        "11111111-1111-4111-8111-111111111111".into(),
        &catalog,
        "work",
    )
    .unwrap();
    (Scratch(dir), binding)
}
#[test]
fn each_catalog_edit_reads_fresh_then_dispatches_exactly_once() {
    for kind in [Kind::Rename, Kind::Default, Kind::Remove] {
        let (scratch, binding) = setup();
        let result = binding
            .execute(&scratch.0.join("hgs"), kind, "New account")
            .unwrap();
        assert_eq!(result["revision"], "r2");
        let calls: Vec<Value> = fs::read_to_string(scratch.0.join("calls.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0][4], "ls");
        match kind {
            Kind::Rename => assert_eq!(result["profiles"][0]["label"], "New account"),
            Kind::Default => assert_eq!(result["profiles"][0]["is_default"], true),
            Kind::Remove => assert_eq!(result["profiles"], json!([])),
        }
    }
}
#[test]
fn changed_catalog_refuses_before_the_effect() {
    let (scratch, binding) = setup();
    let mut catalog: Value =
        serde_json::from_slice(&fs::read(scratch.0.join("catalog.json")).unwrap()).unwrap();
    catalog["revision"] = json!("new");
    fs::write(scratch.0.join("catalog.json"), catalog.to_string()).unwrap();
    assert!(binding
        .execute(&scratch.0.join("hgs"), Kind::Remove, "")
        .is_err());
    assert_eq!(
        fs::read_to_string(scratch.0.join("calls.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
#[test]
fn an_uncertain_exit_never_replays_the_mutation() {
    let (scratch, binding) = setup();
    fs::write(scratch.0.join("uncertain"), b"fixture").unwrap();
    assert!(binding
        .execute(&scratch.0.join("hgs"), Kind::Rename, "New account")
        .is_err());
    assert_eq!(
        fs::read_to_string(scratch.0.join("calls.jsonl"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(binding
        .execute(&scratch.0.join("hgs"), Kind::Rename, "New account")
        .is_err());
    assert_eq!(
        fs::read_to_string(scratch.0.join("calls.jsonl"))
            .unwrap()
            .lines()
            .count(),
        3,
        "second explicit call performs only its read and refuses the stale revision"
    );
}
