//! One bounded background reader/executor. The service thread never opens a
//! Teams database, waits for a launch or reads an account. Shutdown keeps its
//! original owner until this worker and its original children physically end.
use super::{launch, native::Native};
use crate::{
    config::Config,
    desktop::{creation_clone::lease::Machine, feed, fleet},
    platform,
    teams::store::Store,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zerus_watch_protocol::Body;

pub(in crate::desktop) struct Worker {
    config: Config,
    epoch: String,
    owner: Arc<platform::FileLock>,
    machine: Option<Machine>,
    work: Option<Work>,
    stopping: bool,
    next: Instant,
}
struct Work {
    thread: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
}
impl Worker {
    pub fn new(config: &Config, epoch: &str, owner: Arc<platform::FileLock>) -> Self {
        Self {
            config: config.clone(),
            epoch: epoch.into(),
            owner,
            machine: None,
            work: None,
            stopping: false,
            next: Instant::now(),
        }
    }
    pub fn observe(&mut self, event: &fleet::FleetEvent) {
        let old = self.machine.clone();
        Machine::observe(&mut self.machine, event);
        if old != self.machine {
            self.withdraw();
            self.next = Instant::now();
        }
        if let feed::Packet::Watch {
            computer, frame, ..
        } = &event.packet
        {
            if computer.is_empty()
                && matches!(
                    frame.body,
                    Body::Hello(_) | Body::Snapshot(_) | Body::Delta(_)
                )
            {
                self.wake();
            }
        }
    }
    pub fn reject(&mut self, event: &fleet::FleetEvent) {
        let old = self.machine.clone();
        Machine::reject(&mut self.machine, event);
        if old != self.machine {
            self.withdraw();
            self.next = Instant::now();
        }
    }
    /// Future team-store revision intake calls this same bounded/coalesced
    /// wakeup. It grants no admission of a stored original.
    pub fn wake(&self) {
        if let Some(work) = &self.work {
            let _ = work.wake.try_send(());
        }
    }
    fn withdraw(&self) {
        if let Some(work) = &self.work {
            work.stop.store(true, Ordering::Release);
            let _ = work.wake.try_send(());
        }
    }
    pub fn poll(&mut self) {
        if self.work.as_ref().is_some_and(|w| w.thread.is_finished()) {
            let _ = self.work.take().expect("finished worker").thread.join();
        }
        if self.stopping || self.work.is_some() || Instant::now() < self.next {
            return;
        }
        self.next = Instant::now() + Duration::from_secs(60);
        if !crate::teams::enabled(&self.config.state) {
            return;
        }
        let Some(machine) = self.machine.as_ref().map(|m| m.id.clone()) else {
            return;
        };
        let (config, epoch, owner) = (self.config.clone(), self.epoch.clone(), self.owner.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (wake, receive) = mpsc::sync_channel(1);
        if let Ok(thread) = thread::Builder::new()
            .name("teams-member-launch".into())
            .spawn(move || {
                let _owner = owner;
                run(config, epoch, machine, stopped, receive);
            })
        {
            self.work = Some(Work { thread, stop, wake });
        }
    }
    pub fn request_stop(&mut self) {
        self.stopping = true;
        self.withdraw();
    }
    pub fn drained(&mut self) -> bool {
        self.poll();
        self.work.is_none()
    }
}
fn run(
    config: Config,
    epoch: String,
    machine: String,
    stop: Arc<AtomicBool>,
    receive: mpsc::Receiver<()>,
) {
    let Ok(mut actor) = launch::Runtime::open(&config.state, &machine, &epoch) else {
        return;
    };
    let Ok(mut store) = Store::open(&config.state) else {
        return;
    };
    let mut native = Native {
        config: config.clone(),
        machine,
        stop: stop.clone(),
    };
    let mut after = String::new();
    while !stop.load(Ordering::Acquire) && crate::teams::enabled(&config.state) {
        if let Ok(ids) = page(&mut store, &after) {
            for id in &ids {
                for _ in 0..128 {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let now = chrono::Utc::now().timestamp_millis();
                    if !actor
                        .tick(&mut store, &mut native, id, now)
                        .unwrap_or(false)
                    {
                        break;
                    }
                }
                // A bounded turn may end mid-admission on a large roster.
                // Keep that team first on the next pass: another team's
                // admission must never displace its fixed live original.
                if actor.busy() {
                    break;
                }
                after = id.clone();
            }
            if !actor.busy() && ids.len() < 16 {
                after.clear();
            }
        }
        if receive.recv_timeout(Duration::from_secs(60))
            == Err(mpsc::RecvTimeoutError::Disconnected)
        {
            break;
        }
    }
}
fn page(store: &mut Store, after: &str) -> super::ports::Result<Vec<String>> {
    store.refresh()?;
    let Some(db) = store.db.as_ref() else {
        return Ok(vec![]);
    };
    let mut query = db
        .prepare("SELECT id FROM teams WHERE id>?1 ORDER BY id LIMIT 16")
        .map_err(crate::teams::store::storage)?;
    let rows = query
        .query_map([after], |row| row.get(0))
        .map_err(crate::teams::store::storage)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(crate::teams::store::storage);
    rows
}
