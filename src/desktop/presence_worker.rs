//! One sampler/writer, outside the service actor. Its fence lives on the worker
//! until sampling and the last atomic write have actually returned.
use super::{presence_admission, store::Location};
use crate::presence;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zerus_presence_sampler::{Sample, Sampler};

const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Observation {
    code: &'static str,
    written: Option<Instant>,
}

pub(super) struct Runtime {
    stop: Arc<AtomicBool>,
    wake: SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    observation: Arc<Mutex<Observation>>,
    failed: bool,
}

impl Runtime {
    pub fn start(location: &Location) -> Result<Self> {
        Self::spawn(location, INTERVAL, || {
            let mut sampler = Sampler::new();
            move || sampler.sample()
        })
    }

    fn spawn<F: FnMut() -> Sample + 'static>(
        location: &Location,
        interval: Duration,
        make_sampler: impl FnOnce() -> F + Send + 'static,
    ) -> Result<Self> {
        let fence = presence_admission::fence(location)?;
        fence.verify(location)?;
        let state = location
            .root
            .parent()
            .context("missing state directory")?
            .to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let observation = Arc::new(Mutex::new(Observation {
            code: "starting",
            written: None,
        }));
        let (wake, receive) = mpsc::sync_channel(1);
        let worker = {
            let stop = stop.clone();
            let observation = observation.clone();
            thread::Builder::new()
                .name("desktop-presence".into())
                .spawn(move || {
                    let _fence = fence;
                    let mut sample = make_sampler();
                    while !stop.load(Ordering::SeqCst) {
                        let input = sample();
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let result = presence::write_sample(&state, input, presence::now());
                        let mut status = observation.lock().unwrap_or_else(|e| e.into_inner());
                        status.code = match result {
                            Ok(true) => {
                                status.written = Some(Instant::now());
                                "published"
                            }
                            Ok(false) => "inputs_unknown",
                            Err(_) => "write_failed",
                        };
                        drop(status);
                        if receive.recv_timeout(interval).is_ok() {
                            break;
                        }
                    }
                })?
        };
        Ok(Self {
            stop,
            wake,
            worker: Some(worker),
            observation,
            failed: false,
        })
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.wake.try_send(());
    }

    /// Never block the actor in a join of a worker still sampling or writing.
    pub fn stop(&mut self) -> Result<bool> {
        self.request_stop();
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.failed = self.worker.take().unwrap().join().is_err();
        }
        ensure!(!self.failed, "desktop presence worker panicked");
        Ok(self.worker.is_none())
    }

    pub fn status(&self) -> Value {
        let observation = self.observation.try_lock().ok().map(|s| *s);
        let unexpectedly_finished = !self.stop.load(Ordering::SeqCst)
            && self.worker.as_ref().is_some_and(JoinHandle::is_finished);
        let mut code = if self.failed || unexpectedly_finished {
            "worker_failed"
        } else if self.worker.is_none() {
            "drained"
        } else if self.stop.load(Ordering::SeqCst) {
            "draining"
        } else {
            observation.map_or("status_unavailable", |s| s.code)
        };
        if code == "published"
            && observation
                .and_then(|s| s.written)
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(5))
        {
            code = "sample_stale";
        }
        json!({"code":code,"ready":code=="published","drained":code=="drained",
            "writer_active":self.worker.is_some() && !self.failed && !unexpectedly_finished,
            "last_write_age_seconds":observation.and_then(|s|s.written).map(|at|at.elapsed().as_secs_f64()),
            "interval_seconds":INTERVAL.as_secs(),"workers":1})
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.request_stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerus_desktop_migration::canonical;
    use zerus_presence_sampler::{Idle, IdleSource};

    fn fixture() -> (tempfile::TempDir, Location) {
        let temp = tempfile::tempdir().unwrap();
        let location = Location {
            root: canonical(&temp.path().join("state/desktop")).unwrap(),
            configuration: "a".repeat(64),
        };
        (temp, location)
    }

    fn drain(runtime: &mut Runtime) {
        let end = Instant::now() + Duration::from_secs(5);
        while !runtime.stop().unwrap() {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn writer_fence_survives_sampling_and_releases_only_after_join() {
        let (_temp, location) = fixture();
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let mut runtime = Runtime::spawn(&location, INTERVAL, move || {
            move || {
                entered.send(()).unwrap();
                blocked.recv().unwrap();
                Sample {
                    idle: Some(Idle {
                        elapsed: Duration::ZERO,
                        source: IdleSource::Mutter,
                        since_input: true,
                    }),
                    locked: Some(false),
                }
            }
        })
        .unwrap();
        observed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(presence_admission::fence(&location).is_err());
        assert!(!runtime.stop().unwrap());
        assert!(presence_admission::fence(&location).is_err());
        release.send(()).unwrap();
        drain(&mut runtime);
        assert!(presence_admission::fence(&location).is_ok());
        assert!(
            presence::listing(location.root.parent().unwrap(), presence::now()).is_none(),
            "cancellation during sample must not publish it"
        );
    }

    #[test]
    fn worker_publishes_without_actor_polling_then_stops() {
        let (_temp, location) = fixture();
        let mut runtime = Runtime::spawn(&location, INTERVAL, || {
            || Sample {
                idle: Some(Idle {
                    elapsed: Duration::from_secs(7),
                    source: IdleSource::Mutter,
                    since_input: true,
                }),
                locked: Some(true),
            }
        })
        .unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while runtime.status()["ready"] != true {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
        let value = presence::listing(location.root.parent().unwrap(), presence::now()).unwrap();
        assert_eq!(value["locked"], true);
        assert!(value["idle_seconds"].as_f64().unwrap() >= 7.0);
        drain(&mut runtime);
        assert_eq!(runtime.status()["drained"], true);
    }

    #[test]
    fn failed_write_keeps_the_fence_until_shutdown() {
        let (_temp, location) = fixture();
        crate::platform::private_dir(location.root.parent().unwrap()).unwrap();
        let leaf = location
            .root
            .parent()
            .unwrap()
            .join("desktop-presence.json");
        std::fs::create_dir(&leaf).unwrap();
        let mut runtime = Runtime::spawn(&location, INTERVAL, || {
            || Sample {
                idle: Some(Idle {
                    elapsed: Duration::ZERO,
                    source: IdleSource::Mutter,
                    since_input: true,
                }),
                locked: Some(false),
            }
        })
        .unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while runtime.status()["code"] != "write_failed" {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(presence_admission::fence(&location).is_err());
        assert!(
            leaf.is_dir(),
            "a failed write must not repair the obstructing object"
        );
        drain(&mut runtime);
        assert!(presence_admission::fence(&location).is_ok());
    }

    #[test]
    fn worker_panic_is_an_error_instead_of_a_drained_receipt() {
        let (_temp, location) = fixture();
        let mut runtime =
            Runtime::spawn(&location, INTERVAL, || || panic!("fixture sampler failure")).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while !runtime.worker.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(runtime.status()["code"], "worker_failed");
        assert!(runtime.stop().is_err());
        assert!(
            runtime.stop().is_err(),
            "later polls cannot clear a failed join"
        );
        assert_eq!(runtime.status()["drained"], false);
    }
}
