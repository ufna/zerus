use super::*;
use crate::desktop::store::Store;
use serde_json::json;
use std::{io::BufRead, path::PathBuf, sync::atomic::AtomicUsize};
use zerus_desktop_ledger as ledger;

const CHILD: &str = "desktop::notifications::host::tests::child_fixture";
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
struct FixtureNative {
    image: PathBuf,
    checks: Arc<AtomicUsize>,
}
impl Native for FixtureNative {
    fn executable(&self) -> &Path {
        &self.image
    }
    fn revalidate(&self, child: Option<&mut Child>) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if let Some(child) = child {
            ensure!(child.try_wait()?.is_none(), "fixture child exited");
        }
        Ok(())
    }
}
fn launch(
    location: &Location,
    session: wire::Session,
    endpoint: Endpoint,
    checks: Arc<AtomicUsize>,
) -> OwnedHost {
    OwnedHost::start(
        FixtureNative {
            image: std::env::current_exe().unwrap(),
            checks,
        },
        location,
        session,
        endpoint,
        vec!["--exact".into(), CHILD.into(), "--nocapture".into()],
    )
    .unwrap()
}
fn wait(mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(8);
    while !done() {
        assert!(Instant::now() < until, "fixture deadline");
        thread::sleep(Duration::from_millis(2));
    }
}

/// A real private OS child, but a synthetic verifier/protocol peer. It proves
/// fd/lock/EOF ownership and resource retention, never native Mac code identity.
#[test]
fn child_fixture() {
    if !std::env::args().any(|arg| arg == "--exact") || !std::env::args().any(|arg| arg == CHILD) {
        return;
    }
    let lock = unsafe { File::from_raw_fd(3) };
    assert!(platform::verify_private_file(&lock).unwrap());
    let mut stream = unsafe { UnixStream::from_raw_fd(0) };
    let mut reader = io::BufReader::new(stream.try_clone().unwrap());
    let mut line = Vec::new();
    reader
        .by_ref()
        .take(wire::FRAME_BYTES as u64)
        .read_until(b'\n', &mut line)
        .unwrap();
    let wire::ToHost::Hello { session, abi } = wire::decode(&line).unwrap() else {
        panic!("hello required");
    };
    assert_eq!(abi, wire::ABI);
    stream
        .write_all(
            &wire::encode(&wire::FromHost::Ready {
                abi,
                application: wire::APPLICATION.into(),
                session: session.clone(),
            })
            .unwrap(),
        )
        .unwrap();
    loop {
        line.clear();
        reader
            .by_ref()
            .take(wire::FRAME_BYTES as u64)
            .read_until(b'\n', &mut line)
            .unwrap();
        if line.is_empty() {
            break;
        }
        match wire::decode::<wire::ToHost>(&line).unwrap() {
            wire::ToHost::Command(command) => {
                let wire::Action::Post { token, body, .. } = &command.action else {
                    panic!("fixture post");
                };
                let native_id = wire::NativeId::Macos {
                    request_id: zerus_client_core::attention::request_id(token),
                };
                stream
                    .write_all(
                        &wire::encode(&wire::FromHost::Receipt(wire::Receipt {
                            binding: command.binding.clone(),
                            outcome: wire::Outcome::Delivered { native_id },
                        }))
                        .unwrap(),
                    )
                    .unwrap();
                let gate = PathBuf::from(body);
                wait(|| gate.is_file());
                stream
                    .write_all(
                        &wire::encode(&wire::FromHost::Released {
                            binding: command.binding,
                        })
                        .unwrap(),
                    )
                    .unwrap();
            }
            wire::ToHost::Stop { session: observed } => {
                assert_eq!(observed, session);
                stream
                    .write_all(&wire::encode(&wire::FromHost::Stopped { session }).unwrap())
                    .unwrap();
                break;
            }
            wire::ToHost::Restore { .. } => (),
            _ => panic!("fixture protocol"),
        }
    }
}

#[test]
fn original_child_inherits_owner_lock_and_receipt_waits_for_actual_release() {
    let temp = tempfile::tempdir().unwrap();
    let location = Location {
        root: temp.path().join("state/desktop"),
        configuration: "a".repeat(64),
    };
    location.prepare().unwrap();
    let session = wire::Session {
        configuration: location.configuration.clone(),
        service_epoch: id(),
        writer_epoch: id(),
        adapter: id(),
        host_epoch: id(),
        lease: id(),
        platform: wire::Platform::Macos,
    };
    let mut store = Store::open(&location, &session.service_epoch).unwrap();
    ledger::authorize(&mut store.db, &session.writer_epoch, &session.service_epoch).unwrap();
    let (mut outbox, endpoint) = super::super::Outbox::new(session.clone()).unwrap();
    let checks = Arc::new(AtomicUsize::new(0));
    let mut host = launch(&location, session.clone(), endpoint, checks.clone());
    wait(|| host.status()["transport_ready"] == true);
    assert_eq!(host.status()["ready"], false);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(location.root.join("notification-host.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&file).is_err());
    let (_, second_endpoint) = super::super::Outbox::new(session.clone()).unwrap();
    assert!(OwnedHost::start(
        FixtureNative {
            image: std::env::current_exe().unwrap(),
            checks: checks.clone()
        },
        &location,
        session.clone(),
        second_endpoint,
        vec![]
    )
    .is_err());
    let gate = temp.path().join("release");
    let token = zerus_client_core::attention::encode_token(json!({"host":"peer.example","name":"fixture","run":"fixture-run","conversation":"fixture-conversation","notice":id()}).as_object().unwrap());
    let intent = ledger::put_intent(
        &mut store.db,
        &ledger::Identity {
            kind: ledger::Kind::Post,
            target: ledger::Target::Session {
                computer: "peer.example".into(),
                run: "fixture-run".into(),
                conversation: "fixture-conversation".into(),
            },
            edge: id(),
        },
        1,
        &serde_json::to_value(wire::Action::Post {
            token,
            title: "Fixture".into(),
            body: gate.to_str().unwrap().into(),
        })
        .unwrap(),
    )
    .unwrap();
    let request = ledger::Reserve {
        request_id: id(),
        intent_id: intent,
        revision: 1,
        epoch: session.writer_epoch,
        service_epoch: session.service_epoch,
        adapter: session.adapter,
        lease: session.lease,
        retry_of: None,
    };
    outbox
        .offer(&mut store, request.clone(), Instant::now())
        .unwrap();
    let turn = host.status()["turns"].as_u64().unwrap_or(0);
    wait(|| host.status()["turns"].as_u64().unwrap_or(0) >= turn + 2);
    assert_eq!(
        checks.load(Ordering::SeqCst),
        2,
        "new native work waits for restoration"
    );
    assert!(outbox.pending[0].shared.reply.lock().unwrap().is_none());
    host.finish_restoration();
    wait(|| host.status()["ready"] == true);
    wait(|| {
        outbox.poll(&mut store, Instant::now());
        outbox
            .pending
            .first()
            .is_some_and(|p| p.shared.reply.lock().unwrap().is_some())
    });
    let state = |store: &Store| -> String {
        store
            .db
            .query_row(
                "SELECT state FROM desktop_attempts WHERE request_id=?1",
                [&request.request_id],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(
        state(&store),
        "reserved",
        "a receipt alone cannot free native resources"
    );
    std::fs::write(&gate, b"release").unwrap();
    wait(|| {
        outbox.poll(&mut store, Instant::now());
        state(&store) == "delivered"
    });
    assert!(
        checks.load(Ordering::SeqCst) >= 3,
        "selection/child are rechecked before dispatch"
    );
    host.request_stop();
    wait(|| host.poll_drained());
    assert_eq!(host.status()["exit"], 0);
    assert!(host.status()["fault"].is_null(), "{}", host.status());
    assert!(fs2::FileExt::try_lock_exclusive(&file).is_ok());
}

#[test]
fn framing_retains_coalesced_messages_and_refuses_a_partial_eof() {
    let (mut read, mut write) = UnixStream::pair().unwrap();
    read.set_nonblocking(true).unwrap();
    let frame = wire::FromHost::Stopped {
        session: wire::Session {
            configuration: "a".repeat(64),
            service_epoch: id(),
            writer_epoch: id(),
            adapter: id(),
            host_epoch: id(),
            lease: id(),
            platform: wire::Platform::Macos,
        },
    };
    let bytes = wire::encode(&frame).unwrap();
    write.write_all(&bytes).unwrap();
    write.write_all(&bytes).unwrap();
    let mut input = Vec::new();
    let mut eof = false;
    assert!(read_frame(&mut read, &mut input, &mut eof)
        .unwrap()
        .is_none());
    assert_eq!(
        read_frame(&mut read, &mut input, &mut eof).unwrap(),
        Some(frame.clone())
    );
    assert_eq!(
        read_frame(&mut read, &mut input, &mut eof).unwrap(),
        Some(frame)
    );
    write.write_all(b"{").unwrap();
    drop(write);
    assert!(read_frame(&mut read, &mut input, &mut eof)
        .unwrap()
        .is_none());
    assert!(read_frame(&mut read, &mut input, &mut eof).is_err());
    assert!(eof);
}

#[test]
fn logs_keep_a_bounded_prefix_and_still_observe_original_eof() {
    let (mut read, mut write) = UnixStream::pair().unwrap();
    read.set_nonblocking(true).unwrap();
    let mut prefix = vec![b'a'; LOG_BYTES - 1];
    let mut eof = false;
    write.write_all(b"xyz").unwrap();
    drop(write);
    assert!(log_turn(&mut read, &mut prefix, &mut eof).unwrap());
    assert_eq!(prefix.len(), LOG_BYTES);
    assert_eq!(prefix[LOG_BYTES - 1], b'x');
    assert!(!eof);
    assert!(!log_turn(&mut read, &mut prefix, &mut eof).unwrap());
    assert!(eof);
}
