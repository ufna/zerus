//! The worker's thread keeps the service owner lock it was given until the
//! thread itself ends, also after its Worker handle and the original owner
//! are dropped. Two actor runtimes never act on one ledger because of this.
use super::{entry_hold::Hold, *};
use crate::desktop::store::Location;

const MACHINE: &str = "00000000-0000-4000-8000-0000000000bb";

/// A private state folder of the mode the desktop requires, with Teams on.
fn fixture() -> (tempfile::TempDir, Config, Location) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let config = Config {
        home: root.to_str().unwrap().into(),
        dir: root.join("config"),
        state: root.join("state"),
        selves: vec![],
        peers: vec![],
        base_peers: vec![],
        peers_from_environment: false,
        machines: Default::default(),
        tab: false,
        tab_colors: String::new(),
    };
    std::fs::create_dir_all(&config.dir).unwrap();
    let location = Location::new(&config).unwrap();
    location.prepare().unwrap();
    platform::private_dir(&config.state.join("teams")).unwrap();
    std::fs::write(config.state.join("teams/preview"), "on").unwrap();
    assert!(crate::teams::enabled(&config.state));
    (temp, config, location)
}

/// Owns the paused run for the test's whole life. On every exit, an
/// assertion failure included, it stops and releases the thread and joins it.
struct Paused {
    hold: Hold,
    worker: Option<Worker>,
    thread: Option<JoinHandle<()>>,
}
impl Paused {
    fn worker(&mut self) -> &mut Worker {
        self.worker
            .as_mut()
            .expect("the worker handle is still held")
    }
    /// Drops the Worker handle; only the thread's join handle is kept.
    fn drop_worker(&mut self) {
        let mut worker = self.worker.take().expect("the worker handle is held");
        self.thread = worker.work.take().map(|work| work.thread);
        drop(worker);
    }
    fn join(&mut self) -> thread::Result<()> {
        if let Some(worker) = &mut self.worker {
            worker.request_stop();
        }
        self.hold.release();
        let thread = self
            .thread
            .take()
            .or_else(|| Some(self.worker.as_mut()?.work.take()?.thread));
        thread.map_or(Ok(()), JoinHandle::join)
    }
}
impl Drop for Paused {
    fn drop(&mut self) {
        let _ = self.join();
    }
}

#[test]
fn the_worker_thread_keeps_the_service_owner_after_its_handle_until_it_ends() {
    let (_temp, config, location) = fixture();
    let owner = Arc::new(location.lock().unwrap());
    let mut run = Paused {
        hold: Hold::install(&config.state),
        worker: Some(Worker::new(&config, "owner-pin", owner.clone())),
        thread: None,
    };
    run.worker().fixture_machine(MACHINE);
    run.worker().poll();
    assert!(
        run.worker().work.is_some(),
        "poll starts the worker's real thread"
    );
    assert!(
        run.hold.entered(Duration::from_secs(10)),
        "the thread reaches its run entry, before any launch or native effect"
    );
    run.worker().request_stop();
    assert!(!run.worker().drained(), "a stop request is not a drain");
    run.drop_worker();
    drop(owner);
    // A second service on the same state folder takes its own lock.
    let second = Location::new(&config).unwrap();
    let Err(refused) = second.lock() else {
        panic!("the paused thread still holds the service owner");
    };
    assert!(
        format!("{refused:#}").contains("another desktop service owns this state"),
        "{refused:#}"
    );
    run.join().expect("the released thread returns");
    second
        .lock()
        .expect("the owner is free once the thread has ended");
}
