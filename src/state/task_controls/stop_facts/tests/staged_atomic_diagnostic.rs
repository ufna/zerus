//! Two NEW disposable staged-persist cases, not full ordinary-writer acceptance.
#![cfg(windows)]
use super::*;
use crate::{
    platform::persist_diagnostic::{EntryCallback, Scope},
    state::repositories::directory::capture_diagnostic::staged_probe::{
        Evidence, Failure, StagedProbe,
    },
};
use serde::Serialize;
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    time::Instant,
};

const CONTENT: &str = r#"{"state":"live"}"#;
const BYTES_LIMIT: usize = 64 * 1024;
const ROWS_LIMIT: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    Fresh,
    NoDrop,
}
#[derive(Clone, Copy, Debug, Serialize)]
struct SetupError {
    stage: &'static str,
    detail: Option<Failure>,
}
struct Fixture {
    probe: Rc<RefCell<StagedProbe>>,
    // Exact immutable evidence consumed after Captured::verify; never restored
    // as Captured, handed to an actor, or serialized as task/content/path data.
    _snapshot: Snapshot,
    _identity: platform::FileIdentity,
    _writer: platform::FileIdentity,
    bytes: usize,
    digest: [u8; 32],
    // Field drop order: release probe/original handles BEFORE TempDir cleanup.
    directory: tempfile::TempDir,
}
fn prepare() -> std::result::Result<Fixture, SetupError> {
    // No policy argument: BOTH cases use this SAME fresh construction.
    let directory = fixture();
    let guard = writer(directory.path());
    let publisher =
        Publisher::open_enabled(directory.path(), &guard, &executor(), true).map_err(|_| {
            SetupError {
                stage: "fresh_publisher",
                detail: None,
            }
        })?;
    publisher.publish(&fact(2)).map_err(|_| SetupError {
        stage: "publish_fixture_fact",
        detail: None,
    })?;
    let captured = publisher.capture().map_err(|_| SetupError {
        stage: "capture",
        detail: None,
    })?;
    drop(guard);
    captured.verify().map_err(|_| SetupError {
        stage: "full_captured_verify",
        detail: None,
    })?;
    let Captured {
        root,
        directory: facts,
        name: _,
        snapshot,
        identity,
        writer,
        fact_file,
        writer_file,
        bytes,
        digest,
    } = captured;
    let probe =
        StagedProbe::fresh(root, facts, fact_file, writer_file).map_err(|detail| SetupError {
            stage: "consume_verified_originals",
            detail: Some(detail),
        })?;
    Ok(Fixture {
        directory,
        probe: Rc::new(RefCell::new(probe)),
        _snapshot: snapshot,
        _identity: identity,
        _writer: writer,
        bytes,
        digest,
    })
}
fn classification(policy: Policy, witness: bool, producer_ok: bool) -> &'static str {
    match (policy, witness, producer_ok) {
        (Policy::Fresh, true, true) => "staged_persist_cohort_witness",
        (Policy::NoDrop, _, true) => "staged_inconclusive_unexpected_no_drop_success",
        _ => "staged_inconclusive",
    }
}
fn reference_failure(
    policy: Policy,
    producer_ok: bool,
    first_error: Option<i64>,
    entries: Option<u64>,
    entry_complete: bool,
    physical_drop: bool,
) -> bool {
    policy == Policy::NoDrop
        && !producer_ok
        && first_error == Some(32)
        && entries == Some(1)
        && entry_complete
        && !physical_drop
}
fn bounded_record(row: &serde_json::Value) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(row).map_err(io::Error::other)?;
    let evidence = &row["staged"];
    let rows = [
        evidence["pre_creation"].as_array(),
        evidence["post_staging"].as_array(),
        evidence["checks"].as_array(),
        evidence["stages"].as_array(),
        row["observation"]["events"].as_array(),
    ]
    .into_iter()
    .flatten()
    .map(Vec::len)
    .sum::<usize>();
    if bytes.len() <= BYTES_LIMIT && rows <= ROWS_LIMIT {
        return Ok(bytes);
    }
    // Bounds never silently turn missing full evidence into a positive result.
    let phases: Vec<_> = evidence["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .collect();
    let differences: Vec<_> = evidence["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            !r["error"].is_null()
                || r["old_changed"].as_array().is_some_and(|v| !v.is_empty())
                || r["staged_changed"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
        })
        .take(4)
        .collect();
    let summary = serde_json::json!({
        "experiment":"separate_staged_persist", "policy":row["policy"],"status":"staged_inconclusive",
        "overflow":{"byte_limit":BYTES_LIMIT,"row_limit":ROWS_LIMIT,"bytes":bytes.len(),"rows":rows},
        "producer_ok":row["producer_ok"],
        "physical_r_drop":evidence["r_drop_performed"],"r_held":evidence["r_held"],
        "observer_summary":{"entries":row["observation"]["entries"].as_u64(),
            "event_count":row["observation"]["events"].as_array().map_or(0,Vec::len),
            "first_error":row["observation"]["first_error"].as_i64(),
            "released":row["observation"]["released"].as_bool(),"success":row["observation"]["success"].as_bool(),
            "entry":row["observation"]["entry"]},
        "stages":phases,"first_differences":differences,"full_inventories_or_checks_omitted":true,
    });
    let bytes = serde_json::to_vec(&summary).map_err(io::Error::other)?;
    if bytes.len() <= BYTES_LIMIT {
        return Ok(bytes);
    }
    serde_json::to_vec(&serde_json::json!({"experiment":"separate_staged_persist",
        "status":"staged_inconclusive","overflow":"summary_bound",
        "physical_r_drop":evidence["r_drop_performed"].as_bool(),
        "observer_entries":row["observation"]["entries"].as_u64(),
        "observer_event_count":row["observation"]["events"].as_array().map_or(0,Vec::len)}))
    .map_err(io::Error::other)
}
fn complete_report(row: &serde_json::Value) -> bool {
    bounded_record(row)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|encoded| encoded.get("overflow").is_none())
}
fn emit(output: &mut impl Write, row: &serde_json::Value) -> io::Result<()> {
    let bytes = bounded_record(row)?;
    output.write_all(b"G1 staged-persist diagnostic: ")?;
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.flush()
}
fn assert_after_report(
    output: &mut impl Write,
    row: &serde_json::Value,
    oracle: impl FnOnce(),
) -> io::Result<()> {
    let written = emit(output, row);
    oracle();
    written
}
fn staged(policy: Policy) {
    let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(prepare));
    let fixture = match prepared {
        Ok(Ok(fixture)) => fixture,
        outcome => {
            let error = match outcome {
                Ok(Err(error)) => Some(error),
                _ => None,
            };
            let row = serde_json::json!({"experiment":"separate_staged_persist","policy":format!("{policy:?}"),
                "status":"staged_inconclusive","stage":"setup","setup_error":error,
                "observation":{"entries":0,"events":[]},"physical_r_drop":false});
            let _ = assert_after_report(&mut io::stdout().lock(), &row, || {
                panic!("staged fixture was not established")
            });
            return;
        }
    };
    let entry_probe = fixture.probe.clone();
    let entry: EntryCallback = Box::new(move |until| entry_probe.borrow_mut().enter(until));
    // The ONLY policy-dependent construction is whether the release callback
    // exists. No publisher reopen and no extra R alias in matching NoDrop.
    let release = if policy == Policy::Fresh {
        let probe = fixture.probe.clone();
        Some(Box::new(move |until| probe.borrow_mut().release(until))
            as Box<dyn FnMut(Instant) -> io::Result<()>>)
    } else {
        None
    };
    let record = fixture.directory.path().join("run.json");
    let scope =
        Scope::install_staged(record.clone(), CONTENT.as_bytes().to_vec(), release, entry).unwrap();
    // Same actual ordinary producer; it alone creates/writes/syncs the source.
    let produced = storage::atomic(&record, CONTENT);
    let observation = scope.report();
    let observed = serde_json::to_value(&observation).unwrap();
    let final_check = fixture.probe.borrow_mut().finish();
    let evidence: Evidence = fixture.probe.borrow().evidence();
    let witness = evidence.positive(observation.positive(), produced.is_ok());
    let physical = evidence.physically_dropped();
    let reference = reference_failure(
        policy,
        produced.is_ok(),
        observed["first_error"].as_i64(),
        observed["entries"].as_u64(),
        evidence.entry_complete(),
        physical,
    );
    let status = classification(policy, witness, produced.is_ok());
    let row = serde_json::json!({"experiment":"separate_staged_persist","policy":format!("{policy:?}"),
        "status":status,"ordinary_writer_qualification":false,"producer_ok":produced.is_ok(),
        "reference_failure_observed_only":reference,"observation":observed,"staged":evidence,
        "immutable_fact_evidence":{"byte_count":fixture.bytes,
            "digest":fixture.digest.iter().map(|b|format!("{b:02x}")).collect::<String>()},
        "post_producer_check_ok":final_check.is_ok()});
    let full_report = complete_report(&row);
    let written = assert_after_report(&mut io::stdout().lock(), &row, || {
        // Diagnostic acceptance is scoped; none of these assertions replaces
        // any old failed pre-creation/ordinary-writer control.
        if policy == Policy::Fresh {
            assert!(
                witness && full_report,
                "no complete staged-persist cohort witness/report"
            );
        } else {
            assert!(!physical, "NoDrop must retain its original R");
            assert!(
                reference && full_report,
                "no complete NoDrop failure reference/report"
            );
        }
        // The existing source snapshots already verify bytes/cursor through
        // the original file. Do not reopen the destination after R release.
    });
    written.unwrap();
}
#[test]
fn staged_persist_fresh_original_source_and_complete_r_cohort() {
    staged(Policy::Fresh);
}
#[test]
fn staged_persist_matching_fresh_no_drop() {
    staged(Policy::NoDrop);
}

// Pure/report controls: no filesystem, handles, producer or Windows calls.
#[cfg(test)]
mod controls {
    use super::*;
    #[derive(Default)]
    struct Output {
        bytes: Vec<u8>,
        flushes: usize,
    }
    impl Write for Output {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    fn row() -> serde_json::Value {
        serde_json::json!({"policy":"Fresh","status":"staged_inconclusive",
  "observation":{"entries":0,"events":[]},"staged":{"r_drop_performed":false,"pre_creation":[],
  "post_staging":[],"checks":[],"stages":[]}})
    }
    #[test]
    fn zero_observer_and_failures_are_flushed_before_the_assertion() {
        let mut output = Output::default();
        let value = row();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = assert_after_report(&mut output, &value, || panic!("private guard fixture"));
        }))
        .is_err());
        assert_eq!(output.flushes, 1);
        assert!(String::from_utf8(output.bytes)
            .unwrap()
            .contains("\"entries\":0"));
    }
    #[test]
    fn failure_after_drop_is_never_reported_as_no_release() {
        let mut value = row();
        value["staged"]["r_drop_performed"] = true.into();
        value["staged"]["failure"] = "guard_fixture".into();
        let mut output = Output::default();
        emit(&mut output, &value).unwrap();
        assert!(String::from_utf8(output.bytes)
            .unwrap()
            .contains("\"r_drop_performed\":true"));
    }
    #[test]
    fn overflow_is_explicit_inconclusive_and_retains_drop_truth() {
        for bytes in [false, true] {
            let mut value = row();
            value["status"] = "staged_persist_cohort_witness".into();
            value["staged"]["r_drop_performed"] = true.into();
            if bytes {
                value["padding"] = "x".repeat(BYTES_LIMIT * 2).into();
            } else {
                value["staged"]["checks"] =
                    serde_json::json!(vec![serde_json::json!({}); ROWS_LIMIT + 1]);
            }
            let encoded = bounded_record(&value).unwrap();
            assert!(encoded.len() <= BYTES_LIMIT);
            let actual: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(actual["status"], "staged_inconclusive");
            assert_eq!(actual["physical_r_drop"], true);
        }
    }
    #[test]
    fn missing_bounded_report_cannot_admit_a_complete_witness() {
        let mut value = row();
        assert!(complete_report(&value));
        value["padding"] = "x".repeat(BYTES_LIMIT * 2).into();
        assert!(!complete_report(&value));
    }
    #[test]
    fn fresh_needs_both_a_complete_witness_and_successful_original_producer() {
        assert_eq!(
            classification(Policy::Fresh, true, true),
            "staged_persist_cohort_witness"
        );
        for (witness, producer) in [(false, true), (true, false), (false, false)] {
            assert_eq!(
                classification(Policy::Fresh, witness, producer),
                "staged_inconclusive"
            );
        }
    }
    #[test]
    fn no_drop_success_cannot_be_a_reference_or_a_positive_witness() {
        assert_ne!(
            classification(Policy::NoDrop, true, true),
            "staged_persist_cohort_witness"
        );
        assert!(!reference_failure(
            Policy::NoDrop,
            true,
            Some(32),
            Some(1),
            true,
            false
        ));
        assert!(reference_failure(
            Policy::NoDrop,
            false,
            Some(32),
            Some(1),
            true,
            false
        ));
        assert!(!reference_failure(
            Policy::NoDrop,
            false,
            None,
            Some(1),
            true,
            false
        ));
        assert!(!reference_failure(
            Policy::NoDrop,
            false,
            Some(32),
            Some(1),
            true,
            true
        ));
    }
    #[test]
    fn failed_output_does_not_bypass_the_original_guard_assertion() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("output fixture"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush fixture"))
            }
        }
        let failure = std::panic::catch_unwind(|| {
            let _ = assert_after_report(&mut Broken, &row(), || panic!("original staged guard"));
        })
        .unwrap_err();
        let message = failure
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| failure.downcast_ref::<String>().map(String::as_str))
            .unwrap();
        assert!(message.contains("original staged guard"));
    }
}
