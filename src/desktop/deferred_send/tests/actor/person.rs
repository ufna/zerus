//! The person mark is written by the real frontend admission only, with the
//! exact identities of the admitted original, and the CLI reads it only from
//! its own configuration's store (zerus-46l1q, S1).
use super::*;
use crate::desktop::person_mark;

fn submitted_only() -> Fixture {
    Fixture::new(|state, plan, _, _, _, _| {
        submitted(state, plan);
        Ended::Exited {
            code: 0,
            stderr: String::new(),
        }
    })
}

#[test]
fn a_person_admission_writes_one_mark_with_the_originals_exact_identities() {
    let mut f = submitted_only();
    let original = f.original();
    let before = chrono::Utc::now().timestamp_millis();
    assert!(f.admit(&original).accepted);
    let after = chrono::Utc::now().timestamp_millis();
    let mark = person_mark::read_for_test(
        &f.store.db,
        &f.location.configuration,
        &original.plan.request_id,
    )
    .expect("the admission marks the person's message");
    assert_eq!(
        (
            mark.session.as_str(),
            mark.run_id.as_str(),
            mark.conversation_id.as_str(),
            mark.origin.as_str(),
            mark.input_sha256.as_str()
        ),
        (
            SESSION,
            RUN,
            CONVERSATION,
            person_mark::DESKTOP,
            original.binding.input_sha256.as_str()
        )
    );
    assert!((before..=after).contains(&mark.at));
    // The same original admitted again returns its receipt and adds nothing.
    f.until("settled", |f| f.row(&original).receipt.drained);
    assert!(f.admit(&original).accepted);
    assert_eq!(person_mark::count(&f.store.db), 1);
    // A mark is immutable.
    assert!(f
        .store
        .db
        .execute(
            "UPDATE desktop_person_marks SET session='claude/other' WHERE request_id=?1",
            [&original.plan.request_id],
        )
        .is_err());
    f.runtime.request_stop();
    f.until("stopped", |f| f.runtime.drained());
}

#[test]
fn a_refused_or_foreign_admission_writes_no_mark() {
    // Another connection's scope.
    let mut f = Fixture::new(|_, _, _, _, _, _| panic!("a refused original never runs"));
    let foreign = super::super::original(&"b".repeat(64), &f.epoch);
    assert!(!f.admit(&foreign).accepted);
    // A revoked writer.
    f.revoke();
    let original = f.original();
    assert!(!f.admit(&original).accepted);
    assert_eq!(person_mark::count(&f.store.db), 0);
    // The macOS fence refuses before any row.
    let mut f = Fixture::new(|_, _, _, _, _, _| panic!("a fenced original never runs"));
    f.runtime.fixture_mac_fence(true);
    let original = f.original();
    assert!(!f.admit(&original).accepted);
    assert_eq!(person_mark::count(&f.store.db), 0);
}

#[test]
fn a_first_message_without_a_conversation_is_sent_but_never_marked() {
    let mut f = submitted_only();
    let original = original_for(
        &f.location.configuration,
        &f.epoch,
        SESSION,
        RUN,
        "",
        "hello",
    );
    assert!(f.admit(&original).accepted);
    assert_eq!(person_mark::count(&f.store.db), 0);
    f.until("settled", |f| f.row(&original).receipt.drained);
    f.runtime.request_stop();
    f.until("stopped", |f| f.runtime.drained());
}

#[test]
fn the_cli_reads_a_mark_only_from_the_store_of_its_own_configuration() {
    let mut f = submitted_only();
    let original = f.original();
    assert!(f.admit(&original).accepted);
    f.until("settled", |f| f.row(&original).receipt.drained);
    let id = original.plan.request_id.as_str();
    let mark = person_mark::lookup(&f.config, id).expect("the CLI reads its own store");
    assert_eq!(mark.input_sha256, original.binding.input_sha256);
    assert!(person_mark::lookup(&f.config, &uuid::Uuid::new_v4().to_string()).is_none());
    assert!(person_mark::lookup(&f.config, "not-a-request").is_none());
    // A copy of the whole state folder elsewhere names the folders it was
    // made for: its marks are never this configuration's.
    f.runtime.request_stop();
    f.until("stopped", |f| f.runtime.drained());
    let copy = f.config.state.parent().unwrap().join("copy");
    let status = std::process::Command::new("cp")
        .arg("-a")
        .arg(&f.config.state)
        .arg(&copy)
        .status()
        .unwrap();
    assert!(status.success());
    let mut redirected = f.config.clone();
    redirected.state = copy.clone();
    assert!(person_mark::lookup(&redirected, id).is_none());
    // The same copy, read as its own configuration, still names the original
    // folders' configuration.
    let location = Location::new(&redirected).unwrap();
    assert_ne!(location.configuration, f.location.configuration);
    assert!(person_mark::lookup_in(&location, id).is_none());
    // A fresh folder has no store at all.
    let mut fresh = f.config.clone();
    fresh.state = f.config.state.parent().unwrap().join("fresh");
    std::fs::create_dir(&fresh.state).unwrap();
    mode(&fresh.state, 0o700);
    assert!(person_mark::lookup(&fresh, id).is_none());
}

#[test]
fn expired_marks_go_and_a_full_ledger_marks_nothing_but_still_sends() {
    let mut f = submitted_only();
    // A mark older than the turn bound plus its margin is removed by the
    // next admission, which marks its own message.
    let first = f.original();
    assert!(f.admit(&first).accepted);
    f.until("settled", |f| f.row(&first).receipt.drained);
    f.store
        .db
        .execute_batch(
            "DROP TRIGGER desktop_person_mark_original;
             UPDATE desktop_person_marks SET at=1;
             CREATE TRIGGER desktop_person_mark_original BEFORE UPDATE ON desktop_person_marks BEGIN
             SELECT RAISE(ABORT,'immutable person mark'); END;",
        )
        .unwrap();
    let second = f.original();
    assert!(f.admit(&second).accepted);
    f.until("settled", |f| f.row(&second).receipt.drained);
    assert_eq!(person_mark::count(&f.store.db), 1);
    assert!(person_mark::read_for_test(
        &f.store.db,
        &f.location.configuration,
        &second.plan.request_id
    )
    .is_some());
    // At the bound, an admission still sends and writes no mark.
    let now = chrono::Utc::now().timestamp_millis();
    let tx = f.store.db.transaction().unwrap();
    for _ in 1..4096 {
        tx.execute(
            "INSERT INTO desktop_person_marks VALUES(?1,'claude/x','r','c','desktop',?2,?3)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), "0".repeat(64), now],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let third = f.original();
    assert!(f.admit(&third).accepted);
    assert_eq!(person_mark::count(&f.store.db), 4096);
    assert!(person_mark::read_for_test(
        &f.store.db,
        &f.location.configuration,
        &third.plan.request_id
    )
    .is_none());
    f.until("settled", |f| f.row(&third).receipt.drained);
    f.runtime.request_stop();
    f.until("stopped", |f| f.runtime.drained());
}
