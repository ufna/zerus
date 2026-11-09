//! Separate candidate-only tests. Native positives require the actual NTFS/GUID
//! qualification and never replace an old Captured/product gate or oracle.
//! Privacy-result mapping controls are PURE; they do not mutate a Windows
//! owner/DACL. No new ACL-changing primitive, forced-ID-reuse primitive or
//! actual actor/store/resident grant is provided by this candidate. Those
//! native/integration qualifications remain open, not silently modeled green.
#![cfg(windows)]
use super::super::coexistence::{self, Seal, Stage};
use super::*;
use crate::state::repositories::directory::stop_capture;
use std::time::{Duration, Instant};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(20)
}
fn private_fixture() -> tempfile::TempDir {
    let base = platform::canonical(std::env::temp_dir()).unwrap();
    let root = tempfile::tempdir_in(base).unwrap();
    platform::private_dir(root.path()).unwrap();
    platform::restrict_dir(root.path()).unwrap();
    drop(writer(root.path()));
    root
}
fn seeded(root: &Path) {
    let guard = writer(root);
    let publisher = Publisher::open_enabled(root, &guard, &executor(), true).unwrap();
    publisher.publish(&fact(2)).unwrap();
}
fn candidate(root: &Path, fresh: bool, aliases: usize) -> Seal {
    let deadline = until();
    match coexistence::fixture_seal(root, executor(), deadline, fresh, aliases) {
        Ok(seal) => seal,
        Err(failed) => {
            let stage = failed.stage();
            // A failed qualification is NOT a skipped/passing positive.
            eprintln!(
                "candidate SETUPREFUSED at {stage:?}, {:?}; no compatibility evidence",
                failed.detail()
            );
            failed.retire();
            panic!("candidate positive not established");
        }
    }
}
fn ordinary_positive(fresh: bool, aliases: usize) {
    let dir = private_fixture();
    if !fresh {
        seeded(dir.path());
    }
    let seal = candidate(dir.path(), fresh, aliases);
    let expected = vec![fact(2)];
    {
        let lease = seal.lease(&executor()).unwrap();
        assert_eq!(lease.read(|facts| facts.to_vec()).unwrap(), expected);
        lease.final_check().unwrap();
    }
    // A real second owning native Writer, not a writer_released boolean.
    let next = crate::state::task_controls::writer::test_open(dir.path(), until()).unwrap();
    next.verify().unwrap();
    drop(next);
    let fact_path = path(dir.path(), &executor());
    let fact_bytes = fs::read(&fact_path).unwrap();
    let lock_bytes = fs::read(dir.path().join(".lock")).unwrap();
    let record = dir.path().join("run.json");
    // Exact ordinary source and backend, no substitute publisher or retry.
    storage::atomic(&record, r#"{"state":"live"}"#).unwrap();
    assert_eq!(fs::read_to_string(&record).unwrap(), r#"{"state":"live"}"#);
    storage::atomic(&record, r#"{"state":"active"}"#).unwrap();
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        r#"{"state":"active"}"#
    );
    let lease = seal.lease(&executor()).unwrap();
    assert_eq!(lease.read(|facts| facts.to_vec()).unwrap(), expected);
    assert_eq!(fs::read(&fact_path).unwrap(), fact_bytes);
    assert_eq!(fs::read(dir.path().join(".lock")).unwrap(), lock_bytes);
    lease.final_check().unwrap();
    drop(lease);
    seal.retire();
}
#[test]
fn candidate_fresh_seal_keeps_original_ordinary_new_and_replacement_writes() {
    ordinary_positive(true, 0);
}
#[test]
fn candidate_reopened_seal_keeps_original_ordinary_new_and_replacement_writes() {
    ordinary_positive(false, 0);
}
#[test]
fn candidate_all_explicit_owned_root_aliases_settle_before_ordinary_writes() {
    ordinary_positive(false, 2);
}

#[test]
fn retained_native_writer_is_not_accepted_as_a_ready_handoff() {
    let dir = private_fixture();
    seeded(dir.path());
    let deadline = until();
    coexistence::fault(Some(Stage::Retirement));
    let outcome = coexistence::fixture_seal(dir.path(), executor(), deadline, false, 0);
    coexistence::fault(None);
    let failed = match outcome {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("held native writer became ready")
        }
    };
    assert_eq!(failed.stage(), Stage::Retirement);
    assert_eq!(coexistence::retained(&failed), (true, true, true, true));
    assert!(crate::state::task_controls::writer::test_open(
        dir.path(),
        Instant::now() + Duration::from_millis(15)
    )
    .is_err());
    failed.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}
#[test]
fn omitted_original_root_alias_never_produces_a_ready_seal() {
    let dir = private_fixture();
    seeded(dir.path());
    let deadline = until();
    stop_capture::fault_omit_root(true);
    let result = coexistence::fixture_seal(dir.path(), executor(), deadline, false, 2);
    stop_capture::fault_omit_root(false);
    let failed = match result {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("incomplete root cohort was accepted")
        }
    };
    assert_eq!(failed.stage(), Stage::Conversion);
    assert_eq!(coexistence::retained(&failed), (false, true, true, true));
    failed.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}
#[test]
fn every_conversion_cut_keeps_data_custody_until_explicit_retirement() {
    for stage in [
        Stage::OriginalWriter,
        Stage::Publisher,
        Stage::Fact,
        Stage::Snapshot,
        Stage::WriterObject,
        Stage::Namespace,
        Stage::Keeper,
        Stage::Retirement,
        Stage::Conversion,
        Stage::Handoff,
    ] {
        let dir = private_fixture();
        seeded(dir.path());
        let deadline = until();
        let bytes = fs::read(path(dir.path(), &executor())).unwrap();
        coexistence::fault(Some(stage));
        let result = coexistence::fixture_seal(dir.path(), executor(), deadline, false, 0);
        coexistence::fault(None);
        let failed = match result {
            Err(f) => f,
            Ok(s) => {
                s.retire();
                panic!("conversion cut did not refuse")
            }
        };
        assert_eq!(
            failed.stage(),
            stage,
            "an earlier real failure is an unestablished cut"
        );
        let retained = coexistence::retained(&failed);
        if !matches!(stage, Stage::OriginalWriter | Stage::Publisher) {
            assert!(retained.1);
        }
        if matches!(
            stage,
            Stage::WriterObject
                | Stage::Namespace
                | Stage::Keeper
                | Stage::Retirement
                | Stage::Conversion
                | Stage::Handoff
        ) {
            assert!(retained.2);
        }
        assert_eq!(fs::read(path(dir.path(), &executor())).unwrap(), bytes);
        failed.retire();
        crate::state::task_controls::writer::test_open(dir.path(), until())
            .unwrap()
            .verify()
            .unwrap();
    }
}
#[test]
fn active_current_lease_retains_original_bounded_writer_refusal() {
    let dir = private_fixture();
    seeded(dir.path());
    let seal = candidate(dir.path(), false, 0);
    let lease = seal.lease(&executor()).unwrap();
    assert_eq!(lease.read(|facts| facts.to_vec()).unwrap(), [fact(2)]);
    assert!(storage::atomic(&dir.path().join("run.json"), r#"{"state":"live"}"#).is_err());
    lease.final_check().unwrap();
    drop(lease);
    storage::atomic(&dir.path().join("run.json"), r#"{"state":"live"}"#).unwrap();
    seal.lease(&executor()).unwrap().final_check().unwrap();
    seal.retire();
}
#[test]
fn same_phase_legacy_capture_conflict_remains_a_separate_negative() {
    let dir = private_fixture();
    let guard = writer(dir.path());
    let p = Publisher::open_enabled(dir.path(), &guard, &executor(), true).unwrap();
    p.publish(&fact(2)).unwrap();
    let captured = p.capture().unwrap();
    drop(guard);
    captured.verify().unwrap();
    assert!(storage::atomic(&dir.path().join("run.json"), r#"{"state":"live"}"#).is_err());
    captured.verify().unwrap();
}
#[test]
fn same_size_fact_change_refuses_without_repair_or_failure_time_rebase() {
    let dir = private_fixture();
    seeded(dir.path());
    let seal = candidate(dir.path(), false, 0);
    let file = path(dir.path(), &executor());
    let original = fs::read(&file).unwrap();
    let mut changed = original.clone();
    let at = changed.iter().position(|b| *b == b'2').unwrap();
    changed[at] = b'3';
    fs::write(&file, &changed).unwrap();
    assert!(seal.lease(&executor()).is_err());
    assert_eq!(fs::read(&file).unwrap(), changed);
    fs::write(&file, &original).unwrap();
    assert!(seal.lease(&executor()).is_err()); // poisoned, never rebase
    seal.retire();
}
#[test]
fn malformed_oversized_removed_and_multilink_fact_never_gain_current_authority() {
    for mode in ["malformed", "oversized", "removed", "multilink"] {
        let dir = private_fixture();
        seeded(dir.path());
        let seal = candidate(dir.path(), false, 0);
        let file = path(dir.path(), &executor());
        match mode {
            "malformed" => fs::write(&file, b"{broken").unwrap(),
            "oversized" => fs::write(&file, vec![b' '; BYTES + 1]).unwrap(),
            "removed" => {
                if fs::remove_file(&file).is_err() {
                    eprintln!("candidate SETUPREFUSED removed fact");
                    continue;
                }
            }
            _ => {
                if fs::hard_link(&file, file.with_extension("link")).is_err() {
                    eprintln!("candidate SETUPREFUSED multiply linked fact");
                    continue;
                }
            }
        }
        let before = fs::read(&file).ok();
        assert!(seal.lease(&executor()).is_err());
        assert_eq!(fs::read(&file).ok(), before);
        seal.retire();
    }
}
#[test]
fn equal_byte_replacement_and_changed_writer_refuse_if_the_fault_can_be_established() {
    for lock in [false, true] {
        let dir = private_fixture();
        seeded(dir.path());
        let seal = candidate(dir.path(), false, 0);
        let file = if lock {
            dir.path().join(".lock")
        } else {
            path(dir.path(), &executor())
        };
        let bytes = fs::read(&file).unwrap();
        if fs::rename(&file, file.with_extension("kept")).is_err() {
            eprintln!("candidate SETUPREFUSED equal-byte replacement; no exercised-negative claim");
            continue;
        }
        platform::private_open(OpenOptions::new().write(true).create_new(true))
            .open(&file)
            .unwrap();
        fs::write(&file, &bytes).unwrap();
        assert!(seal.lease(&executor()).is_err());
        assert_eq!(fs::read(&file).unwrap(), bytes);
        seal.retire();
    }
}
#[test]
fn facts_directory_rename_or_reparse_never_retargets_a_lease() {
    let dir = private_fixture();
    seeded(dir.path());
    let seal = candidate(dir.path(), false, 0);
    let facts = dir.path().join("task-stop-facts");
    let kept = dir.path().join("kept-facts");
    if fs::rename(&facts, &kept).is_err() {
        eprintln!("candidate SETUPREFUSED facts directory rename");
        return;
    }
    if std::os::windows::fs::symlink_dir(&kept, &facts).is_err() {
        eprintln!("candidate SETUPREFUSED directory reparse setup");
        fs::rename(&kept, &facts).unwrap();
        return;
    }
    assert!(seal.lease(&executor()).is_err());
    seal.retire();
    fs::remove_dir(&facts).unwrap();
    fs::rename(&kept, &facts).unwrap();
}
#[test]
fn an_exact_different_executor_never_uses_an_original_seal() {
    for field in 0..6 {
        let dir = private_fixture();
        seeded(dir.path());
        let seal = candidate(dir.path(), false, 0);
        let mut other = executor();
        match field {
            0 => other.run = "resumed-run".into(),
            1 => other.birth = "different-birth".into(),
            2 => other.session_id = id(9001),
            3 => other.computer = id(9002),
            4 => other.conversation = Some("different-conversation".into()),
            _ => other.child = Some("different-child".into()),
        }
        assert!(seal.lease(&other).is_err());
        assert!(seal.lease(&executor()).is_err());
        seal.retire();
    }
}
#[test]
fn a_failed_final_check_poisoned_lease_cannot_recover_or_escape_as_a_new_current_lease() {
    let dir = private_fixture();
    seeded(dir.path());
    let seal = candidate(dir.path(), false, 0);
    let lease = seal.lease(&executor()).unwrap();
    assert_eq!(lease.read(|facts| facts.to_vec()).unwrap(), [fact(2)]);
    coexistence::fault(Some(Stage::Final));
    assert!(lease.final_check().is_err());
    coexistence::fault(None);
    assert!(lease.read(|facts| facts.to_vec()).is_err());
    drop(lease);
    assert!(seal.lease(&executor()).is_err());
    seal.retire();
}
#[test]
fn an_expired_capture_retains_failed_owner_custody_and_never_renews_its_budget() {
    let dir = private_fixture();
    seeded(dir.path());
    let result = coexistence::fixture_seal(
        dir.path(),
        executor(),
        Instant::now() - Duration::from_secs(1),
        false,
        0,
    );
    let failed = match result {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("expired capture became ready")
        }
    };
    assert_eq!(failed.stage(), Stage::OriginalWriter);
    failed.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}

#[test]
fn dormant_default_entry_cannot_open_or_initialize_a_missing_root() {
    let dir = private_fixture();
    let missing = dir.path().join("missing");
    let mut invalid = executor();
    invalid.birth.clear();
    let result = coexistence::seal(&missing, invalid, until());
    let failed = match result {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("false-gate candidate became ready")
        }
    };
    assert_eq!(failed.stage(), Stage::OriginalWriter);
    assert_eq!(coexistence::retained(&failed), (false, false, false, false));
    assert!(!missing.exists());
    failed.retire();
}
#[test]
fn an_unbounded_alias_request_refuses_without_creating_an_authority_baseline() {
    let dir = private_fixture();
    seeded(dir.path());
    let result = coexistence::fixture_seal(dir.path(), executor(), until(), false, 3);
    let failed = match result {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("unbounded cohort accepted")
        }
    };
    assert_eq!(failed.stage(), Stage::Namespace);
    assert_eq!(coexistence::retained(&failed), (true, false, false, false));
    failed.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}
#[test]
fn renamed_root_is_never_authority_for_the_old_current_name() {
    let outer = private_fixture();
    let root = outer.path().join("native-root");
    fs::create_dir(&root).unwrap();
    drop(writer(&root));
    seeded(&root);
    let seal = candidate(&root, false, 0);
    let kept = outer.path().join("kept-native-root");
    if fs::rename(&root, &kept).is_err() {
        eprintln!("candidate SETUPREFUSED root rename; no exercised-negative claim");
        return;
    }
    fs::create_dir(&root).unwrap();
    assert!(seal.lease(&executor()).is_err());
    seal.retire();
    fs::remove_dir(&root).unwrap();
    fs::rename(&kept, &root).unwrap();
}
#[test]
fn a_redirected_ancestor_never_retargets_the_selected_root() {
    let outer = private_fixture();
    let ancestor = outer.path().join("parent");
    let root = ancestor.join("native-root");
    fs::create_dir_all(&root).unwrap();
    drop(writer(&root));
    seeded(&root);
    let seal = candidate(&root, false, 0);
    let kept = outer.path().join("kept-parent");
    if fs::rename(&ancestor, &kept).is_err() {
        eprintln!("candidate SETUPREFUSED ancestor rename; no exercised-negative claim");
        return;
    }
    if std::os::windows::fs::symlink_dir(&kept, &ancestor).is_err() {
        eprintln!("candidate SETUPREFUSED ancestor reparse setup");
        fs::rename(&kept, &ancestor).unwrap();
        return;
    }
    assert!(seal.lease(&executor()).is_err());
    seal.retire();
    fs::remove_dir(&ancestor).unwrap();
    fs::rename(&kept, &ancestor).unwrap();
}

#[test]
fn a_panicking_read_poisoned_the_seal_and_keeps_custody_until_retirement() {
    let dir = private_fixture();
    seeded(dir.path());
    let seal = candidate(dir.path(), false, 0);
    let lease = seal.lease(&executor()).unwrap();
    let result: std::result::Result<(), Stage> = lease.read(|_| panic!("private read fixture"));
    assert_eq!(result, Err(Stage::Final));
    assert!(lease.final_check().is_err());
    drop(lease);
    assert!(seal.lease(&executor()).is_err());
    seal.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}

#[test]
fn overflow_keeps_exact_original_data_and_refuses_a_ready_handoff() {
    let dir = private_fixture();
    {
        let guard = writer(dir.path());
        let p = Publisher::open_enabled(dir.path(), &guard, &executor(), true).unwrap();
        for n in 2..=66 {
            p.publish(&fact(n)).unwrap();
        }
    }
    let file = path(dir.path(), &executor());
    let bytes = fs::read(&file).unwrap();
    let result = coexistence::fixture_seal(dir.path(), executor(), until(), false, 0);
    let failed = match result {
        Err(f) => f,
        Ok(s) => {
            s.retire();
            panic!("overflow became a ready proof")
        }
    };
    assert_eq!(failed.stage(), Stage::Snapshot);
    assert_eq!(coexistence::retained(&failed), (true, true, false, false));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    failed.retire();
    crate::state::task_controls::writer::test_open(dir.path(), until())
        .unwrap()
        .verify()
        .unwrap();
}
