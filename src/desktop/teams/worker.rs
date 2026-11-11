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
    let mut store = match Store::open(&config.state) {
        Ok(store) => store,
        Err(error) => {
            if let Err(fault) = actor.record_health(&error, chrono::Utc::now().timestamp_millis()) {
                eprintln!("hgs: Teams health write failed: {}", fault.code);
            }
            return;
        }
    };
    let mut native = Native {
        config: config.clone(),
        machine,
        stop: stop.clone(),
    };
    super::child::prune_custody(&config);
    let mut cursor = None;
    let mut cycle_fault = false;
    let mut confirm_cursor = Default::default();
    while !stop.load(Ordering::Acquire) && crate::teams::enabled(&config.state) {
        if let Err(error) = pass(
            &mut actor,
            &mut store,
            &mut native,
            &mut cursor,
            &mut cycle_fault,
            &stop,
        ) {
            eprintln!("hgs: Teams worker fault: {}", error.code);
        }
        if let Err(error) = super::confirm::step(
            &config,
            &epoch,
            &native.machine,
            &mut store,
            &stop,
            &mut confirm_cursor,
        ) {
            eprintln!("hgs: Teams standing confirmation fault: {error}");
        }
        if receive.recv_timeout(Duration::from_secs(60))
            == Err(mpsc::RecvTimeoutError::Disconnected)
        {
            break;
        }
    }
}
pub(super) fn pass(
    actor: &mut launch::Runtime,
    store: &mut Store,
    effects: &mut impl super::ports::Effects,
    cursor: &mut Option<crate::teams::store::Cursor>,
    cycle_fault: &mut bool,
    stop: &AtomicBool,
) -> super::ports::Result<()> {
    actor.begin_pass();
    let page = match page(store, cursor.as_ref()) {
        Ok(page) => page,
        Err(error) => {
            record_fault(actor, cycle_fault, &error)?;
            return Ok(());
        }
    };
    for team in &page.rows {
        for step in 0..12 {
            if step >= 8 && !actor.busy() {
                break;
            }
            if stop.load(Ordering::Acquire) {
                actor.release();
                return Ok(());
            }
            let advanced = match actor.tick(
                store,
                effects,
                &team.id,
                chrono::Utc::now().timestamp_millis(),
            ) {
                Ok(advanced) => advanced,
                Err(error) => {
                    if super::ledger::is_notice_budget(&error) {
                        actor.record_health(&error, chrono::Utc::now().timestamp_millis())?;
                    } else if !actor.row_failure() {
                        record_fault(actor, cycle_fault, &error)?;
                    }
                    false
                }
            };
            if !advanced {
                break;
            }
        }
        actor.release();
    }
    *cursor = page.next;
    if let Err(error) = actor.maintain(store) {
        record_fault(actor, cycle_fault, &error)?;
    }
    if cursor.is_none() {
        // A healthy page cannot clear a fault from another page in this sweep.
        if !*cycle_fault {
            actor.clear_health()?;
        }
        *cycle_fault = false;
    }
    Ok(())
}
fn record_fault(
    actor: &mut launch::Runtime,
    cycle_fault: &mut bool,
    error: &zerus_team_plan::Problem,
) -> super::ports::Result<()> {
    *cycle_fault = true;
    actor.record_health(error, chrono::Utc::now().timestamp_millis())
}
fn page(
    store: &mut Store,
    cursor: Option<&crate::teams::store::Cursor>,
) -> super::ports::Result<crate::teams::store::Page<crate::teams::store::TeamRow>> {
    store.refresh()?;
    store.live_teams(16, cursor)
}
