use super::*;
fn approval(enabled: bool) -> Request {
    Request::AutomaticApprovals {
        request_id: uuid::Uuid::new_v4().to_string(),
        enabled,
    }
}
#[test]
fn an_unknown_original_intent_only_allows_receipt_lookup_and_blocks_a_new_writer() {
    let state = Frontend::default();
    let original = approval(true);
    let Begin::Work(work) = state.begin(Operation::AutomaticApprovals, Some(original.clone()))
    else {
        panic!("reserved")
    };
    assert!(!work.lookup);
    let Begin::Immediate(busy) = state.begin(Operation::AutomaticApprovals, Some(original.clone()))
    else {
        panic!("busy")
    };
    assert_eq!(busy["accepted"], Value::Null);
    drop(work); // A stopped/panicked/cancelled worker remains uncertain.
    let Begin::Immediate(blocked) =
        state.begin(Operation::AutomaticApprovals, Some(approval(false)))
    else {
        panic!("blocked")
    };
    assert_eq!(blocked["accepted"], false);
    let Begin::Work(query) = state.begin(Operation::AutomaticApprovals, Some(original.clone()))
    else {
        panic!("lookup")
    };
    assert!(query.lookup);
    assert_eq!(query.request, original);
}
#[test]
fn a_durable_receipt_releases_the_operation_and_the_same_uuid_never_replays() {
    let state = Frontend::default();
    let request = approval(true);
    let Begin::Work(work) = state.begin(Operation::AutomaticApprovals, Some(request.clone()))
    else {
        panic!("reserved")
    };
    let receipt = json!({"request_id":request.id(),"op":"automatic_approvals","enabled":true,"accepted":true,"durable":true});
    let (receipt, complete) = checked(&request, receipt, false).unwrap();
    assert!(complete);
    work.finish(receipt.clone(), true);
    let Begin::Immediate(cached) = state.begin(Operation::AutomaticApprovals, Some(request)) else {
        panic!("cached")
    };
    assert_eq!(cached, receipt);
    assert!(matches!(
        state.begin(Operation::AutomaticApprovals, Some(approval(false))),
        Begin::Work(_)
    ));
}
#[test]
fn maintenance_config_commit_does_not_settle_an_unknown_recovery_save() {
    let request = Request::MaintenanceConfigure {
        request_id: uuid::Uuid::new_v4().to_string(),
        update: None,
        recovery: Some(Box::new(maintenance::RecoveryConfiguration {
            policy: Some(maintenance::RecoveryForm {
                enabled: false,
                rules: BTreeMap::new(),
                session_limit_mode: "reset".into(),
                delays: BTreeMap::new(),
            }),
            base: serde_json::Map::new(),
        })),
    };
    let result = json!({"request_id":request.id(),"original_request_id":request.id(),"accepted":true,"durable":true,
        "configured":true,"revision":1,"recovery_save":"unknown"});
    let (result, complete) = checked(&request, result, false).unwrap();
    assert!(!complete);
    assert_eq!(result["pending"], true);
}
#[test]
fn the_actual_version_suffix_alone_defines_the_gui_dev_number() {
    assert_eq!(version_dev_number("1.0.0"), Some(0));
    assert_eq!(version_dev_number("1.0.0-preview.local.abcdef0"), Some(0));
    assert_eq!(version_dev_number("1.0.0-nightly.42"), Some(0));
    assert_eq!(version_dev_number("1.0.0-dev.42"), Some(42));
    assert_eq!(
        version_dev_number("1.0.0-preview.local-dev.1a2b3c4"),
        Some(0)
    );
    assert_eq!(
        version_dev_number("1.0.0-preview.local-dev.1234567"),
        Some(0)
    );
    for version in [
        "1.0.0-dev.",
        "1.0.0-dev.0",
        "1.0.0-dev.042",
        "1.0.0-dev.+42",
    ] {
        assert_eq!(version_dev_number(version), None);
    }
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use crate::desktop_service::tests::Fixture;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        path::Path,
        thread,
    };
    use zerus_client_core::desktop::{Build, Responder};
    struct CacheServer {
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl CacheServer {
        fn start(root: &Path, responder: Arc<Responder>) -> Self {
            let listener = UnixListener::bind(root.join("native-cache.sock")).unwrap();
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let running = stop.clone();
            let thread = thread::spawn(move || {
                while !running.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(v) => v,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(e) => panic!("{e}"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    assert!(zerus_client_core::desktop::same_user(&stream));
                    let mut line = Vec::new();
                    let mut byte = [0];
                    while line.len() < 64 && stream.read_exact(&mut byte).is_ok() {
                        if byte[0] == b'\n' {
                            break;
                        }
                        line.push(byte[0]);
                    }
                    let reply = responder.reply(&line).unwrap_or_else(|| b"null\n".to_vec());
                    let _ = stream.write_all(&reply);
                }
            });
            Self {
                stop,
                thread: Some(thread),
            }
        }
    }
    impl Drop for CacheServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    async fn eventually(label: &str, check: impl Fn() -> bool) {
        let until = tokio::time::Instant::now() + Duration::from_secs(10);
        while !check() {
            assert!(tokio::time::Instant::now() < until, "{label}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    fn responder() -> Arc<Responder> {
        let build = crate::cli::build_info(&crate::cli::BUILD);
        let responder = Arc::new(Responder::new(
            "com.hgdev.hgs-tray".into(),
            Build {
                version: build["versionName"].as_str().unwrap().into(),
                source_commit: build["sourceCommit"].as_str().unwrap().into(),
                source_dirty: Some(false),
            },
            uuid::Uuid::new_v4().to_string(),
            true,
            false,
        ));
        responder.configure_once("a".repeat(64));
        responder
    }
    #[test]
    fn original_child_registration_receipts_and_unknown_delivery_use_only_the_dedicated_route() {
        const CHILD: &str = "ZERUS_FRONTEND_SENDER_FIXTURE";
        const TEST:&str="desktop_service::frontend::tests::native::original_child_registration_receipts_and_unknown_delivery_use_only_the_dedicated_route";
        if std::env::var_os(CHILD).is_none() {
            let _process = crate::PROCESS_TREE.read().unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, "1")
                .env_remove("HGS_SESSION")
                .env_remove("HGS_RUN_ID")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        tauri::async_runtime::block_on(async {
            for case in [
                "success",
                "unknown",
                "missing-abi",
                "maintenance",
                "maintenance-unknown",
                "retained-pipe",
            ] {
                let is_maintenance = case.starts_with("maintenance");
                let config = json!({"ready":true,"frontend_uncertain":matches!(case,"unknown"|"maintenance-unknown"),"frontend_abi":case!="missing-abi","frontend_retain":case=="retained-pipe"});
                let fixture = Fixture::new(config);
                let responder = responder();
                let _cache = CacheServer::start(&fixture.0, responder.clone());
                let (service, _) = fixture.start_with_responder(true, Some(responder.clone()));
                eventually("service lifecycle ready", || {
                    service.snapshot().component_ready("approvals")
                })
                .await;
                let id = uuid::Uuid::new_v4().to_string();
                let receipt = if is_maintenance {
                    let config = maintenance::Configuration {
                        update: Some(maintenance::UpdateConfiguration {
                            gui: maintenance::GuiIdentity {
                                bundle_id: "untrusted-js".into(),
                                executable: "untrusted-js".into(),
                                app_version: "wrong".into(),
                                dev_number: 999,
                                source_commit: String::new(),
                            },
                            channel: zerus_client_core::updates::Channel::Stable,
                            automatic: true,
                        }),
                        recovery: None,
                    };
                    service
                        .maintenance_configure(id.clone(), config, "com.hgdev.hgs-tray".into())
                        .await
                        .unwrap()
                } else {
                    service.automatic_approvals(id.clone(), true).await.unwrap()
                };
                if case == "missing-abi" {
                    assert_eq!(receipt["accepted"], false);
                    assert_eq!(receipt["code"], "incompatible_frontend_cli");
                    assert!(!fixture.calls().iter().any(
                        |call| call["args"] == json!(["desktop", "frontend-request", "--json"])
                    ));
                } else if matches!(case, "unknown" | "maintenance-unknown" | "retained-pipe") {
                    assert_eq!(receipt["request_id"], id);
                    assert_eq!(receipt["accepted"], Value::Null);
                    assert_eq!(receipt["pending"], true);
                    if case == "unknown" {
                        // A new service epoch cannot clear native uncertainty
                        // or turn the original logical UUID into a fresh send.
                        std::fs::write(
                            fixture.0.join("runtime-epoch"),
                            uuid::Uuid::new_v4().to_string(),
                        )
                        .unwrap();
                        service.retry();
                        eventually("new service epoch", || {
                            service.snapshot().component_ready("approvals")
                                && fixture
                                    .calls()
                                    .iter()
                                    .filter(|c| c["args"] == json!(["desktop", "watch", "--json"]))
                                    .count()
                                    == 2
                        })
                        .await;
                    } else if case == "retained-pipe" {
                        assert!(
                            service.shared.rpc.snapshot().unwrap().admitted > 0,
                            "held pipe retains original registration resources"
                        );
                        let call = fixture
                            .calls()
                            .into_iter()
                            .find(|c| c["args"] == json!(["desktop", "frontend-request", "--json"]))
                            .unwrap();
                        let mut query =
                            ui_requests::query(call["value"]["attempt"].as_str().unwrap()).unwrap();
                        query.pop();
                        assert!(
                            responder.reply(&query).is_some(),
                            "the original registration stays retained with the inherited pipe"
                        );
                    }
                    let blocked = if is_maintenance {
                        service
                            .maintenance_configure(
                                uuid::Uuid::new_v4().to_string(),
                                maintenance::Configuration {
                                    update: None,
                                    recovery: None,
                                },
                                "com.hgdev.hgs-tray".into(),
                            )
                            .await
                            .unwrap()
                    } else {
                        service
                            .automatic_approvals(uuid::Uuid::new_v4().to_string(), false)
                            .await
                            .unwrap()
                    };
                    assert_eq!(blocked["code"], "previous_frontend_request_unconfirmed");
                    // Same UUID becomes a read-only lookup, never another send.
                    let result = if is_maintenance {
                        service
                            .frontend_result(Operation::MaintenanceConfigure)
                            .await
                    } else {
                        service.automatic_approvals(id.clone(), true).await.unwrap()
                    };
                    assert_eq!(result["request_id"], id);
                    assert_eq!(result["durable"], true);
                    assert_eq!(result["pending"], false);
                    assert_eq!(
                        fixture
                            .calls()
                            .iter()
                            .filter(
                                |c| c["args"] == json!(["desktop", "frontend-request", "--json"])
                            )
                            .count(),
                        1
                    );
                    assert!(fixture.calls().iter().any(|c| c["value"]["op"]
                        == if is_maintenance {
                            "maintenance_result"
                        } else {
                            "delivery_result"
                        }
                        && c["value"]["original_request_id"] == id));
                    if case == "retained-pipe" {
                        std::fs::write(fixture.0.join("release-retainer"), []).unwrap();
                        eventually("original pipes actually drained", || {
                            service.shared.rpc.snapshot().unwrap().admitted == 0
                        })
                        .await;
                    }
                } else {
                    assert_eq!(receipt["request_id"], id);
                    assert_eq!(receipt["accepted"], true);
                    assert_eq!(receipt["durable"], true);
                    assert_eq!(receipt["pending"], false);
                    let call = fixture
                        .calls()
                        .into_iter()
                        .find(|c| c["args"] == json!(["desktop", "frontend-request", "--json"]))
                        .unwrap();
                    assert_eq!(call["value"].as_object().unwrap().len(), 3);
                    assert_eq!(call["value"]["abi"], ui_requests::ABI);
                    assert!(fixture.0.join("original-child-bound").exists());
                    if is_maintenance {
                        let expected = native_gui("com.hgdev.hgs-tray").unwrap();
                        assert_eq!(
                            call["value"]["request"]["update"]["gui"],
                            serde_json::to_value(expected).unwrap()
                        );
                        assert!(call["value"]["request"]
                            .as_object()
                            .unwrap()
                            .contains_key("recovery"));
                        assert!(call["value"]["request"]["recovery"].is_null());
                    }
                    let attempt = call["value"]["attempt"].as_str().unwrap();
                    let mut query = ui_requests::query(attempt).unwrap();
                    query.pop();
                    eventually(
                        "registration released only after owned exchange drains",
                        || responder.reply(&query).is_none(),
                    )
                    .await;
                }
                assert!(request(
                    &service.shared,
                    json!({"op":"automatic_approvals","request_id":id,"enabled":true}),
                    Arc::new(AtomicBool::new(false))
                )
                .await
                .is_err());
                assert!(!fixture
                    .calls()
                    .iter()
                    .any(|c| c["value"]["op"] == "automatic_approvals"));
                service.join().await.unwrap();
            }
        });
    }
}

#[test]
fn an_incomplete_or_mismatched_receipt_never_releases_the_original_request() {
    let request = approval(true);
    let receipt = json!({"request_id":request.id(),"op":"automatic_approvals","enabled":true,"accepted":true,"durable":true});
    for (field, wrong) in [
        ("request_id", json!(uuid::Uuid::new_v4().to_string())),
        ("op", json!("read_intent")),
        ("enabled", json!(false)),
        ("durable", json!(false)),
    ] {
        let mut value = receipt.clone();
        value[field] = wrong;
        assert!(checked(&request, value, false).is_none());
    }
    // A failed query is absence of evidence, never a fresh refusal/admission.
    assert!(checked(
        &request,
        json!({"request_id":request.id(),"accepted":false,"durable":false}),
        true
    )
    .is_none());
}

#[test]
fn non_durable_store_failures_and_unknown_maintenance_states_never_allow_replay() {
    let request = approval(true);
    for code in [
        "delivery_store_refused",
        "maintenance_refused",
        "unrecognized",
    ] {
        assert!(checked(
            &request,
            json!({"request_id":request.id(),"accepted":false,"durable":false,"code":code}),
            false
        )
        .is_none());
    }
    let request = Request::maintenance_configure(
        uuid::Uuid::new_v4().to_string(),
        maintenance::Configuration {
            update: None,
            recovery: None,
        },
    );
    assert!(checked(
        &request,
        json!({"request_id":request.id(),"original_request_id":request.id(),"configured":true,
        "revision":1,"accepted":true,"durable":true,"recovery_save":"new_unknown_state"}),
        false
    )
    .is_none());
}
