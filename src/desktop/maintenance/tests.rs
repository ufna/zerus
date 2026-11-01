use super::*;
use crate::desktop::store::Location;
use rusqlite::params;
use std::sync::{atomic::AtomicUsize, Mutex};
use zerus_client_core::updates::Feed;
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
struct Fixture {
    _temp: tempfile::TempDir,
    location: Location,
    store: Store,
    runtime: Runtime,
}
impl Fixture {
    fn new(runner: Runner) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let location = Location {
            root: temp.path().canonicalize().unwrap().join("state/desktop"),
            configuration: "a".repeat(64),
        };
        location.prepare().unwrap();
        let epoch = id();
        let store = Store::open(&location, &epoch).unwrap();
        Self {
            _temp: temp,
            location,
            store,
            runtime: Runtime::with_runner(&epoch, runner).unwrap(),
        }
    }
    fn configure(&mut self, update: Value, recovery: Value) -> Value {
        self.runtime.request(&mut self.store,&json!({"op":"maintenance_configure","request_id":id(),"update":update,"recovery":recovery}))
    }
    fn settle(&mut self, predicate: impl Fn(&Runtime) -> bool) {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            self.runtime.poll(&mut self.store);
            if predicate(&self.runtime) {
                return;
            }
            assert!(
                Instant::now() < until,
                "did not settle: {}",
                self.runtime.status()
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn native_source(&mut self) {
        self.store.db.execute("INSERT OR REPLACE INTO desktop_catalog_sources(computer,epoch,incarnation,generation,stream,seq,hello,meta,complete,active) VALUES('',?1,1,1,'',0,'{}',?2,1,1)",params![self.runtime.epoch,json!({"peers":[],"local_sessions":true}).to_string()]).unwrap();
        // Same private observation path; no caller-supplied FleetEvent constructor.
        let event = feed::Packet::LegacyComplete {
            computer: String::new(),
            generation: 1,
        };
        self.runtime.observe_fixture(&self.store, &event);
    }
}
fn no_io() -> Runner {
    Arc::new(|_, _, _| panic!("unexpected I/O"))
}
fn update(automatic: bool) -> Value {
    json!({"gui":{"bundle_id":"com.hgdev.hgs-tray","executable":std::env::temp_dir().join("fixture/hgs-tray"),"app_version":"1.0.0","dev_number":2,"source_commit":"a".repeat(40)},"channel":"stable","automatic":automatic})
}
fn projected() -> UpdateProjection {
    UpdateProjection {
        ready: true,
        feed: Feed {
            valid: true,
            version: "2.0.0".into(),
            ..Feed::default()
        },
        available: true,
        ..UpdateProjection::default()
    }
}

#[test]
fn schema_twelve_is_singleton_and_migrates_an_existing_eleven() {
    let f = Fixture::new(no_io());
    assert_eq!(
        f.store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        12
    );
    f.store
        .db
        .execute_batch("DROP TABLE desktop_maintenance; PRAGMA user_version=11")
        .unwrap();
    let location = f.location;
    drop(f.store);
    let store = Store::open(&location, &id()).unwrap();
    assert!(state::load(&store.db).unwrap().configuration.is_none());
    assert!(store
        .db
        .execute("INSERT INTO desktop_maintenance VALUES(2,'{}')", [])
        .is_err());
}
#[test]
fn no_io_without_durable_config_or_when_automatic_is_disabled() {
    let mut f = Fixture::new(no_io());
    f.runtime.poll(&mut f.store);
    assert!(f.runtime.jobs.is_empty());
    assert_eq!(f.configure(update(false), Value::Null)["accepted"], true);
    f.runtime.poll(&mut f.store);
    assert!(f.runtime.jobs.is_empty());
}
#[test]
fn configuration_is_durable_idempotent_closed_and_bounded() {
    let mut f = Fixture::new(no_io());
    let request = json!({"op":"maintenance_configure","request_id":id(),"update":update(false),"recovery":null});
    let first = f.runtime.request(&mut f.store, &request);
    assert_eq!(first["durable"], true);
    assert_eq!(f.runtime.request(&mut f.store, &request), first);
    let mut changed = request.clone();
    changed["update"]["automatic"] = true.into();
    assert_eq!(f.runtime.request(&mut f.store, &changed)["accepted"], false);
    for mut invalid in [request.clone(), request.clone(), request.clone()] {
        invalid["request_id"] = id().into();
        invalid["extra"] = true.into();
        assert_eq!(f.runtime.request(&mut f.store, &invalid)["accepted"], false);
    }
    let oversized = json!({"op":"maintenance_configure","request_id":id(),"update":null,"recovery":{"policy":null,"base":{"x":"x".repeat(dto::CONFIG_LIMIT)}}});
    assert_eq!(
        f.runtime.request(&mut f.store, &oversized)["accepted"],
        false
    );
    assert_eq!(state::load(&f.store.db).unwrap().revision, 1);
    let mut reopened = Runtime::with_runner(&id(), no_io()).unwrap();
    reopened.poll(&mut f.store);
    assert_eq!(
        reopened.status()["configuration"]["update"]["automatic"],
        false
    );
    assert_eq!(reopened.status()["updates"]["current_origin"], false);
    let queried=reopened.request(&mut f.store,&json!({"op":"maintenance_result","request_id":id(),"original_request_id":request["request_id"]}));
    assert_eq!(queried["revision"], first["revision"]);
}
#[test]
fn failed_commit_preserves_old_configuration_and_starts_nothing() {
    let mut f = Fixture::new(no_io());
    assert_eq!(f.configure(update(false), Value::Null)["accepted"], true);
    f.store.db.execute_batch("CREATE TRIGGER refuse_maintenance BEFORE UPDATE ON desktop_maintenance BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
    assert_eq!(f.configure(update(true), Value::Null)["accepted"], false);
    assert!(
        !f.runtime
            .stored
            .as_ref()
            .unwrap()
            .configuration
            .as_ref()
            .unwrap()
            .update
            .as_ref()
            .unwrap()
            .automatic
    );
    assert!(f.runtime.jobs.is_empty());
}
#[test]
fn automatic_result_is_actor_committed_and_attempt_is_spent_before_io() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let mut f = Fixture::new(Arc::new(move |work, _, _| {
        assert!(matches!(work, Work::Update(_)));
        calls.fetch_add(1, Ordering::SeqCst);
        Answer::Update(Box::new(projected()))
    }));
    assert_eq!(f.configure(update(true), Value::Null)["accepted"], true);
    f.runtime.poll(&mut f.store);
    assert!(state::load(&f.store.db).unwrap().updates["stable"].last_attempt > 0);
    f.settle(|r| r.status()["updates"]["available"] == true);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(state::load(&f.store.db).unwrap().updates["stable"].available);
    // A restart keeps the cache and throttle, never cached currentness.
    f.runtime = Runtime::with_runner(&id(), no_io()).unwrap();
    f.runtime.poll(&mut f.store);
    assert_eq!(f.runtime.status()["updates"]["cached"], true);
    assert_eq!(f.runtime.status()["updates"]["available"], false);
}
#[test]
fn stale_completion_cannot_replace_changed_preferences() {
    let released = Arc::new(AtomicBool::new(false));
    let release = released.clone();
    let mut f = Fixture::new(Arc::new(move |_, _, _| {
        while !release.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }
        Answer::Update(Box::new(projected()))
    }));
    f.configure(update(true), Value::Null);
    f.runtime.poll(&mut f.store);
    f.configure(update(false), Value::Null);
    released.store(true, Ordering::Release);
    f.settle(|r| r.jobs.is_empty());
    assert_eq!(f.runtime.status()["updates"]["available"], false);
    assert_eq!(f.runtime.status()["updates"]["automatic"], false);
}
#[test]
fn stop_waits_for_original_worker_and_resource_custody() {
    let released = Arc::new(AtomicBool::new(false));
    let release = released.clone();
    let mut f = Fixture::new(Arc::new(move |_, _, _| {
        while !release.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }
        Answer::Update(Box::new(projected()))
    }));
    f.configure(update(true), Value::Null);
    f.runtime.poll(&mut f.store);
    f.runtime.request_stop();
    assert!(!f.runtime.drained());
    released.store(true, Ordering::Release);
    f.settle(Runtime::drained);
    assert_eq!(f.runtime.status()["updates"]["available"], false);
}
#[test]
fn retained_pipe_lease_refuses_quiet_and_future_admission() {
    let retained = Arc::new(Mutex::new(None));
    let output = retained.clone();
    let mut f = Fixture::new(Arc::new(move |_, _, lease| {
        *output.lock().unwrap() = Some(lease);
        Answer::Update(Box::new(projected()))
    }));
    f.configure(update(true), Value::Null);
    f.runtime.poll(&mut f.store);
    f.settle(|r| r.error == Some("maintenance_io_unknown"));
    assert!(!f.runtime.drained());
    assert_eq!(f.configure(update(false), Value::Null)["accepted"], false);
    retained.lock().unwrap().take();
    // Unknown stays pinned, even after an unrelated later counter change.
    f.runtime.poll(&mut f.store);
    assert!(!f.runtime.drained());
}
#[test]
fn recovery_waits_for_native_local_machine_card_and_reuses_existing_sync() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut f = Fixture::new(Arc::new(move |work, _, _| {
        let Work::Recovery(call) = work else {
            panic!("not recovery")
        };
        assert_eq!(call.op, recovery::Op::Get);
        count.fetch_add(1, Ordering::SeqCst);
        Answer::Recovery(Ok(
            json!({"order":[1,"fixture"],"policy":{"enabled":false,"delays":[30,-1]}})
                .as_object()
                .unwrap()
                .clone(),
        ))
    }));
    f.configure(Value::Null, json!({"policy":null,"base":{}}));
    f.runtime.poll(&mut f.store);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    f.native_source();
    f.settle(|r| !r.sync.envelope().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(state::load(&f.store.db)
        .unwrap()
        .policy
        .contains_key("policy"));
    f.store
        .db
        .execute("UPDATE desktop_catalog_sources SET active=0", [])
        .unwrap();
    f.runtime.poll(&mut f.store);
    assert!(!f.runtime.sync.known());
}
#[test]
fn recovery_edit_is_reserved_before_dispatch_and_never_replayed_on_restart() {
    let mut f = Fixture::new(Arc::new(|work, _, _| {
        let Work::Recovery(call) = work else { panic!() };
        assert_eq!(call.op, recovery::Op::Get);
        Answer::Recovery(Ok(
            json!({"order":[1,"fixture"],"policy":{"enabled":false,"delays":[30,-1]}})
                .as_object()
                .unwrap()
                .clone(),
        ))
    }));
    f.configure(Value::Null, json!({"policy":null,"base":{}}));
    f.native_source();
    f.settle(|r| !r.sync.busy() && !r.sync.envelope().is_empty());
    let base = f.runtime.sync.envelope().clone();
    let form = recovery::form(&base).unwrap();
    let request = json!({"op":"maintenance_configure","request_id":id(),"update":null,"recovery":{"policy":form,"base":base}});
    let accepted = f.runtime.request(&mut f.store, &request);
    assert_eq!(accepted["accepted"], true);
    assert_eq!(accepted["recovery_save"], "unknown");
    assert!(state::load(&f.store.db)
        .unwrap()
        .configuration
        .unwrap()
        .recovery
        .unwrap()
        .policy
        .is_none());
    assert_eq!(f.runtime.request(&mut f.store, &request), accepted);
    f.runtime = Runtime::with_runner(&id(), no_io()).unwrap();
    f.runtime.poll(&mut f.store);
    assert!(f.runtime.pending.is_empty());
    assert!(f.runtime.jobs.is_empty());
}
#[test]
fn full_request_history_refuses_instead_of_forgetting_replay_identities() {
    let mut f = Fixture::new(no_io());
    f.runtime.load(&f.store).unwrap();
    let stored = f.runtime.stored.as_mut().unwrap();
    for _ in 0..state::REQUESTS {
        stored.receipts.insert(
            id(),
            state::Receipt {
                fingerprint: "a".repeat(64),
                revision: 0,
                recovery_save: "not_requested".into(),
            },
        );
    }
    state::save(&mut f.store.db, stored).unwrap();
    assert_eq!(f.configure(Value::Null, Value::Null)["accepted"], false);
    assert_eq!(state::load(&f.store.db).unwrap().revision, 0);
}
impl Runtime {
    fn observe_fixture(&mut self, store: &Store, packet: &feed::Packet) {
        // Exercises the same parser after an owned application; production has
        // only observe(FleetEvent), whose permit cannot be forged from JSON.
        self.observe_packet(store, packet).unwrap();
    }
}

#[test]
fn generic_desktop_request_cannot_configure_maintenance() {
    let mut f = Fixture::new(no_io());
    let request = json!({"op":"maintenance_configure","request_id":id(),"update":update(true),"recovery":null});
    assert_eq!(
        crate::desktop::request(&f.location, &mut f.store, Some(&request))["accepted"],
        false
    );
    assert!(state::load(&f.store.db).unwrap().configuration.is_none());
}
#[test]
fn failed_check_keeps_valid_cached_catalog_without_claiming_current_availability() {
    let mut f = Fixture::new(Arc::new(|_, _, _| {
        Answer::Update(Box::new(UpdateProjection {
            error: Some("unavailable".into()),
            ..UpdateProjection::default()
        }))
    }));
    f.configure(update(true), Value::Null);
    let stored = f.runtime.stored.as_mut().unwrap();
    stored.updates.insert("stable".into(), projected());
    state::save(&mut f.store.db, stored).unwrap();
    f.runtime.poll(&mut f.store);
    f.settle(|r| r.jobs.is_empty());
    let view = f.runtime.status()["updates"].clone();
    assert_eq!(view["feed"]["valid"], true);
    assert_eq!(view["cached"], true);
    assert_eq!(view["available"], false);
    assert_eq!(view["current_origin"], false);
}

#[test]
fn manual_policy_save_commits_result_on_actor_without_recovery_executor_operations() {
    let sets = Arc::new(AtomicUsize::new(0));
    let count = sets.clone();
    let mut f = Fixture::new(Arc::new(move |work, _, _| {
        let Work::Recovery(call) = work else { panic!() };
        let envelope = match call.op {
            recovery::Op::Get => {
                json!({"order":[1,"fixture"],"policy":{"enabled":false,"delays":[30,-1]}})
            }
            recovery::Op::Set => {
                count.fetch_add(1, Ordering::SeqCst);
                json!({"order":[2,"fixture"],"policy":call.data})
            }
            recovery::Op::Sync => panic!("one machine needs no replication"),
        };
        Answer::Recovery(Ok(envelope.as_object().unwrap().clone()))
    }));
    f.configure(Value::Null, json!({"policy":null,"base":{}}));
    f.native_source();
    f.settle(|r| !r.sync.busy() && !r.sync.envelope().is_empty());
    let base = f.runtime.sync.envelope().clone();
    let form = recovery::form(&base).unwrap();
    let request = json!({"op":"maintenance_configure","request_id":id(),"update":null,"recovery":{"policy":form,"base":base}});
    assert_eq!(
        f.runtime.request(&mut f.store, &request)["recovery_save"],
        "unknown"
    );
    let original = request["request_id"].as_str().unwrap().to_owned();
    f.settle(|r| r.stored.as_ref().unwrap().receipts[&original].recovery_save == "saved");
    assert_eq!(sets.load(Ordering::SeqCst), 1);
    assert_eq!(
        state::load(&f.store.db).unwrap().receipts[&original].recovery_save,
        "saved"
    );
    assert_eq!(
        f.runtime.request(&mut f.store, &request)["recovery_save"],
        "saved"
    );
    assert_eq!(sets.load(Ordering::SeqCst), 1);
}
