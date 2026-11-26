//! Read one retained original's receipt on its authenticated desktop client
//! thread. This lane never invokes a lifecycle action or opens a checkout.
use super::{load, valid_id, Intent, Status, Store};
use crate::{
    config::Config,
    desktop::{frame, protocol, store::Location, transport, Event, Hello, ABI, TIMEOUT},
    platform::{self, FileIdentity, FileLock},
    state::{self, LifecycleReceiptEvidence, LifecycleReceiptRoot},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use zerus_client_core::{Exit, Invocation};

const OP: &str = "termination_outcome_observe";
const UNAVAILABLE: &str = "termination_observation_unavailable";
const READ_TIME: Duration = Duration::from_millis(1600);
const PEER_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    op: String,
    request_id: String,
    configuration: String,
    service_epoch: String,
    original: Status,
}

pub(in crate::desktop) struct Reader {
    config: Config,
    root: Option<LifecycleReceiptRoot>,
    owner_path: PathBuf,
    epoch: String,
    configuration: String,
    stopped: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    peer_image: Option<PeerImage>,
}

impl Reader {
    pub fn new(
        config: &Config,
        location: &Location,
        epoch: &str,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config: config.clone(),
            root: location
                .root
                .parent()
                .and_then(|path| LifecycleReceiptRoot::open(path).ok()),
            owner_path: location.root.join("owner.lock"),
            epoch: epoch.into(),
            configuration: location.configuration.clone(),
            stopped,
            busy: Arc::new(AtomicBool::new(false)),
            peer_image: PeerImage::current(),
        }
    }

    pub fn pending(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn admit(
        self: &Arc<Self>,
        store: &Store,
        hello: Hello,
        owner: Arc<FileLock>,
        sequence: u64,
        until: Instant,
    ) -> Reply {
        let job = if hello.abi != ABI || hello.configuration != self.configuration {
            Job::Error(json!({"code":"incompatible_configuration"}))
        } else {
            let value = hello.request.unwrap_or(Value::Null);
            self.prepare(store, &value, owner, until).map_or_else(
                |_| Job::Value(refused(&value)),
                |work| Job::Read(Box::new(work)),
            )
        };
        Reply {
            epoch: self.epoch.clone(),
            sequence,
            job,
        }
    }

    fn prepare(
        self: &Arc<Self>,
        store: &Store,
        value: &Value,
        owner: Arc<FileLock>,
        until: Instant,
    ) -> Result<Work> {
        let query: Query = serde_json::from_value(value.clone())?;
        ensure!(
            query.op == OP
                && query.configuration == self.configuration
                && query.service_epoch == self.epoch
                && valid_id(&query.request_id)
                && valid_id(&query.original.original_request_id)
                && query.request_id != query.original.original_request_id
                && !self.pending()
                && !self.stopped.load(Ordering::SeqCst)
                && Instant::now() < until,
            UNAVAILABLE
        );
        let retained = load(
            &store.db,
            &query.original.original_request_id,
            &self.configuration,
        )?
        .context(UNAVAILABLE)?;
        // Status::matches ignores the serving epoch for an idempotent retain.
        // An observation must prove the original FIRST epoch as well.
        ensure!(retained.status == query.original, UNAVAILABLE);
        self.busy.store(true, Ordering::SeqCst);
        Ok(Work {
            query,
            intent: retained.intent,
            until,
            evidence: None,
            lease: Arc::new(Lease {
                reader: self.clone(),
                owner,
                _permit: Permit(self.busy.clone()),
            }),
        })
    }
}

// Drops last, after the physical worker's reader and serving lock copies.
struct Permit(Arc<AtomicBool>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
struct Lease {
    reader: Arc<Reader>,
    owner: Arc<FileLock>,
    _permit: Permit,
}

struct Work {
    query: Query,
    intent: Intent,
    until: Instant,
    evidence: Option<LifecycleReceiptEvidence>,
    lease: Arc<Lease>,
}

impl Work {
    fn current(&self) -> bool {
        let reader = &self.lease.reader;
        !reader.stopped.load(Ordering::SeqCst)
            && Instant::now() < self.until
            && platform::verify_private(&reader.owner_path, platform::Private::File)
                .is_ok_and(|private| private)
            && self
                .lease
                .owner
                .identity()
                .is_ok_and(|owner| platform::file_identity(&reader.owner_path) == Some(owner))
    }

    fn read(&mut self) -> Result<Receipt> {
        ensure!(self.current(), UNAVAILABLE);
        let reader = &self.lease.reader;
        let capture = &self.intent.capture;
        let receipt = if capture.target.computer.is_empty() {
            let machine = state::lifecycle_receipt_machine(&reader.config)
                .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
            ensure!(machine == capture.machine, UNAVAILABLE);
            let (receipt, evidence) = reader
                .root
                .as_ref()
                .context(UNAVAILABLE)?
                .observe(&capture.target.name, self.intent.original.as_bytes())
                .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
            self.evidence = evidence;
            serde_json::from_value(receipt)?
        } else {
            self.peer()?
        };
        ensure!(self.current(), UNAVAILABLE);
        Ok(receipt)
    }

    fn peer(&self) -> Result<Receipt> {
        let reader = &self.lease.reader;
        let image = reader.peer_image.as_ref().context(UNAVAILABLE)?;
        let capture = &self.intent.capture;
        let invocation = Invocation {
            args: vec![
                format!("@{}", capture.target.computer),
                "--expect-machine".into(),
                capture.machine.clone(),
                "session-action-status".into(),
                capture.target.name.clone(),
                "--json".into(),
            ],
            stdin: Some(self.intent.original.as_bytes().to_vec()),
            timeout: self.until.saturating_duration_since(Instant::now()),
            request_id: None,
        };
        let config = reader.config.dir.to_str().context(UNAVAILABLE)?;
        let state = reader.config.state.to_str().context(UNAVAILABLE)?;
        ensure!(self.current() && image.unchanged(), UNAVAILABLE);
        let result = zerus_client_core::run::run_accounted_group(
            &image.path,
            &invocation,
            &[
                ("HGS_CONFIG_DIR", config),
                ("HGS_STATE_DIR", state),
                ("HOME", &reader.config.home),
            ],
            PEER_BYTES,
            &reader.stopped,
            self.lease.clone(),
        );
        let Ok(Exit::Code {
            code: 0, stdout, ..
        }) = result
        else {
            anyhow::bail!(UNAVAILABLE);
        };
        ensure!(image.unchanged(), UNAVAILABLE);
        Ok(serde_json::from_slice(&stdout)?)
    }

    fn response(&mut self) -> Value {
        let (status, archive) = match self.read().and_then(|receipt| receipt.check(&self.intent)) {
            Ok(outcome) => outcome,
            Err(_) => return refused_id(&self.query.request_id),
        };
        let mut result = json!({"op":OP,"request_id":self.query.request_id,
            "original_request_id":self.intent.request_id,"accepted":true,
            "status":status,"read_only":true,"cleanup_authorized":false});
        if let Some(archive) = archive {
            result["archive_id"] = json!(archive);
        }
        result
    }
}

/// The existing own-CLI helper, bound to the serving image on Linux and
/// checked against the selected executable on other desktops.
struct PeerImage {
    path: PathBuf,
    identity: FileIdentity,
}
impl PeerImage {
    fn current() -> Option<Self> {
        crate::self_exec::ensure().ok()?;
        let path = if cfg!(target_os = "linux") {
            Path::new("/proc/self/exe").to_path_buf()
        } else {
            std::env::current_exe().ok()?
        };
        let identity = platform::file_identity(&path)?;
        #[cfg(target_os = "macos")]
        if platform::image_identity() != Some(identity) {
            return None;
        }
        Some(Self { path, identity })
    }
    fn unchanged(&self) -> bool {
        platform::file_identity(&self.path) == Some(self.identity)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Unavailable,
    Uncertain,
    Failed,
    Completed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    request_id: String,
    name: String,
    run_id: String,
    conversation_id: String,
    status: Outcome,
    at: Option<f64>,
    result_target: Option<Archive>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    name: String,
    run_id: String,
    conversation_id: String,
    archive_id: String,
}
impl Receipt {
    fn check(self, intent: &Intent) -> Result<(Outcome, Option<String>)> {
        let target = &intent.capture.target;
        ensure!(
            self.request_id == intent.request_id
                && self.name == target.name
                && self.run_id == target.run_id
                && self.conversation_id == target.conversation_id
                && if self.status == Outcome::Unavailable {
                    self.at.is_none()
                } else {
                    self.at.is_some_and(|at| at.is_finite() && at >= 0.0)
                },
            UNAVAILABLE
        );
        let archive = match (self.status, self.result_target) {
            (Outcome::Completed, Some(archive)) => {
                ensure!(
                    archive.name == target.name
                        && archive.run_id == target.run_id
                        && archive.conversation_id == target.conversation_id
                        && valid_id(&archive.archive_id),
                    UNAVAILABLE
                );
                Some(archive.archive_id)
            }
            (Outcome::Completed, None) | (_, Some(_)) => anyhow::bail!(UNAVAILABLE),
            (_, None) => None,
        };
        Ok((self.status, archive))
    }
}

enum Job {
    Error(Value),
    Value(Value),
    Read(Box<Work>),
}
pub(in crate::desktop) struct Reply {
    epoch: String,
    sequence: u64,
    job: Job,
}
impl Reply {
    fn frame(&mut self) -> Value {
        let (kind, payload) = match &mut self.job {
            Job::Error(value) => ("error", value.clone()),
            Job::Value(value) => ("response", value.clone()),
            Job::Read(work) => ("response", work.response()),
        };
        frame(&self.epoch, self.sequence, kind, payload)
    }
}

fn refused(value: &Value) -> Value {
    refused_id(
        value["request_id"]
            .as_str()
            .filter(|id| valid_id(id))
            .unwrap_or(""),
    )
}
fn refused_id(id: &str) -> Value {
    json!({"op":OP,"request_id":id,"accepted":false,"read_only":true,
        "cleanup_authorized":false,"code":UNAVAILABLE})
}

pub(in crate::desktop) fn recognizes(hello: &Hello) -> bool {
    hello.op == "request"
        && hello
            .request
            .as_ref()
            .is_some_and(|request| request["op"] == OP)
}

struct Session {
    // Shut down and drop the original socket before dropping the work lease.
    stream: transport::Stream,
    reply: Option<Reply>,
}
impl Drop for Session {
    fn drop(&mut self) {
        transport::shutdown(&self.stream);
    }
}

pub(in crate::desktop) fn client(
    stream: transport::Stream,
    hello: Hello,
    send: mpsc::SyncSender<Event>,
) -> Result<()> {
    let mut session = Session {
        stream,
        reply: None,
    };
    let until = Instant::now() + READ_TIME;
    let (reply, receive) = mpsc::sync_channel(1);
    send.try_send(Event::TerminationObservation(hello, reply, until))
        .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
    session.reply = Some(receive.recv_timeout(TIMEOUT).context(UNAVAILABLE)?);
    let reply = protocol::encode(
        &session
            .reply
            .as_mut()
            .expect("observation admission")
            .frame(),
    )?;
    session.stream.write_all(&reply)?;
    session.stream.flush()?;
    Ok(())
}
