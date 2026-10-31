//! Optional lifetime accounting, not a native lease or full role barrier.
//! The host supplies one NativePosting charge and retains the real platform
//! connection/delegate resources separately. No worker or OS call lives here.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};
use zerus_client_core::desktop::drain::{Charge, Role};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    WrongRole,
    Closed,
    Unavailable,
}

/// Local diagnostics only, never a service drain or execution permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub closed: bool,
    pub pinned: bool,
}

pub(super) struct Resources {
    parent: Mutex<Option<Charge>>,
    pinned: AtomicBool,
}

/// Weak, so retaining a finished control never retains the host charge.
#[derive(Clone)]
pub struct Control(Weak<Resources>);

impl Resources {
    pub fn new(charge: Charge) -> Result<(Arc<Self>, Control), Refused> {
        if charge.role() != Role::NativePosting {
            return Err(Refused::WrongRole);
        }
        let resources = Arc::new(Self {
            parent: Mutex::new(Some(charge)),
            pinned: AtomicBool::new(false),
        });
        let control = Control(Arc::downgrade(&resources));
        Ok((resources, control))
    }

    /// Admission and close share one lock. Copies retain the same host group;
    /// they add no per-token collection or independent resource admission.
    pub fn start(self: &Arc<Self>) -> Result<Completion, Refused> {
        let parent = self.parent.lock().map_err(|_| Refused::Unavailable)?;
        let charge = parent.as_ref().ok_or(Refused::Closed)?.clone();
        Ok(Completion {
            resources: self.clone(),
            charge: Some(charge),
        })
    }

    fn pin(&self, charge: Charge) {
        if !self.pinned.swap(true, Ordering::AcqRel) {
            // A dropped completion/work item is not proof of completion.
            // Retain exactly one copy for this fixed scope until process exit,
            // even if its caller drops every control/notifier handle. Further
            // unknown drops add no retained allocation. Nothing is retried.
            std::mem::forget(charge);
            // This can run during unwinding. A broken diagnostic pipe must
            // not panic a second time from Drop.
            let _ = std::io::Write::write_all(
                &mut std::io::stderr().lock(),
                b"zerus-client: role=native_posting slot=0 completion unverified; resources pinned\n",
            );
        }
    }
}

#[allow(dead_code)] // No production host control caller in this prerequisite.
impl Control {
    /// Close-only and nonblocking apart from the short local lock. It stops
    /// future steps, not already admitted Center calls or completion blocks.
    /// The host must wait on its own closed Gate, off the GUI thread.
    pub fn close(&self) -> Result<Snapshot, Refused> {
        let resources = self.0.upgrade().ok_or(Refused::Unavailable)?;
        let parent = resources
            .parent
            .lock()
            .map_err(|_| Refused::Unavailable)?
            .take();
        drop(parent);
        self.snapshot()
    }

    pub fn snapshot(&self) -> Result<Snapshot, Refused> {
        let resources = self.0.upgrade().ok_or(Refused::Unavailable)?;
        let closed = resources
            .parent
            .lock()
            .map_err(|_| Refused::Unavailable)?
            .is_none();
        Ok(Snapshot {
            closed,
            pinned: resources.pinned.load(Ordering::Acquire),
        })
    }
}

/// Armed until actual work returns normally, not merely until its callback is
/// dropped. The same obligation moves from a native completion into queued
/// Hop work. A panic or an unexecuted drop conservatively pins the fixed scope.
pub(super) struct Completion {
    resources: Arc<Resources>,
    charge: Option<Charge>,
}
impl Completion {
    pub fn fork(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            charge: self.charge.clone(),
        }
    }

    /// Plain retention through the scheduling callback's own return, even
    /// when Hop executes its work synchronously.
    pub fn hold(&self) -> Option<Charge> {
        self.charge.clone()
    }

    pub fn complete(mut self) {
        drop(self.charge.take());
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(charge) = self.charge.take() {
            self.resources.pin(charge);
        }
    }
}
