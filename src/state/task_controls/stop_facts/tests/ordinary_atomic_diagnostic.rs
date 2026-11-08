//! One real ordinary producer, not a separately created publication primitive.
//! These test parts are inert after decomposition and never become Captured
//! authority. Native diagnostic success is not writer compatibility acceptance.
#![cfg(windows)]
use super::*;
use crate::{
    platform::persist_diagnostic::{self, handle_info, Scope},
    state::repositories::directory::capture_diagnostic::{Acquisition, Cohort, Held},
};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    time::Instant,
};

const CONTENT: &str = r#"{"state":"live"}"#;
#[derive(Clone, Copy, Debug)]
enum Case {
    Fresh,
    Reopened,
    ExtraAliases,
    NoDrop,
}

fn ordinary(case: Case) {
    let dir = fixture();
    let guard = writer(dir.path());
    let publisher = Publisher::open_enabled(dir.path(), &guard, &executor(), true).unwrap();
    publisher.publish(&fact(2)).unwrap();
    let publisher = if matches!(case, Case::Fresh) {
        publisher
    } else {
        drop(publisher);
        Publisher::open_enabled(dir.path(), &guard, &executor(), false).unwrap()
    };
    let captured = publisher.capture().unwrap();
    drop(guard);
    captured.verify().unwrap();
    let root = captured.root.object().unwrap();
    let root = persist_diagnostic::Object {
        volume: root.0,
        file: root.1,
    };
    // Capture state is deliberately consumed. There is NO reconstruction or
    // actor import after the diagnostic removes a namespace pin.
    let Captured {
        root: root_dir,
        directory,
        name: _,
        snapshot,
        identity: _,
        writer: _,
        fact_file,
        writer_file,
        bytes: _,
        digest: _,
    } = captured;
    let _snapshot = snapshot;
    let d_acquisition = if matches!(case, Case::Fresh) {
        Acquisition::CreatedDirectory
    } else {
        Acquisition::OpenedDirectory
    };
    let mut handles = root_dir
        .into_capture_diagnostic_parts("capture-root", Acquisition::OpenedDirectory)
        .unwrap();
    handles.extend(
        directory
            .into_capture_diagnostic_parts("capture-facts", d_acquisition)
            .unwrap(),
    );
    handles.push(
        Held::new(
            fact_file,
            "capture-fact-file".into(),
            Acquisition::PrivateReader,
        )
        .unwrap(),
    );
    handles.push(
        Held::new(
            writer_file,
            "capture-independent-writer-file".into(),
            Acquisition::PrivateReader,
        )
        .unwrap(),
    );
    if matches!(case, Case::ExtraAliases) {
        let extra = Directory::open(dir.path()).unwrap();
        let extra_parts = extra
            .into_capture_diagnostic_parts("extra-root-open", Acquisition::OpenedDirectory)
            .unwrap();
        let alias = extra_parts
            .iter()
            .find(|h| h.inventory.native.object == root)
            .unwrap()
            .duplicate_root_alias(root)
            .unwrap();
        handles.extend(extra_parts);
        handles.push(alias);
    }
    let expected = match case {
        Case::Fresh => 1,
        Case::Reopened | Case::NoDrop => 2,
        Case::ExtraAliases => 4,
    };
    let cohort = Rc::new(RefCell::new(Cohort::new(root, handles, expected).unwrap()));
    let record = dir.path().join("run.json");
    assert!(!record.exists());
    let release = if matches!(case, Case::NoDrop) {
        None
    } else {
        let cohort = cohort.clone();
        Some(
            Box::new(move |until| cohort.borrow_mut().release_all_r(until))
                as Box<dyn FnMut(Instant) -> io::Result<()>>,
        )
    };
    let scope = Scope::install(record.clone(), CONTENT.as_bytes().to_vec(), release).unwrap();
    // EXACT maintained production producer. It alone allocates the source.
    let result = storage::atomic(&record, CONTENT);
    let report = scope.report();
    cohort.borrow().verify_inventory().unwrap();
    // Use the real bounded test output stream so the automatic Windows job
    // retains an inconclusive/successful measurement even for a passing test.
    let evidence = serde_json::json!({
        "case": format!("{case:?}"),
        "status": if report.positive() { "measured_R_cohort" } else { "inconclusive" },
        "producer_ok": result.is_ok(),
        "observation": report,
        "inventory": cohort.borrow().inventory(),
    });
    writeln!(
        io::stdout().lock(),
        "G1 ordinary-atomic diagnostic: {evidence}"
    )
    .unwrap();
    if result.is_ok() {
        assert_eq!(fs::read(&record).unwrap(), CONTENT.as_bytes());
        // A new read-back is AFTER the original producer has settled, never a
        // substitute source/reader inside the observed retry.
        let info = handle_info(&fs::File::open(&record).unwrap()).unwrap();
        assert!(!info.directory);
    }
    // The original ordinary-writer test remains unchanged and still mandatory.
    // A failed continuation is a diagnostic evidence gap, never a waived gate.
    drop(scope);
    drop(cohort);
}
#[test]
fn g1_ordinary_atomic_fresh_r_cohort() {
    ordinary(Case::Fresh);
}
#[test]
fn g1_ordinary_atomic_reopened_child_r_alias() {
    ordinary(Case::Reopened);
}
#[test]
fn g1_ordinary_atomic_extra_r_aliases() {
    ordinary(Case::ExtraAliases);
}
#[test]
fn g1_ordinary_atomic_no_drop_reference() {
    ordinary(Case::NoDrop);
}
