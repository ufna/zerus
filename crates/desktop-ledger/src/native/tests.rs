use super::*;
use crate::{authorize, begin_service, finish, put_baseline, put_intent, reserve, State, Target};
use serde_json::json;

fn id(n: u32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}
fn fixture(kind: Kind) -> (Connection, Reserve) {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    crate::migrate(&tx).unwrap();
    tx.commit().unwrap();
    begin_service(&mut db, &id(1)).unwrap();
    authorize(&mut db, &id(2), &id(1)).unwrap();
    let identity = Identity {
        kind,
        target: match kind {
            Kind::Post | Kind::Withdrawal => Target::Session {
                computer: "example-peer".into(),
                run: "original-run".into(),
                conversation: "original-conversation".into(),
            },
            _ => Target::Device {
                configuration: "a".repeat(64),
            },
        },
        edge: "original-edge".into(),
    };
    let intent_id = put_intent(
        &mut db,
        &identity,
        1,
        &json!({"title":"Example", "opaque":{"value":"данные"}}),
    )
    .unwrap();
    let req = Reserve {
        request_id: id(5),
        intent_id,
        revision: 1,
        epoch: id(2),
        service_epoch: id(1),
        adapter: id(3),
        lease: id(4),
        retry_of: None,
    };
    assert!(reserve(&mut db, &req).unwrap().fresh);
    (db, req)
}

#[test]
fn exact_snapshot_is_bounded_redacted_and_reads_without_a_schema_or_data_write() {
    for kind in [Kind::Post, Kind::Withdrawal] {
        let (mut db, req) = fixture(kind);
        let changes = db.total_changes();
        let schema: i64 = db
            .query_row("PRAGMA schema_version", [], |r| r.get(0))
            .unwrap();
        let payload: String = db
            .query_row(
                "SELECT payload FROM desktop_intents WHERE id=?1",
                [&req.intent_id],
                |r| r.get(0),
            )
            .unwrap();
        let observed = inspect(&mut db, &req).unwrap();
        assert_eq!(observed.seq(), 1);
        assert_eq!(observed.identity().kind, kind);
        assert_eq!(observed.payload_json(), payload);
        assert_eq!(observed.payload_sha256(), hash(payload.as_bytes()));
        assert_eq!(
            observed.reservation_fingerprint(),
            hash(json(&req, 4096).unwrap().as_bytes())
        );
        assert_eq!(
            json(observed.request(), 4096).unwrap(),
            json(&req, 4096).unwrap()
        );
        assert_eq!(format!("{observed:?}"), "NativeReservationSnapshot { .. }");
        assert_eq!(db.total_changes(), changes);
        assert_eq!(
            db.query_row("PRAGMA schema_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            schema
        );
        assert!(db.is_autocommit());
    }
}

#[test]
fn rereading_reserved_data_never_changes_freshness_or_renews_a_lease() {
    let (mut db, req) = fixture(Kind::Post);
    let first = inspect(&mut db, &req).unwrap();
    assert!(!reserve(&mut db, &req).unwrap().fresh);
    let duplicate = inspect(&mut db, &req).unwrap();
    assert_eq!(
        first.reservation_fingerprint(),
        duplicate.reservation_fingerprint()
    );
    assert_eq!(first.request().lease, duplicate.request().lease);
    assert_eq!(crate::status(&db).unwrap()["attempts"]["reserved"], 1);
    // This API has no fresh bit, authority, expiry, clock or execution method.
}

#[test]
fn every_original_request_binding_must_match_including_optional_retry() {
    let (mut db, req) = fixture(Kind::Post);
    for part in 0..9 {
        let mut changed = req.clone();
        match part {
            0 => changed.request_id = id(90),
            1 => changed.intent_id = "b".repeat(64),
            2 => changed.revision = 2,
            3 => changed.epoch = id(90),
            4 => changed.service_epoch = id(90),
            5 => changed.adapter = id(90),
            6 => changed.lease = id(90),
            7 => changed.retry_of = Some(id(90)),
            _ => changed.request_id = "malformed".into(),
        }
        assert!(inspect(&mut db, &changed).is_err(), "binding {part}");
    }
    assert!(inspect(&mut db, &req).is_ok());
}

#[test]
fn changed_stored_binding_cannot_hide_behind_an_unchanged_request_fingerprint() {
    for column in ["epoch", "service_epoch", "adapter", "lease", "retry_of"] {
        let (mut db, req) = fixture(Kind::Post);
        db.execute(
            &format!("UPDATE desktop_attempts SET {column}=?1 WHERE request_id=?2"),
            params![id(90), req.request_id],
        )
        .unwrap();
        assert!(inspect(&mut db, &req).is_err(), "stored binding {column}");
    }
}

#[test]
fn stale_writer_or_intent_and_import_baseline_refuse_without_changing_old_data() {
    for case in 0..5 {
        let (mut db, req) = fixture(Kind::Post);
        let old = inspect(&mut db, &req).unwrap();
        match case {
            0 => crate::revoke(&db, "manual_rollback").unwrap(),
            1 => begin_service(&mut db, &id(90)).unwrap(),
            2 => authorize(&mut db, &id(90), &id(1)).unwrap(),
            3 => {
                put_intent(&mut db, old.identity(), 2, &json!({"new":"revision"})).unwrap();
            }
            _ => {
                put_baseline(
                    &mut db,
                    old.identity(),
                    1,
                    &serde_json::from_str(old.payload_json()).unwrap(),
                )
                .unwrap();
            }
        }
        assert!(inspect(&mut db, &req).is_err(), "fence {case}");
        assert_eq!(old.request().revision, 1);
        assert!(!old.payload_json().contains("revision"));
    }
}

#[test]
fn withdrawal_refuses_a_new_snapshot_but_original_receipt_still_settles() {
    let (mut db, req) = fixture(Kind::Post);
    crate::withdraw(&mut db, &req.intent_id, 2).unwrap();
    assert!(inspect(&mut db, &req).is_err());
    crate::revoke(&db, "manual_rollback").unwrap();
    finish(
        &mut db,
        &req,
        State::Delivered,
        &json!({"code":"original_result"}),
    )
    .unwrap();
    assert_eq!(crate::status(&db).unwrap()["attempts"]["delivered"], 1);
}

#[test]
fn terminal_attempts_and_inconsistent_reserved_receipts_cannot_be_inspected() {
    for outcome in [State::Delivered, State::Refused, State::Unknown] {
        let (mut db, req) = fixture(Kind::Post);
        finish(&mut db, &req, outcome, &json!({})).unwrap();
        assert!(inspect(&mut db, &req).is_err());
    }
    let (mut db, req) = fixture(Kind::Post);
    db.execute(
        "UPDATE desktop_attempts SET receipt='{}' WHERE request_id=?1",
        [&req.request_id],
    )
    .unwrap();
    assert!(inspect(&mut db, &req).is_err());
}

#[test]
fn badges_presence_and_corrupt_saved_identity_refuse() {
    for kind in [Kind::Badge, Kind::Presence] {
        let (mut db, req) = fixture(kind);
        assert!(inspect(&mut db, &req).is_err());
    }
    for identity in [
        "null".to_string(),
        "{malformed".into(),
        " ".repeat(IDENTITY_BYTES + 1),
    ] {
        let (mut db, req) = fixture(Kind::Post);
        db.execute(
            "UPDATE desktop_intents SET identity=?1 WHERE id=?2",
            params![identity, req.intent_id],
        )
        .unwrap();
        assert!(inspect(&mut db, &req).is_err());
    }
    let (mut db, req) = fixture(Kind::Post);
    let old = inspect(&mut db, &req).unwrap();
    let mut changed = old.identity().clone();
    changed.edge = "different-edge".into();
    db.execute(
        "UPDATE desktop_intents SET identity=?1 WHERE id=?2",
        params![json(&changed, 4096).unwrap(), req.intent_id],
    )
    .unwrap();
    assert!(inspect(&mut db, &req).is_err());
}

#[test]
fn sql_caps_private_bytes_before_materialization_and_payload_is_validated() {
    let (mut db, req) = fixture(Kind::Post);
    let exact = format!("\"{}\"", "я".repeat(PAYLOAD_BYTES / 2 - 1));
    assert_eq!(exact.len(), PAYLOAD_BYTES);
    db.execute(
        "UPDATE desktop_intents SET payload=?1 WHERE id=?2",
        params![exact, req.intent_id],
    )
    .unwrap();
    assert_eq!(
        inspect(&mut db, &req).unwrap().payload_json().len(),
        PAYLOAD_BYTES
    );
    for bad in [
        " ".repeat(PAYLOAD_BYTES + 1),
        "{".into(),
        "{} {}".into(),
        "".into(),
    ] {
        db.execute(
            "UPDATE desktop_intents SET payload=?1 WHERE id=?2",
            params![bad, req.intent_id],
        )
        .unwrap();
        assert!(inspect(&mut db, &req).is_err());
        assert!(db.is_autocommit());
    }
}

#[test]
fn lookup_uses_unique_request_and_intent_indexes_instead_of_a_journal_scan() {
    let (db, req) = fixture(Kind::Post);
    let fingerprint = fingerprint(&req).unwrap();
    let mut query = db.prepare(&format!("EXPLAIN QUERY PLAN {LOOKUP}")).unwrap();
    let plan = query
        .query_map(
            params![
                req.request_id,
                fingerprint,
                req.intent_id,
                req.revision as i64,
                req.epoch,
                req.service_epoch,
                req.adapter,
                req.lease,
                req.retry_of,
                IDENTITY_BYTES as i64,
                PAYLOAD_BYTES as i64
            ],
            |r| r.get::<_, String>(3),
        )
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
    assert!(
        plan.contains("SEARCH a USING INDEX sqlite_autoindex_desktop_attempts_1 (request_id=?)"),
        "{plan}"
    );
    assert!(
        plan.contains("SEARCH i USING INDEX sqlite_autoindex_desktop_intents_1 (id=?)"),
        "{plan}"
    );
    assert!(!plan.contains("SCAN"), "{plan}");
}
