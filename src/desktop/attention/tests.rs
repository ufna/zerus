use super::projection::{self, Projection};
use super::*;
use crate::desktop::{
    notifications::{self, Endpoint},
    store::Location,
};
use serde_json::{json, Map, Value};
use zerus_client_core::{
    attention,
    desktop::notifications::{Action, NativeId, Outcome, Platform, Session},
};
use zerus_desktop_ledger as ledger;
use zerus_watch_protocol::{Body, CatalogItem, Delta, Frame, Hello, Snapshot};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn enabled() -> Policy {
    Policy {
        enabled: true,
        optional_questions: true,
    }
}
fn row() -> Value {
    json!({"name":"codex/example/review","cmd":"codex","agent":"codex",
        "state":"running","process_state":"running","activity":"idle","phase":"idle",
        "run_id":"run","conversation_id":"conversation","conversation_state":"active",
        "created":1,"reply_id":"reply-1","attention_signature":null,
        "read":{"conversation_id":"conversation","reply_id":null,"attention_signature":null},
        "process_work":{"count":0},"subagents_active":0,"optional_questions":{"count":0}})
}
fn project(raw: &Value) -> Projection {
    projection::project("", "fixture", raw, Map::new(), enabled(), false, false).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    outbox: notifications::Outbox,
    host: Endpoint,
    runtime: Runtime,
    source: catalog::Source,
    seq: u64,
    cursor: i64,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let location = Location {
            root: temp.path().join("state/desktop"),
            configuration: "a".repeat(64),
        };
        location.prepare().unwrap();
        let epoch = id();
        let writer = id();
        let mut store = Store::open(&location, &epoch).unwrap();
        ledger::authorize(&mut store.db, &writer, &epoch).unwrap();
        let (outbox, host) = notifications::Outbox::new(Session {
            configuration: location.configuration,
            service_epoch: epoch.clone(),
            writer_epoch: writer,
            adapter: id(),
            host_epoch: id(),
            lease: id(),
            platform: Platform::Macos,
        })
        .unwrap();
        let source = catalog::Source {
            computer: String::new(),
            service_epoch: epoch.clone(),
            incarnation: 1,
            generation: 1,
        };
        Self {
            _temp: temp,
            store,
            outbox,
            host,
            runtime: Runtime::new(&epoch),
            source,
            seq: 0,
            cursor: 0,
        }
    }
    fn save(&mut self, raw: &Value, baseline: bool) {
        let session = state::session("", raw).unwrap();
        let previous = state::previous(&self.store.db, &session).unwrap();
        let projection =
            projection::project("", "fixture", raw, previous, enabled(), false, baseline).unwrap();
        state::save(
            &mut self.store.db,
            "",
            "key",
            state::Position {
                scope: "scope",
                row_hash: "hash",
                policy_revision: 0,
            },
            projection,
            baseline,
        )
        .unwrap();
    }
    fn intents(&self) -> i64 {
        self.store
            .db
            .query_row(
                "SELECT count(*) FROM desktop_intents WHERE json_extract(identity,'$.kind')='post'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn apply(&mut self, body: Body, connect: bool) {
        self.seq += 1;
        let frame = Frame {
            v: 1,
            seq: self.seq,
            body,
        };
        if connect {
            catalog::connect(&mut self.store.db, &self.source, &frame).unwrap();
        } else {
            catalog::apply(&mut self.store.db, &self.source, &frame).unwrap();
        }
        self.runtime
            .observe_packet(
                &self.store,
                &feed::Packet::Watch {
                    computer: String::new(),
                    generation: 1,
                    frame: Box::new(frame),
                },
            )
            .unwrap();
    }
    fn snapshot(&mut self, rows: Vec<Value>) {
        let hello = Hello {
            machine_id: Some("fixture".into()),
            host: "fixture".into(),
            hgs_version: "1.56.0".into(),
            stream: "stream".into(),
            heartbeat_ms: 10_000,
            max_frame_bytes: 1 << 20,
            topics: vec!["catalog".into()],
            limits: Default::default(),
        };
        self.apply(Body::Hello(hello), true);
        self.apply(
            Body::Snapshot(Snapshot {
                topic: "catalog".into(),
                page: 0,
                key: None,
                value: None,
                last: true,
                cursor: 0,
                items: Some(
                    rows.into_iter()
                        .map(|row| {
                            serde_json::to_value(CatalogItem {
                                key: zerus_watch_protocol::catalog_key(&row).unwrap(),
                                row,
                            })
                            .unwrap()
                        })
                        .collect(),
                ),
            }),
            false,
        );
    }
    fn delta(&mut self, raw: Value) {
        self.cursor += 1;
        self.apply(
            Body::Delta(Delta {
                topic: "catalog".into(),
                cursor: self.cursor,
                upsert: vec![CatalogItem {
                    key: zerus_watch_protocol::catalog_key(&raw).unwrap(),
                    row: raw,
                }],
                remove: vec![],
            }),
            false,
        );
    }
    fn tick(&mut self) {
        self.runtime.poll(&mut self.store, Some(&mut self.outbox));
        assert!(self.runtime.error.is_none(), "{}", self.runtime.status());
    }
    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }
}
#[test]
fn unread_reply_waits_for_all_original_work_without_consuming_its_identity() {
    let raw = row();
    let ready = project(&raw);
    assert_eq!(ready.candidates.len(), 1);
    assert!(!ready.candidates[0].consumed);
    for (field, value) in [
        ("process_work", json!({"count":1})),
        ("subagent_active_count", json!(1)),
        ("subagent_active_count", json!(-1)),
        (
            "coordination_wait",
            json!({"schema":1,"status":"ok","run_id":"run","conversation_id":"conversation","count":1}),
        ),
    ] {
        let mut busy = raw.clone();
        busy[field] = value;
        assert!(project(&busy).candidates.is_empty(), "{field}");
    }
    let mut unknown = raw.clone();
    unknown["process_work"] = json!({"count":-1});
    assert!(
        !project(&unknown).candidates[0].consumed,
        "existing policy treats unknown job count as no known background work"
    );
}
#[test]
fn explicit_attention_precedes_reply_even_with_background_work() {
    let mut raw = row();
    raw["activity"] = json!("waiting");
    raw["phase"] = json!("approval");
    raw["attention_id"] = json!("permission-1");
    raw["attention_signature"] = json!("a".repeat(64));
    raw["process_work"] = json!({"count":1});
    let p = project(&raw);
    assert_eq!(p.candidates.len(), 1);
    assert!(!p.candidates[0].consumed);
}
#[test]
fn shared_mute_and_suppressed_reply_are_permanently_consumed() {
    let mut f = Fixture::new();
    let mut raw = row();
    raw["notification_preferences"] = json!({"conversation_id":"conversation","muted":true,"revision":1,"suppressed":{"reply_id":null,"attention_signature":null,"optional_question_ids":[],"main_attention":null,"child_attention":{}}});
    f.save(&raw, false);
    raw["notification_preferences"]["muted"] = json!(false);
    f.save(&raw, false);
    let blocked: bool = f
        .store
        .db
        .query_row("SELECT blocked FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    assert!(blocked);
    assert_eq!(f.intents(), 1);
    raw["reply_id"] = json!("reply-2");
    raw["notification_preferences"]["suppressed"]["reply_id"] = json!("reply-2");
    f.save(&raw, false);
    assert_eq!(
        f.store
            .db
            .query_row(
                "SELECT count(*) FROM desktop_intents WHERE blocked=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
}
#[test]
fn rename_resume_and_repeated_projection_reuse_original_payload_and_genuine_run() {
    let mut f = Fixture::new();
    let mut raw = row();
    f.save(&raw, false);
    let original: (String, String) = f
        .store
        .db
        .query_row("SELECT identity,payload FROM desktop_intents", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    raw["name"] = json!("codex/example/renamed");
    raw["run_id"] = json!("resumed-run");
    f.save(&raw, false);
    assert_eq!(f.intents(), 1);
    assert!(f
        .store
        .db
        .query_row("SELECT live FROM desktop_attention_edges", [], |r| r
            .get::<_, bool>(0))
        .unwrap());
    let after: (String, String) = f
        .store
        .db
        .query_row("SELECT identity,payload FROM desktop_intents", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(original, after);
    assert!(after.0.contains("\"run\":\"run\""));
    let saved: Map<String, Value> =
        state::previous(&f.store.db, &state::session("", &raw).unwrap()).unwrap();
    let token = saved
        .values()
        .find_map(|e| e.get("token").and_then(Value::as_str))
        .unwrap();
    assert_eq!(attention::target(token).unwrap()["run"], "run");
}
#[test]
fn optional_question_state_and_component_signatures_do_not_repost_main() {
    let mut f = Fixture::new();
    let mut raw = row();
    raw["reply_id"] = Value::Null;
    raw["phase"] = json!("approval");
    raw["attention_id"] = json!("permission");
    f.save(&raw, false);
    raw["optional_questions"] = json!({"count":1,"ids":["q-1"]});
    raw["attention_signature"] = json!("a".repeat(64));
    f.save(&raw, false);
    assert_eq!(
        f.intents(),
        2,
        "one main + one consumed optional, no repeated main banner"
    );
    assert_eq!(
        f.store
            .db
            .query_row(
                "SELECT count(*) FROM desktop_intents WHERE blocked=0",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
#[test]
fn optional_banner_survives_an_unrelated_update_and_is_withdrawn_when_answered() {
    let mut f = Fixture::new();
    let mut raw = row();
    raw["reply_id"] = Value::Null;
    raw["optional_questions"] = json!({"count":1,"ids":["q-1"]});
    f.save(&raw, false);
    f.save(&raw, false);
    assert_eq!(f.intents(), 1);
    assert!(!f
        .store
        .db
        .query_row("SELECT blocked FROM desktop_intents", [], |r| r
            .get::<_, bool>(0))
        .unwrap());
    raw["optional_questions"] = json!({"count":0,"ids":[]});
    f.save(&raw, false);
    assert!(!f
        .store
        .db
        .query_row("SELECT live FROM desktop_attention_edges", [], |r| r
            .get::<_, bool>(0))
        .unwrap());
}
#[test]
fn policy_off_focus_history_and_initial_busy_baseline_never_post_later() {
    for (policy, focused, baseline, field, value) in [
        (Policy::default(), false, false, "phase", json!("idle")),
        (enabled(), true, false, "phase", json!("idle")),
        (enabled(), false, true, "process_work", json!({"count":1})),
        (enabled(), false, false, "state", json!("archived")),
    ] {
        let mut f = Fixture::new();
        let mut raw = row();
        raw[field] = value;
        let p = projection::project("", "fixture", &raw, Map::new(), policy, focused, baseline)
            .unwrap();
        state::save(
            &mut f.store.db,
            "",
            "key",
            state::Position {
                scope: "scope",
                row_hash: "hash",
                policy_revision: 0,
            },
            p,
            baseline,
        )
        .unwrap();
        f.save(&row(), false);
        assert_eq!(f.intents(), 1);
        assert!(f
            .store
            .db
            .query_row("SELECT blocked FROM desktop_intents", [], |r| r
                .get::<_, bool>(0))
            .unwrap());
    }
}
#[test]
fn quota_and_mapping_failure_refuse_without_evicting_unknown_or_delivered_edges() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "UPDATE desktop_attention_totals SET edges=?1",
            [state::MAX_EDGES],
        )
        .unwrap();
    let p = project(&row());
    assert!(state::save(
        &mut f.store.db,
        "",
        "key",
        state::Position {
            scope: "scope",
            row_hash: "hash",
            policy_revision: 0
        },
        p,
        false
    )
    .is_err());
    assert_eq!(f.intents(), 0);
    f.store
        .db
        .execute("UPDATE desktop_attention_totals SET edges=0", [])
        .unwrap();
    f.store.db.execute_batch("CREATE TRIGGER fixture_refuse BEFORE INSERT ON desktop_intents BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let p = project(&row());
    assert!(state::save(
        &mut f.store.db,
        "",
        "key",
        state::Position {
            scope: "scope",
            row_hash: "hash",
            policy_revision: 0
        },
        p,
        false
    )
    .is_err());
    f.store
        .db
        .execute_batch("DROP TRIGGER fixture_refuse;")
        .unwrap();
    let mut resumed = row();
    resumed["run_id"] = json!("new-run");
    f.save(&resumed, false);
    assert_eq!(f.intents(), 1);
    let identity: String = f
        .store
        .db
        .query_row("SELECT identity FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    assert!(identity.contains("\"run\":\"run\""));
}
#[test]
fn oversized_inputs_do_not_create_partial_intents() {
    let mut raw = row();
    raw["optional_questions"] = json!({"count":65,"ids":vec!["q";65]});
    assert!(projection::project("", "fixture", &raw, Map::new(), enabled(), false, false).is_err());
    raw = row();
    raw["run_id"] = json!("x".repeat(1025));
    assert!(projection::project("", "fixture", &raw, Map::new(), enabled(), false, false).is_err());
}
#[test]
fn owned_snapshot_is_baseline_then_delta_posts_once_and_offline_stops_admission() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    f.snapshot(vec![row()]);
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    let mut next = row();
    next["reply_id"] = json!("reply-2");
    f.delta(next.clone());
    f.ticks(4);
    let dispatch = f.host.try_next().unwrap().begin().unwrap();
    let command = dispatch.command().clone();
    assert!(matches!(command.action, Action::Post { .. }));
    dispatch.finish(Outcome::Unknown {
        code: zerus_client_core::desktop::notifications::Uncertainty::HostLost,
    });
    f.outbox.poll(&mut f.store, Instant::now());
    f.delta(next.clone());
    f.ticks(4);
    assert!(f.host.try_next().is_err(), "unknown never replayed");
    f.runtime.reject("");
    next["reply_id"] = json!("reply-3");
    f.cursor += 1;
    f.seq += 1;
    catalog::apply(
        &mut f.store.db,
        &f.source,
        &Frame {
            v: 1,
            seq: f.seq,
            body: Body::Delta(Delta {
                topic: "catalog".into(),
                cursor: f.cursor,
                upsert: vec![CatalogItem {
                    key: zerus_watch_protocol::catalog_key(&next).unwrap(),
                    row: next,
                }],
                remove: vec![],
            }),
        },
    )
    .unwrap();
    f.ticks(4);
    assert!(f.host.try_next().is_err());
}
#[test]
fn terminal_receipt_is_retained_before_exact_native_withdrawal_and_callback_tombstone() {
    let mut f = Fixture::new();
    f.save(&row(), false);
    let intent: String = f
        .store
        .db
        .query_row("SELECT id FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    f.outbox
        .offer_intent(&mut f.store, &intent, 1, Instant::now())
        .unwrap();
    let dispatch = f.host.try_next().unwrap().begin().unwrap();
    let command = dispatch.command().clone();
    assert!(matches!(
        state::withdrawal(&mut f.store.db, &intent).unwrap(),
        state::Withdrawal::Waiting
    ));
    let native_id = NativeId::Macos {
        request_id: attention::request_id(command.action.token()),
    };
    dispatch.finish(Outcome::Delivered {
        native_id: native_id.clone(),
    });
    f.outbox.poll(&mut f.store, Instant::now());
    let state::Withdrawal::Ready(withdrawal) = state::withdrawal(&mut f.store.db, &intent).unwrap()
    else {
        panic!("missing native mapping")
    };
    f.outbox
        .offer_intent(&mut f.store, &withdrawal, 1, Instant::now())
        .unwrap();
    let native = f.host.try_next().unwrap().begin().unwrap();
    assert!(
        matches!(&native.command().action,Action::Withdraw{token,native_id:n} if token==command.action.token()&&*n==native_id)
    );
    assert!(!ledger::current(&f.store.db, &intent, 1).unwrap());
}
#[test]
fn actor_turn_and_resident_pending_rows_stay_bounded() {
    let mut f = Fixture::new();
    let mut rows = Vec::new();
    for i in 0..20 {
        let mut r = row();
        r["conversation_id"] = json!(format!("conversation-{i}"));
        r["run_id"] = json!(format!("run-{i}"));
        rows.push(r);
    }
    f.snapshot(rows);
    f.tick();
    assert!(f.intents() <= ROWS_PER_TURN as i64);
    assert!(f.runtime.pending.as_ref().unwrap().rows.len() <= catalog::PAGE_ROWS);
    f.runtime.request_stop();
    assert!(f.runtime.drained());
    f.ticks(1);
    assert!(f.host.try_next().is_err());
}

#[test]
fn no_host_still_consumes_policy_off_baseline_before_host_and_policy_appear() {
    let mut f = Fixture::new();
    f.snapshot(vec![row()]);
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    assert_eq!(f.intents(), 1);
    assert_eq!(
        f.store
            .db
            .query_row("SELECT count(*) FROM desktop_attempts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    f.runtime.set_policy(enabled());
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    let mut next = row();
    next["reply_id"] = json!("new-reply");
    f.delta(next);
    f.ticks(4);
    assert!(f.host.try_next().unwrap().begin().is_some());
}
#[test]
fn host_arrival_revalidates_the_live_row_and_cannot_post_a_superseded_reply() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    let mut initial = row();
    initial["reply_id"] = Value::Null;
    f.snapshot(vec![initial]);
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    let mut first = row();
    first["reply_id"] = json!("reply-first");
    f.delta(first);
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    let mut latest = row();
    latest["reply_id"] = json!("reply-latest");
    f.delta(latest);
    f.ticks(4);
    let dispatch = f.host.try_next().unwrap().begin().unwrap();
    let target = attention::target(dispatch.command().action.token()).unwrap();
    assert_eq!(target["reply"], "reply-latest");
    assert!(f.host.try_next().is_err());
}
#[test]
fn policy_change_invalidates_queued_admission_even_if_reenabled_before_poll() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    let mut initial = row();
    initial["reply_id"] = Value::Null;
    f.snapshot(vec![initial]);
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    f.delta(row());
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    f.runtime.set_policy(Policy::default());
    f.runtime.set_policy(enabled());
    f.ticks(8);
    assert!(f.host.try_next().is_err());
    let mut next = row();
    next["reply_id"] = json!("after-enable");
    f.delta(next);
    f.ticks(4);
    assert!(f.host.try_next().unwrap().begin().is_some());
}
#[test]
fn expired_original_observation_and_retention_gap_cannot_authorize_a_post() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    let mut initial = row();
    initial["reply_id"] = Value::Null;
    f.snapshot(vec![initial]);
    f.ticks(4);
    f.delta(row());
    f.runtime.sources.get_mut("").unwrap().at = Instant::now() - FRESH - Duration::from_secs(1);
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    // A fresh owned callback can restore source eligibility, but starts from a
    // consumed baseline, rather than promoting the missing interval.
    let packet = feed::Packet::Watch {
        computer: String::new(),
        generation: 1,
        frame: Box::new(Frame {
            v: 1,
            seq: f.seq,
            body: Body::Heartbeat(zerus_watch_protocol::Heartbeat { at: 0.0 }),
        }),
    };
    f.runtime.observe_packet(&f.store, &packet).unwrap();
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    let mut second = row();
    second["reply_id"] = json!("lost-reply");
    f.delta(second);
    f.store
        .db
        .execute("UPDATE desktop_catalog_change_views SET floor=head", [])
        .unwrap();
    f.ticks(8);
    assert!(f.host.try_next().is_err());
}
#[test]
fn retiring_an_unstarted_original_send_cancels_it_without_a_native_close() {
    let mut f = Fixture::new();
    f.save(&row(), false);
    let intent: String = f
        .store
        .db
        .query_row("SELECT id FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    f.outbox
        .offer_intent(&mut f.store, &intent, 1, Instant::now())
        .unwrap();
    f.outbox.retire_intent(&mut f.store, &intent).unwrap();
    assert!(f.host.try_next().unwrap().begin().is_none());
    f.outbox.poll(&mut f.store, Instant::now());
    assert_eq!(
        f.store
            .db
            .query_row("SELECT state FROM desktop_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "refused"
    );
    assert!(f.host.try_next().is_err());
}
#[test]
fn live_projection_restores_exact_callback_mapping_without_reposting_history() {
    let mut f = Fixture::new();
    f.save(&row(), false);
    let intent: String = f
        .store
        .db
        .query_row("SELECT id FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    f.outbox
        .offer_intent(&mut f.store, &intent, 1, Instant::now())
        .unwrap();
    let dispatch = f.host.try_next().unwrap().begin().unwrap();
    let token = dispatch.command().action.token().to_owned();
    let native_id = NativeId::Macos {
        request_id: attention::request_id(&token),
    };
    dispatch.finish(Outcome::Delivered {
        native_id: native_id.clone(),
    });
    f.outbox.poll(&mut f.store, Instant::now());
    let head = f.outbox.restore_live_head(&f.store).unwrap();
    let page = f.outbox.restore_live_page(&f.store, 0, head).unwrap();
    assert!(page.complete);
    assert_eq!(page.mappings.len(), 1);
    assert_eq!(page.mappings[0].token, token);
    assert_eq!(page.mappings[0].native_id, native_id);
    assert!(f.host.try_next().is_err());
    let mut read = row();
    read["read"]["reply_id"] = json!("reply-1");
    f.save(&read, false);
    let page = f.outbox.restore_live_page(&f.store, 0, head).unwrap();
    assert!(page.mappings.is_empty());
    assert!(!ledger::current(&f.store.db, &intent, 1).unwrap());
}

#[test]
fn host_arrival_waits_again_if_a_retained_reply_has_returned_to_background_work() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    let mut initial = row();
    initial["reply_id"] = Value::Null;
    f.snapshot(vec![initial]);
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    f.delta(row());
    for _ in 0..4 {
        f.runtime.poll(&mut f.store, None);
    }
    let mut busy = row();
    busy["process_work"] = json!({"count":1});
    f.delta(busy);
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    f.delta(row());
    f.ticks(4);
    assert!(f.host.try_next().unwrap().begin().is_some());
}
#[test]
fn unknown_reply_activity_retains_unread_identity_without_creating_current_work() {
    let mut unknown = row();
    unknown["activity"] = json!("unknown");
    assert!(project(&unknown).candidates.is_empty());
    let mut f = Fixture::new();
    f.save(&unknown, false);
    assert_eq!(f.intents(), 0);
    f.save(&row(), false);
    assert_eq!(f.intents(), 1);
    assert!(!f
        .store
        .db
        .query_row("SELECT blocked FROM desktop_intents", [], |r| r
            .get::<_, bool>(0))
        .unwrap());
}

#[test]
fn resume_removal_and_old_archived_rows_preserve_the_original_live_notice() {
    let mut f = Fixture::new();
    f.runtime.set_policy(enabled());
    let mut initial = row();
    initial["reply_id"] = Value::Null;
    f.snapshot(vec![initial]);
    f.ticks(4);
    f.delta(row());
    f.ticks(4);
    let dispatch = f.host.try_next().unwrap().begin().unwrap();
    let token = dispatch.command().action.token().to_owned();
    dispatch.finish(Outcome::Delivered {
        native_id: NativeId::Macos {
            request_id: attention::request_id(&token),
        },
    });
    f.outbox.poll(&mut f.store, Instant::now());
    let mut resumed = row();
    resumed["run_id"] = json!("resumed-run");
    resumed["name"] = json!("codex/example/renamed");
    f.cursor += 1;
    f.apply(
        Body::Delta(Delta {
            topic: "catalog".into(),
            cursor: f.cursor,
            upsert: vec![CatalogItem {
                key: zerus_watch_protocol::catalog_key(&resumed).unwrap(),
                row: resumed,
            }],
            remove: vec![zerus_watch_protocol::Removal {
                key: "run:run".into(),
                reason: None,
            }],
        }),
        false,
    );
    f.ticks(6);
    let mut historical = row();
    historical["state"] = json!("archived");
    historical["process_state"] = json!("exited");
    f.delta(historical);
    f.ticks(4);
    assert!(f.host.try_next().is_err());
    let head = f.outbox.restore_live_head(&f.store).unwrap();
    let mappings = f
        .outbox
        .restore_live_page(&f.store, 0, head)
        .unwrap()
        .mappings;
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].token, token);
    assert_eq!(attention::target(&mappings[0].token).unwrap()["run"], "run");
}
#[test]
fn schema_fourteen_migrates_an_existing_thirteen_without_changing_prior_effects() {
    let f = Fixture::new();
    let root = f.store.db.path().unwrap().to_owned();
    let count: i64 = f
        .store
        .db
        .query_row("SELECT count(*) FROM desktop_intents", [], |r| r.get(0))
        .unwrap();
    f.store.db.execute_batch("DROP INDEX desktop_attention_catalog_identity; DROP TABLE desktop_attention_edges; DROP TABLE desktop_attention_sessions; DROP TABLE desktop_attention_totals; PRAGMA user_version=13;").unwrap();
    let location = Location {
        root: std::path::Path::new(&root).parent().unwrap().to_owned(),
        configuration: "a".repeat(64),
    };
    drop(f.store);
    let store = Store::open(&location, &id()).unwrap();
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        14
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM desktop_intents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        count
    );
    assert_eq!(state::head(&store.db).unwrap(), 0);
}
