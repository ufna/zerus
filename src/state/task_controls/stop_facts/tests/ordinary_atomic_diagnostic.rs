//! One real ordinary producer, not a separately created publication primitive.
//! These test parts are inert after decomposition and never become Captured
//! authority. Native diagnostic success is not writer compatibility acceptance.
#![cfg(windows)]
use super::*;
use crate::{
    platform::persist_diagnostic::{self, handle_info, Scope},
    state::repositories::directory::capture_diagnostic::{Acquisition, Cohort, Held, QueryError},
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

const JSON_BYTES: usize = 64 * 1024;
const REPORT_ROWS: usize = 128;

fn observer_summary(observation: &serde_json::Value) -> serde_json::Value {
    let invalid = observation["invalid"]
        .as_str()
        .map(|s| s.chars().take(256).collect::<String>());
    serde_json::json!({
        "summary_only": true, "entries": observation["entries"].as_u64(),
        "event_count": observation["events"].as_array().map_or(0, Vec::len),
        "first_error": observation["first_error"].as_i64(), "invalid": invalid,
        "released": observation["released"].as_bool(), "success": observation["success"].as_bool(),
        "success_before_deadline": observation["success_before_deadline"].as_bool(),
    })
}
fn bounded_json(evidence: &serde_json::Value) -> io::Result<Vec<u8>> {
    let full = serde_json::to_vec(evidence).map_err(io::Error::other)?;
    let row_count = [
        evidence["inventory"].as_array(),
        evidence["verification"]["rows"].as_array(),
        evidence["verification"]["phases"].as_array(),
        evidence["observation"]["events"].as_array(),
    ]
    .into_iter()
    .flatten()
    .map(Vec::len)
    .sum::<usize>();
    if full.len() <= JSON_BYTES && row_count <= REPORT_ROWS {
        return Ok(full);
    }
    let verification = &evidence["verification"];
    let rows = verification["rows"].as_array();
    let differences: Vec<_> = rows
        .into_iter()
        .flatten()
        .filter(|row| {
            !row["query_error"].is_null()
                || row["changed"].as_array().is_some_and(|x| !x.is_empty())
        })
        .take(4)
        .collect();
    let fallback = serde_json::json!({
        "case": evidence["case"], "status": "inconclusive",
        "producer_ok": evidence["producer_ok"], "post_check": evidence["post_check"],
        "overflow": {"kind": "report_bounds", "byte_limit": JSON_BYTES, "required_bytes": full.len(),
            "row_limit": REPORT_ROWS, "required_rows": row_count,
            "full_inventory_omitted": true, "full_rows_omitted": true},
        "observation": observer_summary(&evidence["observation"]),
        "verification_summary": {"phases":verification["phases"],
            "row_overflow": verification["row_overflow"], "phase_overflow": verification["phase_overflow"],
            "seen_rows": verification["seen_rows"], "r_held": verification["r_held"],
            "retained_held": verification["retained_held"],
            "released_by_diagnostic": verification["released_by_diagnostic"]},
        "first_differences": differences,
    });
    let bytes = serde_json::to_vec(&fallback).map_err(io::Error::other)?;
    if bytes.len() <= JSON_BYTES {
        return Ok(bytes);
    }
    // Even unexpected report growth keeps bounded phase/observer summaries.
    let phases: Vec<_> = verification["phases"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .map(|row| {
            serde_json::json!({
                "phase": row["phase"].as_str().map(|s| s.chars().take(32).collect::<String>()),
                "expected": row["expected"].as_u64(), "checked": row["checked"].as_u64(),
                "not_checked": row["not_checked"].as_u64(), "ok": row["ok"].as_bool(),
            })
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "case":evidence["case"].as_str().map(|s| s.chars().take(32).collect::<String>()),
        "status":"inconclusive", "overflow":{"kind":"summary_bytes", "limit":JSON_BYTES},
        "observation":observer_summary(&evidence["observation"]),
        "phase_summary":phases, "full_verification_omitted":true,
    }))
    .map_err(io::Error::other)
}
fn classification(
    observer_positive: bool,
    post_ok: bool,
    verification_complete: bool,
) -> &'static str {
    if observer_positive && post_ok && verification_complete {
        "measured_R_cohort"
    } else {
        "inconclusive"
    }
}

fn emit_then_assert_post(
    output: &mut impl Write,
    evidence: &serde_json::Value,
    post_check: io::Result<()>,
) -> io::Result<()> {
    let written = (|| {
        let bytes = bounded_json(evidence)?;
        output.write_all(b"G1 ordinary-atomic diagnostic: ")?;
        output.write_all(&bytes)?;
        output.write_all(b"\n")?;
        output.flush()
    })();
    // This is the ORIGINAL assertion, after report write+flush. Output failure
    // cannot turn it into Ok or bypass it. The caller checks `written` only
    // after all the original producer/read-back assertions have run.
    post_check.unwrap();
    written
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
    let post_check = cohort.borrow().verify_inventory();
    let verification = cohort.borrow().verification_log();
    // Use the real bounded test output stream so the automatic Windows job
    // retains an inconclusive/successful measurement even for a passing test.
    let evidence = serde_json::json!({
        "case": format!("{case:?}"),
        "status": classification(report.positive(), post_check.is_ok(), verification.complete()),
        "producer_ok": result.is_ok(),
        "observation": report,
        "inventory": cohort.borrow().inventory(),
        "verification": verification,
        "post_check": {"ok":post_check.is_ok(), "error":post_check.as_ref().err().map(QueryError::of)},
    });
    let written = emit_then_assert_post(&mut io::stdout().lock(), &evidence, post_check);
    if result.is_ok() {
        assert_eq!(fs::read(&record).unwrap(), CONTENT.as_bytes());
        // A new read-back is AFTER the original producer has settled, never a
        // substitute source/reader inside the observed retry.
        let info = handle_info(&fs::File::open(&record).unwrap()).unwrap();
        assert!(!info.directory);
    }
    // The original ordinary-writer test remains unchanged and still mandatory.
    // A failed continuation is a diagnostic evidence gap, never a waived gate.
    written.unwrap();
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

#[cfg(test)]
mod report_tests {
    use super::*;
    #[derive(Default)]
    struct Output {
        bytes: Vec<u8>,
        flushes: usize,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    fn evidence() -> serde_json::Value {
        serde_json::json!({"case":"fixture", "status":"inconclusive", "producer_ok":false,
            "observation":{"entries":0,"events":[],"invalid":"fixture","released":false,
                "success":false,"first_error":null,"success_before_deadline":false},
            "inventory":[], "verification":{"rows":[],"phases":[]}, "post_check":{"ok":false}})
    }
    #[test]
    fn zero_row_observer_is_written_and_flushed_before_original_post_check_panic() {
        let mut output = Output::default();
        let value = evidence();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_then_assert_post(
                &mut output,
                &value,
                Err(io::Error::other("original post-check fixture")),
            )
            .unwrap();
        }));
        assert!(panic.is_err());
        assert_eq!(output.flushes, 1);
        let text = String::from_utf8(output.bytes).unwrap();
        let row: serde_json::Value = serde_json::from_str(
            text.strip_prefix("G1 ordinary-atomic diagnostic: ")
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert_eq!(row["observation"]["entries"], 0);
        assert_eq!(row["observation"]["events"], serde_json::json!([]));
        assert_eq!(row["status"], "inconclusive");
    }
    #[test]
    fn callback_mismatch_and_query_failure_remain_visible_before_post_panic() {
        for failure in [
            serde_json::json!({"changed":["attributes"],"before":{"attributes":1},"after":{"attributes":2},"query_error":null}),
            serde_json::json!({"changed":[],"before":{"length":4},"after":null,"query_error":{"code":5}}),
        ] {
            let mut value = evidence();
            let mut row = failure;
            row["phase"] = "before_release".into();
            row["origin"] = "fixture-ancestor".into();
            row["cohort"] = "retained".into();
            value["verification"]["rows"] = serde_json::json!([row]);
            let mut output = Output::default();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = emit_then_assert_post(
                    &mut output,
                    &value,
                    Err(io::Error::other("original inventory refusal")),
                );
            }));
            assert!(panic.is_err());
            assert_eq!(output.flushes, 1);
            assert!(String::from_utf8(output.bytes)
                .unwrap()
                .contains("before_release"));
        }
    }
    #[test]
    fn row_or_json_overflow_is_explicit_bounded_inconclusive_and_keeps_summary() {
        for rows in [false, true] {
            let mut value = evidence();
            value["status"] = "measured_R_cohort".into();
            if rows {
                value["inventory"] =
                    serde_json::json!(vec![serde_json::json!({}); REPORT_ROWS + 1]);
            } else {
                value["observation"]["invalid"] = "x".repeat(JSON_BYTES * 2).into();
            }
            let bytes = bounded_json(&value).unwrap();
            assert!(bytes.len() <= JSON_BYTES);
            let row: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(row["status"], "inconclusive");
            assert!(row.get("overflow").is_some());
            assert_eq!(row["observation"]["entries"], 0);
            assert_eq!(row["observation"]["event_count"], 0);
        }
    }
    #[test]
    fn a_positive_observer_cannot_override_failed_or_incomplete_verification() {
        assert_eq!(classification(true, true, true), "measured_R_cohort");
        for guards in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
        ] {
            assert_eq!(classification(guards.0, guards.1, guards.2), "inconclusive");
        }
    }
    #[test]
    fn failed_output_never_bypasses_original_post_check_assertion() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("output fixture"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush fixture"))
            }
        }
        let panic = std::panic::catch_unwind(|| {
            let _ = emit_then_assert_post(
                &mut Broken,
                &evidence(),
                Err(io::Error::other("original post-check fixture")),
            );
        })
        .unwrap_err();
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap();
        assert!(message.contains("original post-check fixture"));
    }
}
