use super::super::{Offered, Outbox};
use super::ingress::test_bus as bus;
use super::*;
use crate::desktop::store::{Location, Store};
use std::sync::atomic::AtomicU8;
use zerus_desktop_ledger::{self as ledger, Identity, Kind, Reserve, Target};

fn session() -> Session {
    Session {
        configuration: "a".repeat(64),
        service_epoch: uuid::Uuid::new_v4().to_string(),
        writer_epoch: uuid::Uuid::new_v4().to_string(),
        adapter: uuid::Uuid::new_v4().to_string(),
        host_epoch: uuid::Uuid::new_v4().to_string(),
        lease: uuid::Uuid::new_v4().to_string(),
        platform: Platform::Linux,
    }
}
fn command(session: Session) -> Command {
    let token = zerus_client_core::attention::encode_token(
        json!({
            "host":"peer.example","run":"run-1","conversation":"conversation-1","name":"fixture",
        "notice":uuid::Uuid::new_v4().to_string(),
        })
        .as_object()
        .unwrap(),
    );
    Command {
        binding: wire::Binding {
            session,
            request_id: uuid::Uuid::new_v4().to_string(),
            intent_id: "b".repeat(64),
            revision: 1,
            seq: 1,
        },
        action: Action::Post {
            token,
            title: "Needs attention".into(),
            body: "<b>literal & text</b>".into(),
        },
    }
}
fn namespace() -> Namespace {
    Namespace {
        bus_id: "a".repeat(32),
        owner: ":1.42".into(),
    }
}

#[test]
fn mapping_restoration_is_read_only_exact_and_never_replaces_an_id() {
    let mut map = Mappings::new(namespace());
    let command = command(session());
    assert!(matches!(map.adopt(&command, 42), Outcome::Delivered { .. }));
    let value = map.rows[&42].clone();
    assert!(map.restore(vec![value.clone()]));
    let mut changed = value.clone();
    changed.request_id = uuid::Uuid::new_v4().to_string();
    assert!(!map.restore(vec![changed]));
    assert_eq!(map.rows[&42], value);
    let mut changed = value.clone();
    if let NativeId::Linux { bus_id, .. } = &mut changed.native_id {
        *bus_id = "b".repeat(32);
    }
    assert!(!map.restore(vec![changed]));
    assert!(matches!(map.adopt(&command, 42), Outcome::Unknown { .. }));
    assert!(matches!(map.adopt(&command, 0), Outcome::Unknown { .. }));
}

#[test]
fn callback_context_is_ephemeral_and_closed_removes_the_exact_mapping() {
    let s = session();
    let (outbox, endpoint) = super::super::Outbox::new(s.clone()).unwrap();
    let mut map = Mappings::new(namespace());
    let command = command(s);
    map.adopt(&command, 42);
    assert!(map.signal(
        Signal::ActivationToken {
            id: 42,
            token: "wayland-activation".into()
        },
        &endpoint
    ));
    assert!(map.rows[&42].platform_context.is_none());
    assert!(map.signal(
        Signal::ActionInvoked {
            id: 42,
            action: "default".into()
        },
        &endpoint
    ));
    let clicked = outbox.events.try_recv().unwrap();
    assert!(matches!(&clicked.event, Event::Clicked(v)
        if matches!(&v.platform_context,Some(PlatformContext::Wayland { activation_token }) if activation_token=="wayland-activation")));
    assert!(map.tokens.is_empty());
    assert!(map.signal(Signal::NotificationClosed { id: 42, reason: 2 }, &endpoint));
    assert!(map.rows.is_empty());
    let closed = outbox.events.try_recv().unwrap();
    assert!(matches!(&closed.event, Event::Closed(v) if v.platform_context.is_none()));
    assert!(map.signal(
        Signal::ActionInvoked {
            id: 42,
            action: "default".into()
        },
        &endpoint
    ));
}

#[test]
fn mapping_capacity_never_evicts_and_a_restore_page_is_all_or_nothing() {
    let mut map = Mappings::new(namespace());
    let command = command(session());
    for id in 1..wire::MAPPINGS as u32 {
        assert!(matches!(map.adopt(&command, id), Outcome::Delivered { .. }));
    }
    let mut one = map.rows[&1].clone();
    let mut two = one.clone();
    if let NativeId::Linux { id, .. } = &mut one.native_id {
        *id = wire::MAPPINGS as u32;
    }
    if let NativeId::Linux { id, .. } = &mut two.native_id {
        *id = wire::MAPPINGS as u32 + 1;
    }
    assert!(!map.restore(vec![one.clone(), two]));
    assert_eq!(map.rows.len(), wire::MAPPINGS - 1);
    assert!(map.restore(vec![one]));
    assert!(matches!(
        map.adopt(&command, wire::MAPPINGS as u32 + 1),
        Outcome::Unknown { .. }
    ));
    assert!(map.rows.contains_key(&1));
    assert_eq!(map.rows.len(), wire::MAPPINGS);
}

#[test]
fn cleanup_keeps_the_first_deadline_without_a_mutex_or_renewal() {
    let scope = Shared {
        stop: AtomicBool::new(false),
        ready: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        born: Instant::now(),
        cleanup: AtomicU64::new(0),
    };
    let first = scope.cleanup_deadline();
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(scope.cleanup_deadline(), first);
}

#[test]
fn native_reply_requires_the_pinned_sender_and_method_return_type() {
    let call = zbus::Message::method_call("/", "Example")
        .unwrap()
        .build(&())
        .unwrap();
    for owner in [":1.42", ":1.99"] {
        let reply = zbus::Message::method_return(&call.header())
            .unwrap()
            .sender(owner)
            .unwrap()
            .build(&42_u32)
            .unwrap();
        assert_eq!(worker::reply_from(&reply, ":1.42"), owner == ":1.42");
    }
    let signal = zbus::Message::signal("/", "org.example.Fixture", "Example")
        .unwrap()
        .sender(":1.42")
        .unwrap()
        .build(&42_u32)
        .unwrap();
    assert!(!worker::reply_from(&signal, ":1.42"));
}

#[test]
fn unsupported_action_consumes_its_ephemeral_token_without_a_click() {
    let s = session();
    let (outbox, endpoint) = Outbox::new(s.clone()).unwrap();
    let mut mappings = Mappings::new(namespace());
    mappings.adopt(&command(s), 42);
    assert!(mappings.signal(
        Signal::ActivationToken {
            id: 42,
            token: "one-use".into()
        },
        &endpoint
    ));
    assert!(mappings.signal(
        Signal::ActionInvoked {
            id: 42,
            action: "unsupported".into()
        },
        &endpoint
    ));
    assert!(mappings.tokens.is_empty());
    assert!(outbox.events.try_recv().is_err());
}

fn until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT + Duration::from_secs(1);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "private notification fixture deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
struct Fake {
    called: mpsc::SyncSender<String>,
    closed: mpsc::SyncSender<u32>,
    mode: Arc<AtomicU8>,
}
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Fake {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        _app: String,
        _replace: u32,
        _icon: String,
        _title: String,
        body: String,
        _actions: Vec<String>,
        _hints: BTreeMap<String, zbus::zvariant::OwnedValue>,
        _timeout: i32,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<u32> {
        self.called.try_send(body).unwrap();
        match self.mode.load(Ordering::Acquire) {
            1 => Err(zbus::fdo::Error::Failed("fixture error".into())),
            2 => Ok(0),
            3 => {
                connection
                    .emit_signal(
                        None::<&str>,
                        native_path(),
                        zerus_freedesktop_notify::SERVICE,
                        "NotificationClosed",
                        &(42_u32, 2_u32),
                    )
                    .await
                    .unwrap();
                Ok(42)
            }
            _ => Ok(42),
        }
    }
    async fn close_notification(&self, id: u32, #[zbus(connection)] connection: &zbus::Connection) {
        self.closed.try_send(id).unwrap();
        connection
            .emit_signal(
                None::<&str>,
                native_path(),
                zerus_freedesktop_notify::SERVICE,
                "NotificationClosed",
                &(id, 2_u32),
            )
            .await
            .unwrap();
    }
}
fn native_path() -> &'static str {
    zerus_freedesktop_notify::PATH
}
struct Server {
    connection: zbus::blocking::Connection,
    called: mpsc::Receiver<String>,
    closed: mpsc::Receiver<u32>,
    mode: Arc<AtomicU8>,
}
impl Server {
    fn new(bus: &bus::Bus) -> Self {
        let (called, calls) = mpsc::sync_channel(8);
        let (closed, closes) = mpsc::sync_channel(8);
        let mode = Arc::new(AtomicU8::new(0));
        let connection = zbus::blocking::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name(zerus_freedesktop_notify::SERVICE)
            .unwrap()
            .serve_at(
                native_path(),
                Fake {
                    called,
                    closed,
                    mode: mode.clone(),
                },
            )
            .unwrap()
            .method_timeout(WAIT)
            .build()
            .unwrap();
        Self {
            connection,
            called: calls,
            closed: closes,
            mode,
        }
    }
    fn click(&self, id: u32) {
        self.connection
            .emit_signal(
                None::<&str>,
                native_path(),
                zerus_freedesktop_notify::SERVICE,
                "ActivationToken",
                &(id, "fixture-wayland"),
            )
            .unwrap();
        self.connection
            .emit_signal(
                None::<&str>,
                native_path(),
                zerus_freedesktop_notify::SERVICE,
                "ActionInvoked",
                &(id, "default"),
            )
            .unwrap();
    }
}
struct Harness {
    _root: tempfile::TempDir,
    store: Store,
    outbox: Outbox,
    endpoint: Option<Endpoint>,
}
impl Harness {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let location = Location {
            root: root.path().canonicalize().unwrap().join("state/desktop"),
            configuration: "a".repeat(64),
        };
        location.prepare().unwrap();
        let session = session();
        let mut store = Store::open(&location, &session.service_epoch).unwrap();
        ledger::authorize(&mut store.db, &session.writer_epoch, &session.service_epoch).unwrap();
        let (outbox, endpoint) = Outbox::new(session).unwrap();
        Self {
            _root: root,
            store,
            outbox,
            endpoint: Some(endpoint),
        }
    }
    fn start(&mut self, address: &str) -> Runtime {
        Runtime::start(
            self.outbox.session.clone(),
            self.endpoint.take().unwrap(),
            address.to_owned(),
        )
        .unwrap()
    }
    fn activate(runtime: &Runtime) {
        let ack = runtime.finish_restoration().unwrap();
        until(|| ack.poll() == Some(true));
        assert_eq!(runtime.status()["ready"], true);
    }
    fn offer(&mut self, action: Action, kind: Kind) -> Reserve {
        let id = ledger::put_intent(
            &mut self.store.db,
            &Identity {
                kind,
                target: Target::Session {
                    computer: "peer.example".into(),
                    run: "run-1".into(),
                    conversation: "conversation-1".into(),
                },
                edge: uuid::Uuid::new_v4().to_string(),
            },
            1,
            &serde_json::to_value(action).unwrap(),
        )
        .unwrap();
        let s = &self.outbox.session;
        let request = Reserve {
            request_id: uuid::Uuid::new_v4().to_string(),
            intent_id: id,
            revision: 1,
            epoch: s.writer_epoch.clone(),
            service_epoch: s.service_epoch.clone(),
            adapter: s.adapter.clone(),
            lease: s.lease.clone(),
            retry_of: None,
        };
        assert!(matches!(
            self.outbox
                .offer(&mut self.store, request.clone(), Instant::now())
                .unwrap(),
            Offered::Queued { .. }
        ));
        request
    }
    fn post(&mut self) -> Reserve {
        self.offer(command(self.outbox.session.clone()).action, Kind::Post)
    }
    fn state(&self, request: &Reserve) -> String {
        self.store
            .db
            .query_row(
                "SELECT state FROM desktop_attempts WHERE request_id=?1",
                [&request.request_id],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn settle(&mut self, request: &Reserve, state: &str) {
        until(|| {
            self.outbox.poll(&mut self.store, Instant::now());
            self.state(request) == state
        });
    }
    fn mappings(&self) -> Vec<Activation> {
        let head = self.outbox.restore_head(&self.store).unwrap();
        self.outbox
            .restore_page(&self.store, 0, head)
            .unwrap()
            .mappings
    }
    fn stop(&mut self, runtime: &mut Runtime) {
        self.outbox.request_stop();
        runtime.request_stop();
        until(|| {
            self.outbox.poll(&mut self.store, Instant::now());
            runtime.drained() && self.outbox.operation_drained()
        });
    }
}

#[test]
fn actual_worker_waits_for_restoration_then_posts_clicks_and_withdraws_once() {
    let bus = bus::Bus::new();
    let server = Server::new(&bus);
    let mut h = Harness::new();
    let mut runtime = h.start(&bus.address);
    let request = h.post();
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        server.called.try_recv().is_err(),
        "restoration gate precedes Endpoint consumption"
    );
    Harness::activate(&runtime);
    assert_eq!(
        server.called.recv_timeout(WAIT).unwrap(),
        "&lt;b&gt;literal &amp; text&lt;/b&gt;"
    );
    h.settle(&request, "delivered");
    let original = h.mappings().remove(0);
    assert!(
        matches!(&original.native_id, NativeId::Linux { owner, id:42, .. }
        if owner==server.connection.unique_name().unwrap().as_str())
    );
    server.click(42);
    let mut clicked = Vec::new();
    until(|| {
        clicked.extend(h.outbox.poll(&mut h.store, Instant::now()));
        !clicked.is_empty()
    });
    assert_eq!(clicked.len(), 1);
    assert_eq!(clicked[0].request_id, request.request_id);
    assert!(
        matches!(&clicked[0].platform_context,Some(PlatformContext::Wayland { activation_token }) if activation_token=="fixture-wayland")
    );
    assert!(h.mappings()[0].platform_context.is_none());
    ledger::withdraw(&mut h.store.db, &request.intent_id, 1).unwrap();
    let withdrawal = h.offer(
        Action::Withdraw {
            token: original.token,
            native_id: original.native_id,
        },
        Kind::Withdrawal,
    );
    assert_eq!(server.closed.recv_timeout(WAIT).unwrap(), 42);
    h.settle(&withdrawal, "delivered");
    assert!(server.called.try_recv().is_err());
    h.stop(&mut runtime);
}

#[test]
fn actual_native_errors_and_zero_ids_are_unknown_without_replay() {
    for mode in [1, 2] {
        let bus = bus::Bus::new();
        let server = Server::new(&bus);
        server.mode.store(mode, Ordering::Release);
        let mut h = Harness::new();
        let mut runtime = h.start(&bus.address);
        Harness::activate(&runtime);
        let request = h.post();
        server.called.recv_timeout(WAIT).unwrap();
        h.settle(&request, "unknown");
        assert!(h.mappings().is_empty());
        assert!(matches!(
            h.outbox
                .offer(&mut h.store, request, Instant::now())
                .unwrap(),
            Offered::Existing(_)
        ));
        assert!(server.called.try_recv().is_err());
        h.stop(&mut runtime);
    }
}

#[test]
fn stopping_during_restoration_refuses_queued_work_and_joins_the_original_host() {
    let bus = bus::Bus::new();
    let server = Server::new(&bus);
    let mut h = Harness::new();
    let mut runtime = h.start(&bus.address);
    let ack = runtime.restore(Vec::new()).unwrap();
    until(|| ack.poll() == Some(true));
    let request = h.post();
    h.stop(&mut runtime);
    assert_eq!(h.state(&request), "refused");
    assert!(server.called.try_recv().is_err());
}

#[test]
fn actual_early_close_keeps_original_dispatch_charge_and_never_resurrects() {
    let bus = bus::Bus::new();
    let server = Server::new(&bus);
    server.mode.store(3, Ordering::Release);
    let mut h = Harness::new();
    let mut runtime = h.start(&bus.address);
    Harness::activate(&runtime);
    let request = h.post();
    let held = h.outbox.pending[0].shared.clone();
    server.called.recv_timeout(WAIT).unwrap();
    until(|| h.outbox.event_count.load(Ordering::Acquire) > 0);
    h.outbox.poll(&mut h.store, Instant::now());
    assert_eq!(h.state(&request), "reserved");
    drop(held);
    h.settle(&request, "delivered");
    assert!(h.mappings().is_empty());
    h.stop(&mut runtime);
}

#[test]
fn actual_restore_rebuilds_callbacks_without_notify_and_rejects_other_daemon() {
    let bus = bus::Bus::new();
    let server = Server::new(&bus);
    let mut h = Harness::new();
    let mut runtime = h.start(&bus.address);
    Harness::activate(&runtime);
    let request = h.post();
    server.called.recv_timeout(WAIT).unwrap();
    h.settle(&request, "delivered");
    let values = h.mappings();
    h.stop(&mut runtime);
    let mut session = h.outbox.session.clone();
    session.adapter = uuid::Uuid::new_v4().to_string();
    session.host_epoch = uuid::Uuid::new_v4().to_string();
    session.lease = uuid::Uuid::new_v4().to_string();
    let (outbox, endpoint) = Outbox::new(session).unwrap();
    h.outbox = outbox;
    h.endpoint = Some(endpoint);
    let mut runtime = h.start(&bus.address);
    let ack = runtime.restore(values).unwrap();
    until(|| ack.poll() == Some(true));
    Harness::activate(&runtime);
    assert!(server.called.try_recv().is_err());
    server.click(42);
    let mut clicked = Vec::new();
    until(|| {
        clicked.extend(h.outbox.poll(&mut h.store, Instant::now()));
        !clicked.is_empty()
    });
    assert_eq!(clicked[0].request_id, request.request_id);
    server
        .connection
        .release_name(zerus_freedesktop_notify::SERVICE)
        .unwrap();
    let replacement = Server::new(&bus);
    until(|| runtime.shared.failed.load(Ordering::Acquire));
    until(|| runtime.thread.as_ref().is_some_and(JoinHandle::is_finished));
    assert!(
        !runtime.drained(),
        "uncertain namespace loss never reports CleanExit"
    );
    assert!(replacement.called.try_recv().is_err());
    assert!(replacement.closed.try_recv().is_err());
}

#[test]
fn actual_stop_drains_ordered_closed_callbacks_but_never_opens_a_late_click() {
    let bus = bus::Bus::new();
    let server = Server::new(&bus);
    let mut h = Harness::new();
    let mut runtime = h.start(&bus.address);
    Harness::activate(&runtime);
    let request = h.post();
    server.called.recv_timeout(WAIT).unwrap();
    h.settle(&request, "delivered");
    let original = h.mappings().remove(0);
    h.stop(&mut runtime);
    let mut next = h.outbox.session.clone();
    next.adapter = uuid::Uuid::new_v4().to_string();
    next.host_epoch = uuid::Uuid::new_v4().to_string();
    next.lease = uuid::Uuid::new_v4().to_string();
    let (outbox, endpoint) = Outbox::new(next).unwrap();
    h.outbox = outbox;
    assert!(worker::test_close_with_queued_callbacks(
        bus.address.clone(),
        endpoint,
        original.clone(),
        || {
            h.outbox.request_stop();
            server.click(42);
            server
                .connection
                .emit_signal(
                    None::<&str>,
                    native_path(),
                    zerus_freedesktop_notify::SERVICE,
                    "NotificationClosed",
                    &(42_u32, 2_u32),
                )
                .unwrap();
        }
    ));
    assert_eq!(h.outbox.status()["callbacks"], 1);
    assert!(h.outbox.poll(&mut h.store, Instant::now()).is_empty());
    assert!(h
        .outbox
        .current_activation(&h.store, &original)
        .unwrap()
        .is_none());
    assert!(h.outbox.operation_drained());
}
