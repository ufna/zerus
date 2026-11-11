//! One thread that runs the Teams read model: it runs each due read off the UI
//! thread, one at a time, and hands every update to the client as a
//! [`crate::Change::Teams`]. Following a team, a refresh after an action, a
//! changed tasks topic and the tab's visibility wake it. Dropping the poller
//! stops it after the read in flight.
use super::model::{self, Sent, TeamsFollow, TeamsModel, TeamsUpdate};
use super::read::{self, TeamError, TeamRead};
use crate::{Change, ComputerId};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Instant;

/// Runs one read: `read::read` against `hgs`, or a fixture in tests.
pub type Runner = Box<dyn Fn(&TeamRead) -> Result<Value, TeamError> + Send + Sync>;
/// Receives each change (behind a lock, from the poller thread).
pub type Emit = Box<dyn Fn(Change) + Send>;
type SharedEmit = Arc<Mutex<Emit>>;

#[derive(Default)]
struct State {
    model: TeamsModel,
    /// The ends of a follow change, sent by the thread.
    outbox: Vec<(String, TeamsUpdate)>,
    stop: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The Teams reads of this computer.
pub struct TeamsPoller {
    shared: Arc<Shared>,
}

impl TeamsPoller {
    /// Polls this computer's teams through `hgs`.
    pub fn spawn(hgs: PathBuf, emit: impl Fn(Change) + Send + 'static) -> std::io::Result<Self> {
        Self::spawn_with(
            Box::new(move |read: &TeamRead| read::read(&hgs, read)),
            Box::new(emit),
        )
    }

    /// Polls with `runner` instead of `hgs`.
    pub fn spawn_with(runner: Runner, emit: Emit) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let computer = ComputerId::local();
        let runner = Arc::new(runner);
        let emit: SharedEmit = Arc::new(Mutex::new(emit));
        thread::Builder::new()
            .name("teams-poll".into())
            .spawn(move || run(&thread_shared, &computer, &runner, &emit))?;
        Ok(Self { shared })
    }

    /// Follows `follow` from now on; following nothing ends every view.
    pub fn follow(&self, follow: TeamsFollow) {
        let mut state = self.shared.lock();
        let ended = state.model.follow(follow, Instant::now());
        state.outbox.extend(ended);
        drop(state);
        self.shared.wake.notify_all();
    }

    /// The current follow.
    pub fn following(&self) -> TeamsFollow {
        self.shared.lock().model.following().clone()
    }

    /// Reads every followed view again now (after an action).
    pub fn refresh(&self) {
        self.shared.lock().model.refresh(Instant::now());
        self.shared.wake.notify_all();
    }

    /// The project's tasks topic changed: the watched team is read again.
    pub fn tasks_changed(&self) {
        self.shared.lock().model.tasks_changed(Instant::now());
        self.shared.wake.notify_all();
    }

    /// The Team tab was hidden (`true`) or shown again. A hidden tab reads
    /// nothing; shown again, every followed view is read at once.
    pub fn hidden(&self, hidden: bool) {
        self.shared.lock().model.set_hidden(hidden, Instant::now());
        self.shared.wake.notify_all();
    }
}

impl Drop for TeamsPoller {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
    }
}

fn emit_to(emit: &SharedEmit, change: Change) {
    (emit.lock().unwrap_or_else(PoisonError::into_inner))(change);
}

fn run(shared: &Arc<Shared>, computer: &ComputerId, runner: &Arc<Runner>, emit: &SharedEmit) {
    loop {
        let (read, outbox) = {
            let mut state = shared.lock();
            loop {
                // The ends of a last follow go out before a stop.
                if !state.outbox.is_empty() {
                    break (None, std::mem::take(&mut state.outbox));
                }
                if state.stop {
                    return;
                }
                let now = Instant::now();
                if let Some(read) = state.model.next(now) {
                    break (Some(read), Vec::new());
                }
                let wait = state.model.wait(now);
                state = shared
                    .wake
                    .wait_timeout(state, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        };
        for (key, update) in outbox {
            emit_to(emit, model::change(computer, Sent::new(key, update)));
        }
        let Some(read) = read else { continue };
        let reply = runner(&read);
        let updates = {
            let mut state = shared.lock();
            if state.stop {
                return;
            }
            state.model.apply(&read, reply, Instant::now())
        };
        shared.wake.notify_all();
        for (key, update) in updates {
            emit_to(emit, model::change(computer, Sent::new(key, update)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/team-plan/fixtures/team-queue.json"
        )))
        .expect("the shared team fixture parses")
    }

    fn change_key(change: &Change) -> Option<&str> {
        match change {
            Change::Teams { key, .. } => Some(key),
            _ => None,
        }
    }

    #[test]
    fn a_followed_view_arrives_as_a_batch_change() {
        let (sender, receiver) = channel();
        let poller = TeamsPoller::spawn_with(
            Box::new(|_read| Ok(fixture())),
            Box::new(move |change| {
                let _ = sender.send(change);
            }),
        )
        .expect("the poller starts");
        poller.follow(TeamsFollow {
            list: true,
            waiting: false,
            team: None,
        });
        let change = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("a followed read gives a change");
        assert_eq!(change_key(&change), Some("list"));
        let Change::Teams {
            update, payload, ..
        } = &change
        else {
            panic!("not a teams change");
        };
        assert_eq!(update.kind(), "list");
        assert_eq!(payload["teams"][0]["name"], "Fixes");
        drop(poller);
    }

    #[test]
    fn a_hidden_tab_reads_nothing() {
        let reads = Arc::new(Mutex::new(0usize));
        let counted = Arc::clone(&reads);
        let (sender, receiver) = channel();
        let poller = TeamsPoller::spawn_with(
            Box::new(move |_read| {
                *counted.lock().unwrap() += 1;
                Ok(fixture())
            }),
            Box::new(move |change| {
                let _ = sender.send(change);
            }),
        )
        .expect("the poller starts");
        poller.follow(TeamsFollow {
            list: true,
            waiting: false,
            team: None,
        });
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the list is read");
        assert_eq!(*reads.lock().unwrap(), 1);
        // While the tab is hidden nothing is read, even after a refresh.
        poller.hidden(true);
        poller.refresh();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(*reads.lock().unwrap(), 1, "a hidden tab reads nothing");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(50)),
            Err(RecvTimeoutError::Timeout)
        ));
        // Shown again, a refresh reads the view at once.
        poller.hidden(false);
        poller.refresh();
        let mut read_again = false;
        for _ in 0..40 {
            if *reads.lock().unwrap() > 1 {
                read_again = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(read_again, "a shown tab reads the view again");
        drop(poller);
    }
}
