//! One real ordinary producer, not a separately created publication primitive.
//! These test parts are inert after decomposition and never become Captured
//! authority. Native diagnostic success is not writer compatibility acceptance.
#![cfg(windows)]
use super::*;
use crate::state::repositories::directory::capture_diagnostic::legacy_reference::{
    self, AuditFailure, DiagnosticPurpose, LegacyAudit, LegacyAuditPoint, LegacyCase,
    LegacyOutcomeInput, OriginalCapturedBinding, OriginalCapturedData,
};
use crate::{
    platform::persist_diagnostic::{self, handle_info, Scope},
    state::repositories::directory::capture_diagnostic::{Acquisition, Cohort, Held, QueryError},
};
use serde::Serialize;
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

// BEGIN PORTABLE LEGACY OUTPUT — the real opt-in encoder, not a model copy.
mod legacy_output {
    use serde::Serialize;
    use std::io::{self, Write};
    pub(super) const BYTES: usize = 64 * 1024;
    pub(super) const ROWS: usize = 128;
    struct Capped {
        bytes: Vec<u8>,
    }
    impl Capped {
        fn new() -> Self {
            Self {
                bytes: Vec::with_capacity(BYTES),
            }
        }
    }
    impl Write for Capped {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > BYTES.saturating_sub(self.bytes.len()) {
                return Err(io::Error::other("legacy JSON byte cap"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[derive(Clone, Copy, Serialize)]
    pub(super) struct OldSummary {
        pub case: &'static str,
        pub status: &'static str,
        pub producer_ok: bool,
        pub post_ok: bool,
    }
    #[derive(Serialize)]
    struct Incomplete {
        old: OldSummary,
        purpose: &'static str,
        incomplete: bool,
        old_full_raw_omitted: bool,
        reason: &'static str,
        logical_rows: usize,
        row_limit: usize,
        byte_limit: usize,
    }
    pub(super) struct Emission {
        pub result: io::Result<()>,
        pub bytes: usize,
    }
    pub(super) fn emit(
        output: &mut impl Write,
        evidence: &impl Serialize,
        summary: OldSummary,
        rows: usize,
    ) -> Emission {
        let mut buffer = Capped::new();
        let complete = rows <= ROWS && serde_json::to_writer(&mut buffer, evidence).is_ok();
        if !complete {
            // The partial full buffer is never published. Encode one bounded
            // explicit NotQualified summary; no retry/full Value allocation.
            buffer = Capped::new();
            let fallback = Incomplete {
                old: summary,
                purpose: "NotQualified",
                incomplete: true,
                old_full_raw_omitted: true,
                reason: if rows > ROWS {
                    "logical_row_bound"
                } else {
                    "byte_bound_or_serialization_failure"
                },
                logical_rows: rows,
                row_limit: ROWS,
                byte_limit: BYTES,
            };
            if serde_json::to_writer(&mut buffer, &fallback).is_err() {
                return Emission {
                    result: Err(io::Error::other("bounded legacy summary failed")),
                    bytes: 0,
                };
            }
        }
        let bytes = buffer.bytes.len();
        let result = (|| {
            output.write_all(b"G1 ordinary-atomic diagnostic: ")?;
            output.write_all(&buffer.bytes)?;
            output.write_all(b"\n")?;
            output.flush()
        })();
        Emission {
            result: if complete {
                result
            } else {
                result.and(Err(io::Error::other("legacy evidence incomplete")))
            },
            bytes,
        }
    }
    #[cfg(test)]
    mod controls {
        use super::*;
        #[derive(Default)]
        struct Output {
            bytes: Vec<u8>,
            flushes: usize,
            fail_write: bool,
            fail_flush: bool,
            calls: usize,
            fail_at: Option<usize>,
        }
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.calls += 1;
                if self.fail_write || self.fail_at == Some(self.calls) {
                    return Err(io::Error::other("model write refusal"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                if self.fail_flush {
                    Err(io::Error::other("model flush refusal"))
                } else {
                    Ok(())
                }
            }
        }
        fn summary() -> OldSummary {
            OldSummary {
                case: "model",
                status: "inconclusive",
                producer_ok: false,
                post_ok: false,
            }
        }
        #[test]
        fn complete_typed_payload_is_written_and_flushed_before_outcome_assertion() {
            let mut output = Output::default();
            let e = emit(&mut output, &summary(), summary(), 1);
            assert!(e.result.is_ok());
            assert!(e.bytes <= BYTES);
            assert_eq!(output.flushes, 1);
            let json = output
                .bytes
                .strip_prefix(b"G1 ordinary-atomic diagnostic: ")
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(json).unwrap();
            assert_eq!(v["post_ok"], false);
        }
        #[test]
        fn row_overflow_is_explicit_not_qualified_and_never_complete() {
            let mut output = Output::default();
            let e = emit(&mut output, &summary(), summary(), ROWS + 1);
            assert!(e.result.is_err());
            assert!(e.bytes <= BYTES);
            assert_eq!(output.flushes, 1);
            let text = String::from_utf8(output.bytes).unwrap();
            assert!(text.contains("NotQualified"));
            assert!(text.contains("logical_row_bound"));
            assert!(text.contains("old_full_raw_omitted"));
        }
        #[test]
        fn byte_overflow_is_capped_before_publication_without_unbounded_full_encoding() {
            let mut output = Output::default();
            let huge = "x".repeat(BYTES + 1);
            let e = emit(&mut output, &huge, summary(), 1);
            assert!(e.result.is_err());
            assert!(e.bytes <= BYTES);
            assert!(output.bytes.len() < BYTES);
            let text = String::from_utf8(output.bytes).unwrap();
            assert!(text.contains("NotQualified"));
            assert!(!text.contains(&"x".repeat(256)));
        }
        #[test]
        fn write_and_flush_failures_cannot_become_complete_output() {
            for flush in [false, true] {
                let mut out = Output {
                    fail_write: !flush,
                    fail_flush: flush,
                    ..Output::default()
                };
                let e = emit(&mut out, &summary(), summary(), 1);
                assert!(e.result.is_err(), "output guard {flush}");
            }
            for at in 1..=3 {
                let mut out = Output {
                    fail_at: Some(at),
                    ..Output::default()
                };
                let e = emit(&mut out, &summary(), summary(), 1);
                assert!(e.result.is_err(), "individual write guard {at}");
            }
        }
    }
}
// END PORTABLE LEGACY OUTPUT

fn validate_original_snapshot(bytes: &[u8], expected: &Snapshot) -> io::Result<()> {
    let actual: Snapshot = serde_json::from_slice(bytes)
        .map_err(|_| io::Error::other("original snapshot framing changed"))?;
    actual
        .validate(&expected.executor)
        .map_err(|_| io::Error::other("original snapshot validation changed"))?;
    if actual != *expected || actual.held_unknown() {
        return Err(io::Error::other(
            "original complete snapshot/selectors changed",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod legacy_snapshot_controls {
    use super::*;
    fn original() -> Snapshot {
        Snapshot {
            schema: 1,
            executor: executor(),
            facts: vec![fact(2)],
            overflow: None,
        }
    }
    #[test]
    fn original_framing_validation_full_equality_and_each_selector_are_required() {
        let expected = original();
        assert!(
            validate_original_snapshot(&serde_json::to_vec(&expected).unwrap(), &expected).is_ok()
        );
        assert!(validate_original_snapshot(b"{bad", &expected).is_err());
        let mut invalid = original();
        invalid.schema = 0;
        assert!(
            validate_original_snapshot(&serde_json::to_vec(&invalid).unwrap(), &invalid).is_err()
        );
        let mutations: &[fn(&mut Snapshot)] = &[
            |s| s.facts[0].id = id(3),
            |s| s.facts[0].turn = Some("other-turn".into()),
            |s| s.executor.computer = id(5),
            |s| s.executor.session_id = id(6),
            |s| s.executor.run = "other-run".into(),
            |s| s.executor.birth = "other-birth".into(),
            |s| s.executor.conversation = Some("other-conversation".into()),
            |s| s.executor.child = Some("other-child".into()),
        ];
        for (index, change) in mutations.iter().enumerate() {
            let mut actual = original();
            change(&mut actual);
            assert!(
                validate_original_snapshot(&serde_json::to_vec(&actual).unwrap(), &expected)
                    .is_err(),
                "snapshot selector {index}"
            );
        }
    }
    #[test]
    fn valid_overflow_never_becomes_complete_original_data() {
        let overflow = fact(66);
        let expected = Snapshot {
            schema: 1,
            executor: executor(),
            facts: (2..=65).map(fact).collect(),
            overflow: Some(Overflow {
                id: overflow.id.clone(),
                original: overflow,
            }),
        };
        expected.validate(&executor()).unwrap();
        assert!(
            validate_original_snapshot(&serde_json::to_vec(&expected).unwrap(), &expected).is_err()
        );
    }
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Fresh => "Fresh",
            Self::Reopened => "Reopened",
            Self::ExtraAliases => "ExtraAliases",
            Self::NoDrop => "NoDrop",
        }
    }
    fn legacy(self) -> LegacyCase {
        match self {
            Self::Fresh => LegacyCase::Fresh,
            Self::Reopened => LegacyCase::Reopened,
            Self::ExtraAliases => LegacyCase::ExtraAliases,
            Self::NoDrop => LegacyCase::NoDrop,
        }
    }
}
#[derive(Clone, Copy, Serialize)]
enum ReleaseObservation {
    NotAttempted,
    Refused,
    Released,
}
#[derive(Serialize)]
struct OldPostCheck {
    ok: bool,
    error: Option<QueryError>,
}
#[derive(Serialize)]
struct LegacyEvidence<'a> {
    case: &'static str,
    status: &'static str,
    producer_ok: bool,
    observation: &'a persist_diagnostic::Report,
    inventory: &'a [crate::state::repositories::directory::capture_diagnostic::Inventory],
    verification: &'a crate::state::repositories::directory::capture_diagnostic::VerificationLog,
    post_check: OldPostCheck,
    // New rows refer only to the original roster slots. OLD raw numeric
    // labels above retain their unchanged, narrowly admitted representation.
    legacy_reference: LegacySupplement<'a>,
}
#[derive(Serialize)]
struct LegacySupplement<'a> {
    original_data: legacy_reference::OriginalDataSummary,
    first32: &'a std::result::Result<LegacyAudit, AuditFailure>,
    teardown: &'a std::result::Result<LegacyAudit, AuditFailure>,
    trace: Option<&'a persist_diagnostic::LegacyTrace>,
    old_release: ReleaseObservation,
    purpose_before_output: &'a legacy_reference::DiagnosticOutcome,
    logical_rows: usize,
    final_purpose_requires_successful_output_and_libtest: bool,
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
    let expected_snapshot = Rc::new(captured.snapshot.clone());
    // Original DATA is captured from the SAME verified objects, before the
    // original destructive decomposition. It is never current-name authority.
    let binding = Rc::new(
        OriginalCapturedBinding::capture(
            case.legacy(),
            &captured.root,
            &captured.directory,
            (&captured.fact_file, &captured.writer_file),
            OriginalCapturedData::new(
                captured.identity,
                captured.writer,
                captured.bytes,
                captured.digest,
            ),
            |bytes| validate_original_snapshot(bytes, &expected_snapshot),
        )
        .unwrap(),
    );
    let root = captured.root.object().unwrap();
    let root = persist_diagnostic::Object {
        volume: root.0,
        file: root.1,
    };
    // EXACT legacy construction/decomposition remains inert; no selected
    // factory, reconstruction or actor import is used by these four cases.
    let Captured {
        root: root_dir,
        directory,
        name: _,
        snapshot: _,
        identity: _,
        writer: _,
        fact_file,
        writer_file,
        bytes: _,
        digest: _,
    } = captured;
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
    let old_release = Rc::new(RefCell::new(ReleaseObservation::NotAttempted));
    let release = if matches!(case, Case::NoDrop) {
        None
    } else {
        let cohort = cohort.clone();
        let observation = old_release.clone();
        Some(Box::new(move |until| {
            // The REAL old predicate is called once, with its original
            // complete comparison/log/refusal. The supplement never replaces it.
            let result = cohort.borrow_mut().release_all_r(until);
            *observation.borrow_mut() = if result.is_ok() {
                ReleaseObservation::Released
            } else {
                ReleaseObservation::Refused
            };
            result
        }) as Box<dyn FnMut(Instant) -> io::Result<()>>)
    };
    let first_slot: Rc<RefCell<Option<std::result::Result<LegacyAudit, AuditFailure>>>> =
        Rc::new(RefCell::new(None));
    let audit = {
        let cohort = cohort.clone();
        let binding = binding.clone();
        let expected = expected_snapshot.clone();
        let slot = first_slot.clone();
        Box::new(move |until| {
            if slot.borrow().is_some() {
                return Err(io::Error::other("first32 audit already attempted"));
            }
            *slot.borrow_mut() = Some(Err(AuditFailure::unavailable(
                LegacyAuditPoint::TimelyFirst32,
            )));
            let result = cohort.borrow().audit_legacy_failure(
                LegacyAuditPoint::TimelyFirst32,
                until,
                &binding,
                |bytes| validate_original_snapshot(bytes, &expected),
            );
            let complete = result.as_ref().map(|_| ()).map_err(AuditFailure::io);
            *slot.borrow_mut() = Some(result);
            complete
        }) as persist_diagnostic::LegacyAuditCallback
    };
    let scope = Scope::install_legacy_reference(
        record.clone(),
        CONTENT.as_bytes().to_vec(),
        release,
        audit,
    )
    .unwrap();
    // EXACT original producer/content/backend/backoff and original deadline.
    let result = storage::atomic(&record, CONTENT);
    let report = scope.report();
    let post_check = cohort.borrow().verify_inventory();
    let verification = cohort.borrow().verification_log();
    let trace = scope.legacy_trace();
    let first = first_slot
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err(AuditFailure::unavailable(LegacyAuditPoint::TimelyFirst32)));
    let teardown = match trace
        .as_ref()
        .and_then(persist_diagnostic::LegacyTrace::original_until)
    {
        Some(until) => cohort.borrow().audit_legacy_failure(
            LegacyAuditPoint::Teardown,
            until,
            &binding,
            |bytes| validate_original_snapshot(bytes, &expected_snapshot),
        ),
        None => Err(AuditFailure::unavailable(LegacyAuditPoint::Teardown)),
    };
    // The original destination read-back remains AFTER the actual invocation;
    // it is not another terminal SOURCE snapshot or a no-effect check on Err.
    let readback = result.is_ok().then(|| fs::read(&record));
    let readback_info = result
        .is_ok()
        .then(|| fs::File::open(&record).and_then(|file| handle_info(&file)));
    let checked_readback = match (&readback, &readback_info) {
        (Some(Ok(bytes)), Some(Ok(info))) if !info.directory => Some(bytes.as_slice()),
        _ => None,
    };
    let inventory = cohort.borrow().inventory().to_vec();
    let mut input = LegacyOutcomeInput {
        binding: &binding,
        first: &first,
        teardown: &teardown,
        trace: trace.as_ref(),
        report: &report,
        inventory: &inventory,
        verification: &verification,
        post: &post_check,
        producer: &result,
        readback: checked_readback,
        expected_content: CONTENT.as_bytes(),
        total_rows: 0,
    };
    // Count the separate real old-release observation conservatively too.
    let rows = legacy_reference::required_rows(&input).saturating_add(1);
    input.total_rows = rows;
    let purpose = legacy_reference::classify(input);
    let status = classification(
        report.positive(),
        post_check.is_ok(),
        verification.complete(),
    );
    let evidence = LegacyEvidence {
        case: case.name(),
        status,
        producer_ok: result.is_ok(),
        observation: &report,
        inventory: &inventory,
        verification: &verification,
        post_check: OldPostCheck {
            ok: post_check.is_ok(),
            error: post_check.as_ref().err().map(QueryError::of),
        },
        legacy_reference: LegacySupplement {
            original_data: binding.summary(),
            first32: &first,
            teardown: &teardown,
            trace: trace.as_ref(),
            old_release: *old_release.borrow(),
            purpose_before_output: &purpose,
            logical_rows: rows,
            final_purpose_requires_successful_output_and_libtest: true,
        },
    };
    let emitted = legacy_output::emit(
        &mut io::stdout().lock(),
        &evidence,
        legacy_output::OldSummary {
            case: case.name(),
            status,
            producer_ok: result.is_ok(),
            post_ok: post_check.is_ok(),
        },
        rows,
    );
    let final_purpose = purpose.after_output(&emitted.result, rows, emitted.bytes);
    if result.is_ok() {
        assert_eq!(
            readback.as_ref().unwrap().as_ref().unwrap(),
            CONTENT.as_bytes()
        );
        assert!(!readback_info.as_ref().unwrap().as_ref().unwrap().directory);
    }
    match final_purpose.purpose() {
        DiagnosticPurpose::FullLegacyPositive => (),
        DiagnosticPurpose::CheckedLegacyFailureReference => (),
        DiagnosticPurpose::NotQualified => {
            panic!("legacy diagnostic remains NotQualified; old measurement is unchanged")
        }
    }
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
