use super::*;
use std::{fs, path::Path, time::Instant};

struct Fixture(PathBuf);
impl Fixture {
    fn new(config: Value) -> Self {
        use std::os::unix::fs::DirBuilderExt;
        let root =
            std::env::temp_dir().join(format!("zerus-client-service-{}", uuid::Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        std::os::unix::fs::symlink(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/desktop_service/fake_hgs.py"),
            root.join("hgs"),
        )
        .unwrap();
        fs::write(root.join("fixture.json"), config.to_string()).unwrap();
        Self(root)
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("calls"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
    fn start(&self, allowed: bool) -> (Arc<Service>, crate::pump::Pump) {
        let pump = crate::pump::Pump::default();
        let (sink, rx) = std::sync::mpsc::sync_channel(256);
        let drain = pump.drain_service(rx, |_| {});
        let service = Service::start(
            self.0.join("hgs"),
            pump.clone(),
            sink,
            allowed,
            drain,
            None,
            vec![],
        );
        (service, pump)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
async fn eventually(label: &str, check: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(Instant::now() < until, "{label}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
fn watch_calls(calls: &[Value]) -> usize {
    calls
        .iter()
        .filter(|c| c["args"] == json!(["desktop", "watch", "--json"]))
        .count()
}

#[test]
fn replay_without_an_explicit_transport_never_launches_a_cli() {
    let _process = crate::PROCESS_TREE.read().unwrap();
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new(json!({}));
        let (service, _) = fixture.start(false);
        eventually("explicit unavailable", || {
            service.snapshot().code == "replay_transport_unavailable"
        })
        .await;
        service.join().await.unwrap();
        assert!(fixture.calls().is_empty());
    });
}
#[test]
fn incompatible_cli_stays_unavailable_until_one_explicit_retry_without_fallback() {
    let _process = crate::PROCESS_TREE.read().unwrap();
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new(json!({"supported":false}));
        let (service, _) = fixture.start(true);
        eventually("old CLI refused", || {
            service.snapshot().code == "incompatible_service_cli"
        })
        .await;
        assert_eq!(fixture.calls().len(), 1);
        fs::write(fixture.0.join("fixture.json"), json!({}).to_string()).unwrap();
        service.retry();
        eventually("new CLI connected", || service.snapshot().connected).await;
        eventually("one watch", || watch_calls(&fixture.calls()) == 1).await;
        assert!(!service.snapshot().background_ready());
        service.join().await.unwrap();
        assert!(fixture
            .calls()
            .iter()
            .all(|c| c["args"] == json!(["--help"]) || c["args"][0] == "desktop"));
    });
}
#[test]
fn dirty_walk_finishes_its_cursor_and_coalesces_one_refresh_without_deleting_rows() {
    let _process = crate::PROCESS_TREE.read().unwrap();
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new(json!({"dirty_walk":true}));
        let (service, pump) = fixture.start(true);
        pump.push(Change::Upsert {
            computer: ComputerId::local(),
            key: "unseen".into(),
            row: zerus_client_core::view::row(&json!({"name":"codex/example/unseen"})),
        });
        eventually("one follow-up refresh", || {
            fixture.0.join("refresh-started").exists()
        })
        .await;
        let pages: Vec<_> = fixture
            .calls()
            .into_iter()
            .filter(|c| c["value"]["op"] == "catalog_page")
            .collect();
        assert_eq!(pages.len(), 3);
        assert!(pages[0]["value"]["cursor"].is_null());
        assert!(pages[1]["value"]["cursor"].is_string());
        assert!(pages[2]["value"]["cursor"].is_null());
        assert!(
            pump.row("", "unseen").is_some(),
            "dirty walk cannot infer deletion"
        );
        fs::write(fixture.0.join("finish-refresh"), []).unwrap();
        eventually("clean refresh removes old membership", || {
            pump.row("", "unseen").is_none()
        })
        .await;
        assert!(pump.row("", "row").is_some());
        service.join().await.unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join("page-number")).unwrap(),
            "3"
        );
    });
}
#[test]
fn uncertain_read_keeps_its_uuid_and_only_durable_actor_commit_acknowledges() {
    let _process = crate::PROCESS_TREE.read().unwrap();
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new(json!({"ready":true}));
        let (service, _) = fixture.start(true);
        eventually("read component ready", || {
            service.snapshot().component_ready("reads")
        })
        .await;
        let mark = zerus_client_core::readmarks::ReadMark {
            computer: "".into(),
            name: "codex/example/row".into(),
            run_id: "run".into(),
            conversation_id: "conversation".into(),
            reply_id: Some("reply".into()),
            attention_signature: None,
        };
        assert!(service.read_marks(vec![mark.clone()]).await.is_err());
        assert_eq!(
            service.read_marks(vec![mark.clone()]).await.unwrap(),
            vec![mark]
        );
        let reads: Vec<_> = fixture
            .calls()
            .into_iter()
            .filter(|c| c["value"]["op"] == "read_intent")
            .collect();
        assert_eq!(reads.len(), 2);
        assert_eq!(
            reads[0]["value"]["request_id"],
            reads[1]["value"]["request_id"]
        );
        assert!(fixture
            .calls()
            .iter()
            .any(|c| c["value"]["op"] == "delivery_result"));
        assert!(request(
            &service.shared,
            json!({"op":"automatic_approvals"}),
            Arc::new(AtomicBool::new(false))
        )
        .await
        .is_err());
        service.join().await.unwrap();
        assert!(!fixture
            .calls()
            .iter()
            .any(|c| c["value"]["op"] == "automatic_approvals"));
    });
}
#[test]
fn queued_catalog_rows_are_discarded_after_source_replacement_and_removal() {
    let _process = crate::PROCESS_TREE.read().unwrap();
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new(json!({"supported":false}));
        let (service, pump) = fixture.start(true);
        eventually("old CLI refused", || {
            service.snapshot().code == "incompatible_service_cli"
        })
        .await;
        service.shared.publish(Snapshot {
            connected: true,
            epoch: Some("epoch".into()),
            ..Snapshot::default()
        });
        let old = SourceVersion {
            incarnation: 1,
            generation: 1,
        };
        service.shared.dirty("", old).unwrap();
        let delivery = |key: &str| CatalogDelivery {
            shared: Arc::downgrade(&service.shared),
            computer: "".into(),
            epoch: "epoch".into(),
            source: old,
            stopped: Arc::new(AtomicBool::new(false)),
            value: CatalogValue::Row(Change::Upsert {
                computer: ComputerId::local(),
                key: key.into(),
                row: zerus_client_core::view::row(&json!({"name":"codex/example/row"})),
            }),
        };
        let queued = delivery("stale");
        service
            .shared
            .dirty(
                "",
                SourceVersion {
                    incarnation: 1,
                    generation: 2,
                },
            )
            .unwrap();
        queued.apply(&pump, &|_| {});
        assert!(pump.row("", "stale").is_none());
        service.shared.dirty("", old).unwrap();
        let queued = delivery("removed");
        service.shared.invalidate();
        queued.apply(&pump, &|_| {});
        assert!(pump.row("", "removed").is_none());
        service.join().await.unwrap();
    });
}

#[test]
fn startup_switch_selects_only_the_service_source_and_closes_legacy_read_admission() {
    const CHILD: &str = "ZERUS_FIXTURE_DESKTOP_CHILD";
    if std::env::var_os(CHILD).is_some() {
        tauri::async_runtime::block_on(async {
            assert!(mode());
            let root = PathBuf::from(std::env::var_os(CHILD).unwrap());
            let actions = crate::actions::Actions::new(root.join("marks.json"));
            actions.disarm_read_delivery().unwrap();
            assert!(actions.queue_marks(vec![]).is_err());
            let source = crate::commands::AppState::default();
            assert_eq!(source.start_source().unwrap(), "service");
            assert_eq!(source.start_source().unwrap(), "service");
            let service = source.desktop_service().unwrap();
            eventually("service subscriber", || service.snapshot().connected).await;
            assert!(!service.snapshot().background_ready());
            eventually("one watch proxy", || {
                fs::read_to_string(root.join("calls"))
                    .unwrap_or_default()
                    .lines()
                    .any(|line| {
                        serde_json::from_str::<Value>(line)
                            .is_ok_and(|call| call["args"] == json!(["desktop", "watch", "--json"]))
                    })
            })
            .await;
            service.join().await.unwrap();
        });
        return;
    }
    let _process = crate::PROCESS_TREE.read().unwrap();
    let fixture = Fixture::new(json!({}));
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "desktop_service::tests::startup_switch_selects_only_the_service_source_and_closes_legacy_read_admission", "--nocapture"])
        .env(CHILD, &fixture.0).env(service::MODE_ENV, "1")
        .env("ZERUS_CLIENT_HGS", fixture.0.join("hgs")).env_remove("ZERUS_CLIENT_REPLAY")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let calls = fixture.calls();
    assert_eq!(watch_calls(&calls), 1);
    assert!(calls
        .iter()
        .all(|call| call["args"] == json!(["--help"]) || call["args"][0] == "desktop"));
    assert!(
        !fixture.0.join("marks.json").exists(),
        "legacy mark file was not written"
    );
}

#[test]
fn approval_seam_is_an_explicit_refusal_without_a_generic_cli_policy_call() {
    tauri::async_runtime::block_on(async {
        let id = uuid::Uuid::new_v4().to_string();
        let result = desktop_service_automatic_approvals(id.clone(), true)
            .await
            .unwrap();
        assert_eq!(result["request_id"], id);
        assert_eq!(result["accepted"], false);
        assert_eq!(result["durable"], false);
        assert_eq!(result["code"], "frontend_route_unavailable");
    });
}
