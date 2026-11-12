//! Test-only pause of a worker thread at its run entry, before any launch,
//! store or native effect. A hold is keyed by its state folder, so tests that
//! run concurrently never pause each other's workers. Release builds compile
//! none of this.
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    time::Duration,
};

struct Slot {
    state: PathBuf,
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}

static SLOTS: Mutex<Vec<Slot>> = Mutex::new(Vec::new());

/// The test's side of one pause. Dropping it releases the paused thread, or
/// withdraws the slot when no thread reached it, also while unwinding.
pub(in crate::desktop) struct Hold {
    state: PathBuf,
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::SyncSender<()>>,
}

impl Hold {
    /// Pauses the next worker thread that enters its run for `state`.
    pub fn install(state: &Path) -> Self {
        let (entered, reached) = mpsc::sync_channel(1);
        let (release, receive) = mpsc::sync_channel(1);
        let mut slots = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            slots.iter().all(|slot| slot.state != state),
            "one hold per state folder"
        );
        slots.push(Slot {
            state: state.to_owned(),
            entered,
            release: receive,
        });
        Self {
            state: state.to_owned(),
            entered: reached,
            release: Some(release),
        }
    }
    /// Whether the worker thread reached its run entry within `timeout`.
    pub fn entered(&self, timeout: Duration) -> bool {
        self.entered.recv_timeout(timeout).is_ok()
    }
    /// Lets the paused thread return without running.
    pub fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.try_send(());
        }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        SLOTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|slot| slot.state != self.state);
        self.release();
    }
}

/// Called first in the worker thread's run. Returns true after a paused run
/// was released; the run then returns without any effect.
pub(super) fn pause(state: &Path) -> bool {
    let slot = {
        let mut slots = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        let index = slots.iter().position(|slot| slot.state == state);
        index.map(|index| slots.swap_remove(index))
    };
    let Some(slot) = slot else {
        return false;
    };
    let _ = slot.entered.try_send(());
    // A release, or the hold dropped by an unwinding test, ends the pause.
    let _ = slot.release.recv();
    true
}
