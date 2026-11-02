//! One original off-actor Freedesktop worker. The actor supplies the private
//! Endpoint and Session; no request, ambient bus discovery or UI process does.
use super::{wire, Action, Activation, Command, Endpoint, Event, Outcome, Session};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use wire::{NativeId, Platform, PlatformContext, Refusal, Uncertainty};
use zerus_freedesktop_notify::Signal;

#[path = "../../../../crates/freedesktop-notify/src/ingress.rs"]
mod ingress;
mod worker;

const WAIT: Duration = Duration::from_secs(5);
const CALLBACKS: usize = 32;
enum Control {
    Restore(Vec<Activation>, mpsc::SyncSender<bool>),
    Finish(mpsc::SyncSender<bool>),
}
struct Shared {
    stop: AtomicBool,
    ready: AtomicBool,
    failed: AtomicBool,
    born: Instant,
    cleanup: AtomicU64,
}
impl Shared {
    fn cleanup_deadline(&self) -> Instant {
        let wanted = (self.born.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64)
            .saturating_add(WAIT.as_nanos() as u64)
            .max(1);
        let after =
            match self
                .cleanup
                .compare_exchange(0, wanted, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => wanted,
                Err(original) => original,
            };
        self.born + Duration::from_nanos(after)
    }
}
pub(super) struct Acknowledgement(mpsc::Receiver<bool>);
impl Acknowledgement {
    pub fn poll(&self) -> Option<bool> {
        match self.0.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(false),
        }
    }
}

/// Shared connection/worker lifetime is separate from per-command resources.
/// Only a successful original cleanup AND actual thread join reports drained.
pub(super) struct Runtime {
    shared: Arc<Shared>,
    control: mpsc::SyncSender<Control>,
    thread: Option<JoinHandle<bool>>,
    joined: Option<bool>,
}
impl Runtime {
    pub fn start(session: Session, endpoint: Endpoint, address: String) -> Result<Self> {
        ensure!(
            session.valid() && session.platform == Platform::Linux,
            "invalid Linux notification session"
        );
        ensure!(
            endpoint.platform == Platform::Linux,
            "wrong notification endpoint"
        );
        ensure!(
            address.len() <= ingress::AUTH && address.capacity() <= ingress::AUTH,
            "notification bus address is too large"
        );
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            born: Instant::now(),
            cleanup: AtomicU64::new(0),
        });
        let (control, receive) = mpsc::sync_channel(1);
        let scope = shared.clone();
        let setup = shared.born + WAIT;
        let thread = std::thread::Builder::new()
            .name("hgs-linux-notify".into())
            .spawn(move || worker::run(session, endpoint, address, receive, scope, setup))?;
        Ok(Self {
            shared,
            control,
            thread: Some(thread),
            joined: None,
        })
    }
    /// Read-only pages from the actor's original indexed restore scan. No
    /// notification API is called and no record is evicted to admit a page.
    pub fn restore(&self, values: Vec<Activation>) -> Option<Acknowledgement> {
        if values.len() > 16
            || values.capacity() > 16
            || values
                .iter()
                .any(|v| !v.valid_for(Platform::Linux) || v.platform_context.is_some())
        {
            return None;
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.control
            .try_send(Control::Restore(values, reply))
            .ok()?;
        Some(Acknowledgement(result))
    }
    /// The actor waits for every restore acknowledgement before this gate.
    /// Ready remains display data; only original Endpoint work can invoke APIs.
    pub fn finish_restoration(&self) -> Option<Acknowledgement> {
        let (reply, result) = mpsc::sync_channel(1);
        self.control.try_send(Control::Finish(reply)).ok()?;
        Some(Acknowledgement(result))
    }
    pub fn request_stop(&self) {
        self.shared.cleanup_deadline();
        self.shared.ready.store(false, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
    }
    pub fn drained(&mut self) -> bool {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            self.joined = Some(self.thread.take().unwrap().join().unwrap_or(false));
            if self.joined == Some(false) {
                self.shared.failed.store(true, Ordering::Release);
            }
        }
        self.joined == Some(true)
    }
    pub fn status(&self) -> Value {
        json!({"platform":"linux","ready":self.shared.ready.load(Ordering::Acquire),
            "stopping":self.shared.stop.load(Ordering::Acquire),
            "error":self.shared.failed.load(Ordering::Acquire),
            "joined":self.joined.is_some(),"drained":self.joined==Some(true)})
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // Abandonment grants no join/clean receipt. The original worker keeps
        // its endpoint/connection/callback resources until its actual return.
        self.request_stop();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Namespace {
    bus_id: String,
    owner: String,
}
struct Mappings {
    namespace: Namespace,
    rows: BTreeMap<u32, Activation>,
    tokens: BTreeMap<u32, String>,
}
impl Mappings {
    fn new(namespace: Namespace) -> Self {
        Self {
            namespace,
            rows: BTreeMap::new(),
            tokens: BTreeMap::new(),
        }
    }
    fn id(&self, value: &NativeId) -> Option<u32> {
        match value {
            NativeId::Linux { bus_id, owner, id }
                if *bus_id == self.namespace.bus_id
                    && *owner == self.namespace.owner
                    && *id != 0 =>
            {
                Some(*id)
            }
            _ => None,
        }
    }
    fn restore(&mut self, values: Vec<Activation>) -> bool {
        if values.iter().any(|v| {
            !v.valid_for(Platform::Linux)
                || v.platform_context.is_some()
                || self.id(&v.native_id).is_none()
        }) {
            return false;
        }
        let mut page = BTreeMap::new();
        for value in values {
            let id = self.id(&value.native_id).unwrap();
            if self.rows.get(&id).is_some_and(|old| old != &value)
                || page.get(&id).is_some_and(|old| old != &value)
            {
                return false;
            }
            page.insert(id, value);
        }
        let extra = page.keys().filter(|id| !self.rows.contains_key(id)).count();
        if self.rows.len() + extra > wire::MAPPINGS {
            return false;
        }
        self.rows.extend(page);
        true
    }
    fn adopt(&mut self, command: &Command, id: u32) -> Outcome {
        if id == 0 || self.rows.contains_key(&id) || self.rows.len() == wire::MAPPINGS {
            return Outcome::Unknown {
                code: Uncertainty::NativeError,
            };
        }
        let native_id = NativeId::Linux {
            bus_id: self.namespace.bus_id.clone(),
            owner: self.namespace.owner.clone(),
            id,
        };
        self.rows.insert(
            id,
            Activation {
                request_id: command.binding.request_id.clone(),
                token: command.action.token().to_owned(),
                native_id: native_id.clone(),
                platform_context: None,
            },
        );
        Outcome::Delivered { native_id }
    }
    fn owns(&self, token: &str, native_id: &NativeId) -> bool {
        self.id(native_id)
            .and_then(|id| self.rows.get(&id))
            .is_some_and(|v| v.token == token)
    }
    fn signal(&mut self, signal: Signal, endpoint: &Endpoint) -> bool {
        match signal {
            Signal::ActivationToken { id, token } => {
                if !self.rows.contains_key(&id) {
                    return true;
                }
                if token.is_empty()
                    || token.len() > wire::ACTIVATION_TOKEN_BYTES
                    || token.chars().any(char::is_control)
                {
                    return false;
                }
                if !self.tokens.contains_key(&id) && self.tokens.len() == CALLBACKS {
                    return false;
                }
                self.tokens.insert(id, token);
                true
            }
            Signal::ActionInvoked { id, action } => {
                let context = self.tokens.remove(&id);
                if action != "default" && action != "open-session" {
                    return true;
                }
                let Some(value) = self.rows.get(&id) else {
                    return true;
                };
                let mut value = value.clone();
                value.platform_context =
                    context.map(|activation_token| PlatformContext::Wayland { activation_token });
                endpoint.emit(Event::Clicked(value))
            }
            Signal::NotificationClosed { id, .. } => {
                self.tokens.remove(&id);
                self.rows
                    .remove(&id)
                    .is_none_or(|value| endpoint.emit(Event::Closed(value)))
            }
        }
    }
}

#[cfg(test)]
mod tests;
