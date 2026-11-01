//! One read-only desktop subscriber. No daemon launch, legacy fallback or writer admission.
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc, Mutex, OnceLock, PoisonError, Weak,
    },
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    sync::{oneshot, watch, Notify},
};
use zerus_client_core::{
    desktop::{
        drain::{Gate, Role},
        service::{self, CatalogPage, Snapshot},
    },
    Change, ComputerId, ComputerStatus,
};

const RPC_WAIT: Duration = Duration::from_secs(5);
const HELP: &str = "Desktop service ABI: desktop-v1";
const SOURCES: usize = 64;
const KEY_BYTES: usize = 32 << 20;
const CATALOG_KEYS: usize = 1_000_000;

/// Registered at setup before start_source. Some contains a verified service
/// watch frame; None invalidates the prior projection. Callbacks must not block.
pub type Projection = Arc<dyn Fn(Option<&Value>) + Send + Sync>;

/// Captured before startup exclusion and before any producer is constructed.
pub fn mode() -> bool {
    static MODE: OnceLock<bool> = OnceLock::new();
    *MODE.get_or_init(|| service::enabled(std::env::var(service::MODE_ENV).ok().as_deref()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Retry(u64),
    Stop,
}

#[derive(Default)]
struct Dirty {
    versions: BTreeMap<String, u64>,
    sources: BTreeMap<String, SourceVersion>,
    pending: BTreeSet<String>,
    next: u64,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct SourceVersion {
    incarnation: u64,
    generation: u64,
}
impl SourceVersion {
    fn from(value: &Value) -> Self {
        Self {
            incarnation: value["incarnation"].as_u64().unwrap_or(0),
            generation: value["generation"].as_u64().unwrap_or(0),
        }
    }
}

pub enum Data {
    Change(Change),
    Catalog(CatalogDelivery),
}

/// Recheck identity at the view drain, not just when an RPC finishes. Frames
/// already queued before a removal, reconnect or new generation cannot reappear.
pub struct CatalogDelivery {
    shared: Weak<Shared>,
    computer: String,
    epoch: String,
    source: SourceVersion,
    stopped: Arc<AtomicBool>,
    value: CatalogValue,
}
enum CatalogValue {
    Row(Change),
    Finish {
        version: u64,
        complete: bool,
        keys: BTreeSet<String>,
        done: oneshot::Sender<()>,
    },
}
impl CatalogDelivery {
    pub fn apply(self, pump: &crate::pump::Pump, observe: &impl Fn(&Change)) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        // Same lock order as Shared::current. Hold admission through the push so
        // an invalidation cannot race the last membership decision.
        let snapshot = shared
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let dirty = shared.dirty.lock().unwrap_or_else(PoisonError::into_inner);
        if self.stopped.load(Ordering::Acquire)
            || shared.stopped.load(Ordering::Acquire)
            || snapshot.epoch.as_deref() != Some(&self.epoch)
            || dirty.sources.get(&self.computer) != Some(&self.source)
        {
            return;
        }
        match self.value {
            CatalogValue::Row(change) => {
                observe(&change);
                pump.push(change);
            }
            CatalogValue::Finish {
                version,
                complete,
                keys,
                done,
            } => {
                let clean = complete && dirty.versions.get(&self.computer) == Some(&version);
                let computer = ComputerId(self.computer);
                if clean {
                    pump.service_catalog_finish(&computer, &keys);
                }
                pump.push(Change::Computer {
                    computer,
                    status: if clean {
                        ComputerStatus::Live
                    } else {
                        ComputerStatus::Reconnecting { attempt: 0 }
                    },
                });
                let _ = done.send(());
            }
        }
    }
}

struct Shared {
    hgs: PathBuf,
    pump: crate::pump::Pump,
    sink: Mutex<Option<SyncSender<Data>>>,
    snapshot: Mutex<Snapshot>,
    dirty: Mutex<Dirty>,
    changed: Notify,
    rpc: Gate,
    allowed: bool,
    stopped: AtomicBool,
    cancellation: Mutex<Option<Arc<AtomicBool>>>,
    responder: Option<Arc<zerus_client_core::desktop::Responder>>,
    reads: Mutex<BTreeMap<String, ReadIntent>>,
    projections: Vec<Projection>,
}

struct ReadIntent {
    id: String,
    in_flight: bool,
    uncertain: bool,
}

pub struct Service {
    shared: Arc<Shared>,
    control: watch::Sender<Control>,
    join: Mutex<Option<oneshot::Receiver<()>>>,
    drain: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Service {
    pub fn start(
        hgs: PathBuf,
        pump: crate::pump::Pump,
        sink: SyncSender<Data>,
        allowed: bool,
        drain: std::thread::JoinHandle<()>,
        responder: Option<Arc<zerus_client_core::desktop::Responder>>,
        projections: Vec<Projection>,
    ) -> Arc<Self> {
        let shared = Arc::new(Shared {
            hgs,
            pump,
            sink: Mutex::new(Some(sink)),
            snapshot: Mutex::new(Snapshot::default()),
            dirty: Mutex::new(Dirty::default()),
            changed: Notify::new(),
            rpc: Gate::new(4).unwrap(),
            allowed,
            stopped: AtomicBool::new(false),
            cancellation: Mutex::new(None),
            responder,
            reads: Mutex::new(BTreeMap::new()),
            projections,
        });
        let (control, receiver) = watch::channel(Control::Retry(0));
        let (finished, join) = oneshot::channel();
        let service = Arc::new(Self {
            shared: shared.clone(),
            control,
            join: Mutex::new(Some(join)),
            drain: Mutex::new(Some(drain)),
        });
        shared.publish(Snapshot::default());
        tauri::async_runtime::spawn(async move {
            run(shared, receiver).await;
            let _ = finished.send(());
        });
        service
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
    pub fn retry(&self) {
        if self.shared.stopped.load(Ordering::Acquire) {
            return;
        }
        if let Some(cancel) = self
            .shared
            .cancellation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            cancel.store(true, Ordering::Release);
        }
        self.control.send_modify(|control| {
            if let Control::Retry(n) = control {
                *control = n.checked_add(1).map_or(Control::Stop, Control::Retry);
            }
        });
    }
    pub async fn read_marks(
        &self,
        marks: Vec<zerus_client_core::readmarks::ReadMark>,
    ) -> Result<Vec<zerus_client_core::readmarks::ReadMark>, String> {
        if marks.len() > service::CATALOG_ROWS {
            return Err("Select at most 128 read marks at once.".into());
        }
        let mut committed = Vec::new();
        for mark in marks {
            if !self.snapshot().component_ready("reads") {
                return Err(
                    "Read marks are unavailable until the background service is ready.".into(),
                );
            }
            let key = serde_json::to_string(&mark).map_err(|_| "Invalid read intent.")?;
            if key.len() > zerus_watch_protocol::CONTROL_LINE_BYTES / 2 || mark.is_empty() {
                return Err("Invalid read intent.".into());
            }
            let (id, uncertain) = {
                let mut reads = self
                    .shared
                    .reads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if reads.len() >= 256 && !reads.contains_key(&key) {
                    return Err(
                        "Too many unconfirmed read intents. Retry after the service reconnects."
                            .into(),
                    );
                }
                let intent = reads.entry(key.clone()).or_insert_with(|| ReadIntent {
                    id: uuid::Uuid::new_v4().to_string(),
                    in_flight: false,
                    uncertain: false,
                });
                if intent.in_flight {
                    return Err("This read intent is already being submitted.".into());
                }
                intent.in_flight = true;
                (intent.id.clone(), intent.uncertain)
            };
            let stop = self
                .shared
                .cancellation
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .ok_or("Background service is unavailable.")?;
            let result = async {
                if uncertain {
                    let query = uuid::Uuid::new_v4().to_string();
                    let receipt = request(
                        &self.shared,
                        json!({"op":"delivery_result","request_id":query,"original_request_id":id}),
                        stop.clone(),
                    )
                    .await?;
                    if receipt["request_id"] != query
                        || receipt["op"] != "delivery_result"
                        || receipt["original_request_id"] != id
                        || receipt["accepted"] != true
                    {
                        return Err("invalid_service_receipt");
                    }
                    if service::read_committed(&receipt["result"], &id) {
                        return Ok(());
                    }
                    if receipt["found"] != false {
                        return Err("read_commit_unconfirmed");
                    }
                }
                // Reissuing a read uses its SAME durable UUID, including after
                // an uncertain result. Policy-setting requests never use this lane.
                let receipt = request(
                    &self.shared,
                    json!({"op":"read_intent","request_id":id,"mark":mark}),
                    stop,
                )
                .await?;
                if service::read_committed(&receipt, &id) {
                    Ok(())
                } else {
                    Err("read_commit_unconfirmed")
                }
            }
            .await;
            {
                let mut reads = self
                    .shared
                    .reads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if result.is_ok() {
                    reads.remove(&key);
                } else if let Some(intent) = reads.get_mut(&key) {
                    intent.in_flight = false;
                    intent.uncertain = true;
                }
            }
            result.map_err(|_| "The background service has not confirmed this read mark. Retry keeps its request identity.".to_owned())?;
            committed.push(mark);
        }
        Ok(committed)
    }
    pub fn stop(&self) {
        self.shared.stopped.store(true, Ordering::Release);
        if let Some(cancel) = self
            .shared
            .cancellation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            cancel.store(true, Ordering::Release);
        }
        self.shared
            .sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        self.shared.rpc.close();
        let _ = self.control.send(Control::Stop);
    }
    pub async fn join(&self) -> Result<(), String> {
        self.stop();
        let receiver = self
            .join
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(receiver) = receiver {
            receiver
                .await
                .map_err(|_| "The service connection did not stop.".to_owned())?;
        }
        let drain = self
            .drain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(drain) = drain {
            tauri::async_runtime::spawn_blocking(move || {
                drain
                    .join()
                    .map_err(|_| "The service view did not stop.".to_owned())
            })
            .await
            .map_err(|error| error.to_string())??;
        }
        let closed = self.shared.rpc.close();
        let drained = tauri::async_runtime::spawn_blocking(move || {
            closed.wait_until(std::time::Instant::now() + RPC_WAIT)
        })
        .await
        .map_err(|error| error.to_string())?;
        if drained != zerus_client_core::desktop::drain::Observation::Idle {
            return Err("The service connection still has unconfirmed helper work.".into());
        }
        Ok(())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Shared {
    fn publish(&self, snapshot: Snapshot) {
        *self.snapshot.lock().unwrap_or_else(PoisonError::into_inner) = snapshot.clone();
        self.pump.service(snapshot);
    }
    fn unavailable(&self, code: &str) {
        for projection in &self.projections {
            projection(None);
        }
        self.publish(Snapshot {
            code: code.into(),
            ..Snapshot::default()
        });
    }
    fn dirty(&self, computer: &str, source: SourceVersion) -> Result<bool, &'static str> {
        let mut dirty = self.dirty.lock().unwrap_or_else(PoisonError::into_inner);
        if !dirty.versions.contains_key(computer) && dirty.versions.len() >= SOURCES {
            return Err("source_capacity");
        }
        dirty.next = dirty
            .next
            .checked_add(1)
            .ok_or("source_generation_exhausted")?;
        let next = dirty.next;
        dirty.versions.insert(computer.into(), next);
        let replaced = dirty
            .sources
            .insert(computer.into(), source)
            .is_some_and(|old| old != source);
        dirty.pending.insert(computer.into());
        self.changed.notify_one();
        Ok(replaced)
    }
    fn current(&self, computer: &str, source: SourceVersion, epoch: &str) -> bool {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .epoch
            .as_deref()
            == Some(epoch)
            && self
                .dirty
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .sources
                .get(computer)
                == Some(&source)
    }
    fn invalidate(&self) -> Vec<String> {
        let mut dirty = self.dirty.lock().unwrap_or_else(PoisonError::into_inner);
        let computers = dirty.versions.keys().cloned().collect();
        dirty.versions.clear();
        dirty.sources.clear();
        dirty.pending.clear();
        computers
    }
}

async fn emit(shared: &Shared, change: Change, stop: &AtomicBool) -> Result<(), &'static str> {
    emit_data(shared, Data::Change(change), stop).await
}
async fn emit_data(
    shared: &Shared,
    mut change: Data,
    stop: &AtomicBool,
) -> Result<(), &'static str> {
    loop {
        if stop.load(Ordering::Acquire) || shared.stopped.load(Ordering::Acquire) {
            return Err("stopped");
        }
        let sink = shared
            .sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("frontend_closed")?;
        match sink.try_send(change) {
            Ok(()) => return Ok(()),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return Err("frontend_closed"),
            Err(std::sync::mpsc::TrySendError::Full(value)) => change = value,
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

async fn command(
    shared: &Arc<Shared>,
    args: Vec<String>,
    stdin: Option<String>,
    stop: Arc<AtomicBool>,
) -> Result<Vec<u8>, &'static str> {
    if !shared.allowed {
        return Err("replay_transport_unavailable");
    }
    if shared.stopped.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
        return Err("stopped");
    }
    let charge = shared
        .rpc
        .admit(Role::Fleet)
        .map_err(|_| "service_transport_busy")?;
    let hgs = shared.hgs.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let inv = zerus_client_core::Invocation {
            args,
            stdin: stdin.map(String::into_bytes),
            timeout: RPC_WAIT,
            request_id: None,
        };
        let guard: Arc<dyn Send + Sync> = Arc::new(charge);
        match zerus_client_core::run::run_accounted(
            &hgs,
            &inv,
            &[],
            service::FRAME_BYTES,
            &stop,
            guard,
        ) {
            Ok(zerus_client_core::Exit::Code {
                code: 0, stdout, ..
            }) => Ok(stdout),
            _ => Err("service_request_unconfirmed"),
        }
    })
    .await
    .map_err(|_| "service_request_unconfirmed")?
}

async fn request(
    shared: &Arc<Shared>,
    value: Value,
    stop: Arc<AtomicBool>,
) -> Result<Value, &'static str> {
    // This lane never acquires frontend approval-setting authority.
    if value["op"] == "automatic_approvals" {
        return Err("frontend_route_required");
    }
    let input = serde_json::to_string(&value).map_err(|_| "invalid_service_request")?;
    if input.len() > zerus_watch_protocol::CONTROL_LINE_BYTES {
        return Err("service_request_too_large");
    }
    let bytes = command(
        shared,
        vec!["desktop".into(), "request".into(), "--json".into()],
        Some(input),
        stop,
    )
    .await?;
    serde_json::from_slice(&bytes).map_err(|_| "invalid_service_response")
}

async fn preflight(shared: &Arc<Shared>, stop: Arc<AtomicBool>) -> Result<String, &'static str> {
    let help = command(shared, vec!["--help".into()], None, stop.clone()).await?;
    if !std::str::from_utf8(&help)
        .ok()
        .is_some_and(|help| help.lines().any(|line| line.trim() == HELP))
    {
        return Err("incompatible_service_cli");
    }
    let bytes = command(
        shared,
        vec!["desktop".into(), "status".into(), "--json".into()],
        None,
        stop,
    )
    .await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_service_response")?;
    let configuration = value["configuration"]
        .as_str()
        .filter(|value| zerus_client_core::desktop::digest(value))
        .ok_or("incompatible_service")?;
    let mut state = service::State::new(configuration.into())?;
    state.status(&value)?;
    if let Some(responder) = &shared.responder {
        responder.configure_once(configuration.into());
    }
    shared.publish(state.snapshot().clone());
    Ok(configuration.into())
}

async fn run(shared: Arc<Shared>, mut control: watch::Receiver<Control>) {
    loop {
        let requested = *control.borrow_and_update();
        if requested == Control::Stop || shared.stopped.load(Ordering::Acquire) {
            break;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *shared
            .cancellation
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(stop.clone());
        let result = connection(shared.clone(), &mut control, stop.clone()).await;
        stop.store(true, Ordering::Release);
        let computers = shared.invalidate();
        shared.unavailable(result.err().unwrap_or("service_unavailable"));
        for computer in computers {
            let _ = emit(
                &shared,
                Change::Computer {
                    computer: ComputerId(computer),
                    status: ComputerStatus::Reconnecting { attempt: 0 },
                },
                &AtomicBool::new(false),
            )
            .await;
        }
        if *control.borrow() == Control::Stop {
            break;
        }
        // Retry is explicit and reuses this one worker after its children join.
        if *control.borrow() != requested {
            continue;
        }
        if control.changed().await.is_err() {
            break;
        }
    }
}

async fn connection(
    shared: Arc<Shared>,
    control: &mut watch::Receiver<Control>,
    stop: Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let configuration = preflight(&shared, stop.clone()).await?;
    if shared.stopped.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
        return Err("stopped");
    }
    let mut command = tokio::process::Command::new(&shared.hgs);
    command
        .args(["desktop", "watch", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::child_environment::restore(command.as_std_mut());
    command.env_remove("HGS_SESSION").env_remove("HGS_RUN_ID");
    let mut child = command.spawn().map_err(|_| "service_unavailable")?;
    // Keep stdin open: EOF is the proxy's explicit detach instruction.
    let stdin = child.stdin.take();
    let mut stdout = child.stdout.take().ok_or("service_unavailable")?;
    let mut stderr = child.stderr.take().ok_or("service_unavailable")?;
    let mut state = service::State::new(configuration)?;
    let mut buffer = Vec::new();
    let (mut out, mut err) = ([0u8; 8192], [0u8; 4096]);
    let (mut error_bytes, mut stderr_open) = (0usize, true);
    let catalog_shared = shared.clone();
    let catalog_stop = stop.clone();
    let mut catalogs =
        tauri::async_runtime::spawn(
            async move { catalog_loop(catalog_shared, catalog_stop).await },
        );
    let mut last_frame = tokio::time::Instant::now();
    let result = async {
        loop {
            tokio::select! {
                biased;
                _ = control.changed() => return Err("service_reconnecting"),
                _ = tokio::time::sleep_until(last_frame + Duration::from_secs(30)) => return Err("service_stale"),
                value = &mut catalogs => return value.map_err(|_| "catalog_unavailable")?,
                count = stdout.read(&mut out) => {
                    let count = count.map_err(|_| "service_unavailable")?;
                    if count == 0 { return Err("service_unavailable"); }
                    for part in out[..count].split_inclusive(|byte| *byte == b'\n') {
                        if buffer.len() + part.len() > service::FRAME_BYTES { return Err("service_frame_too_large"); }
                        buffer.extend_from_slice(part);
                        if !part.ends_with(b"\n") { continue; }
                        let frame: Value = serde_json::from_slice(&buffer).map_err(|_| "invalid_service_frame")?;
                        buffer.clear();
                        let update = state.frame(&frame)?;
                        for projection in &shared.projections { projection(Some(&frame)); }
                        last_frame = tokio::time::Instant::now();
                        shared.publish(state.snapshot().clone());
                        match update {
                            service::Update::Status => {
                                if let Some(computers) = state.snapshot().attention_counts.as_ref().and_then(|counts| counts["computers"].as_array()) {
                                    for computer in computers {
                                        if let Some(name) = computer["computer"].as_str() {
                                            let known = shared.dirty.lock().unwrap_or_else(PoisonError::into_inner).versions.contains_key(name);
                                            if !known { shared.dirty(name, SourceVersion::from(computer))?; }
                                        }
                                    }
                                }
                            }
                            service::Update::Catalog { computer, removed } => {
                                if removed {
                                    { let mut dirty = shared.dirty.lock().unwrap_or_else(PoisonError::into_inner); dirty.versions.remove(&computer); dirty.sources.remove(&computer); dirty.pending.remove(&computer); }
                                    emit(&shared, Change::Computer { computer: ComputerId(computer.clone()), status: ComputerStatus::Ended }, &stop).await?;
                                    emit(&shared, Change::Replace { computer: ComputerId(computer), rows: vec![] }, &stop).await?;
                                } else if shared.dirty(&computer, SourceVersion::from(&frame["value"]))? {
                                    emit(&shared, Change::Computer { computer: ComputerId(computer), status: ComputerStatus::Probing }, &stop).await?;
                                }
                            }
                            service::Update::Other => (),
                        }
                    }
                }
                count = stderr.read(&mut err), if stderr_open => {
                    let count = count.map_err(|_| "service_unavailable")?;
                    stderr_open = count != 0;
                    error_bytes += count;
                    if error_bytes > 64 << 10 { return Err("service_error_output_limit"); }
                }
            }
        }
    }.await;
    stop.store(true, Ordering::Release);
    shared.changed.notify_one();
    drop(stdin);
    // Only this owned local proxy is stopped; the desktop service and native agents remain untouched.
    if child.kill().await.is_err() && child.wait().await.is_err() {
        shared.stopped.store(true, Ordering::Release);
        if !catalogs.inner().is_finished() {
            let _ = catalogs.await;
        }
        return Err("service_transport_unverified");
    }
    if !catalogs.inner().is_finished() {
        let _ = catalogs.await;
    }
    result
}

#[tauri::command]
pub fn desktop_service_mode() -> bool {
    mode()
}

#[tauri::command]
pub fn desktop_service_retry(
    source: tauri::State<'_, crate::commands::AppState>,
) -> Result<(), String> {
    if !mode() {
        return Err("Background service mode is not enabled.".into());
    }
    if let Some(service) = source.desktop_service() {
        service.retry();
    } else {
        source.start_source()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn desktop_service_read_marks(
    source: tauri::State<'_, crate::commands::AppState>,
    marks: Vec<zerus_client_core::readmarks::ReadMark>,
) -> Result<Vec<zerus_client_core::readmarks::ReadMark>, String> {
    if !mode() {
        return Err("Background service mode is not enabled.".into());
    }
    let service = source
        .desktop_service()
        .ok_or("Background service is unavailable.")?;
    service.read_marks(marks).await
}

/// Reserved for the authenticated native frontend API. No generic policy route.
#[tauri::command(rename_all = "snake_case")]
pub async fn desktop_service_automatic_approvals(
    request_id: String,
    enabled: bool,
) -> Result<Value, String> {
    if !uuid::Uuid::parse_str(&request_id).is_ok_and(|id| id.to_string() == request_id) {
        return Err("Invalid approval request identity.".into());
    }
    let _ = enabled;
    Ok(
        json!({"request_id":request_id,"op":"automatic_approvals","accepted":false,"durable":false,"code":"frontend_route_unavailable"}),
    )
}

async fn catalog_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) -> Result<(), &'static str> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let work = {
            let mut dirty = shared.dirty.lock().unwrap_or_else(PoisonError::into_inner);
            dirty.pending.pop_first().and_then(|computer| {
                dirty
                    .versions
                    .get(&computer)
                    .zip(dirty.sources.get(&computer))
                    .map(|(version, source)| (computer, *version, *source))
            })
        };
        if let Some((computer, version, source)) = work {
            let epoch = shared
                .snapshot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .epoch
                .clone()
                .ok_or("service_unavailable")?;
            catalog_walk(&shared, &computer, version, source, &epoch, stop.clone()).await?;
        } else {
            shared.changed.notified().await;
        }
    }
}

async fn catalog_walk(
    shared: &Arc<Shared>,
    computer: &str,
    version: u64,
    source: SourceVersion,
    epoch: &str,
    stop: Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let (mut cursor, mut keys, mut key_bytes) = (None::<String>, BTreeSet::new(), 0usize);
    let result = async {
        loop {
            if stop.load(Ordering::Acquire) || !shared.current(computer, source, epoch) {
                return Ok(());
            }
            let id = uuid::Uuid::new_v4().to_string();
            let value = request(
                shared,
                json!({"op":"catalog_page","request_id":id,"computer":computer,"cursor":cursor}),
                stop.clone(),
            )
            .await?;
            let page = CatalogPage::checked(value, &id, epoch, computer)?;
            cursor = page.next_cursor;
            if stop.load(Ordering::Acquire) || !shared.current(computer, source, epoch) {
                return Ok(());
            }
            for item in page.items {
                if !shared.current(computer, source, epoch) {
                    return Ok(());
                }
                key_bytes += item.key.len();
                if key_bytes > KEY_BYTES
                    || keys.len() >= CATALOG_KEYS
                    || !keys.insert(item.key.clone())
                {
                    return Err("catalog_capacity_or_duplicate");
                }
                let row = zerus_client_core::view::row(&item.row);
                emit_data(
                    shared,
                    Data::Catalog(CatalogDelivery {
                        shared: Arc::downgrade(shared),
                        computer: computer.into(),
                        epoch: epoch.into(),
                        source,
                        stopped: stop.clone(),
                        value: CatalogValue::Row(Change::Upsert {
                            computer: ComputerId::from(computer),
                            key: item.key,
                            row,
                        }),
                    }),
                    &stop,
                )
                .await?;
            }
            if cursor.is_none() {
                let (done, joined) = oneshot::channel();
                emit_data(
                    shared,
                    Data::Catalog(CatalogDelivery {
                        shared: Arc::downgrade(shared),
                        computer: computer.into(),
                        epoch: epoch.into(),
                        source,
                        stopped: stop.clone(),
                        value: CatalogValue::Finish {
                            version,
                            complete: page.complete,
                            keys: std::mem::take(&mut keys),
                            done,
                        },
                    }),
                    &stop,
                )
                .await?;
                // Only one retained membership set and one serial walk at a time.
                joined.await.map_err(|_| "catalog_invalidated")?;
                return Ok(());
            }
        }
    }
    .await;
    if let Some(cursor) = cursor {
        let _ = request(shared, json!({"op":"catalog_close","request_id":uuid::Uuid::new_v4().to_string(),"computer":computer,"cursor":cursor}), stop).await;
    }
    result
}

#[cfg(all(test, unix))]
mod tests;
