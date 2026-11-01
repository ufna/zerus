//! Qt's Windows-only swarm catalog worker. Own process/job only: never an agent.
use std::os::windows::{io::AsRawHandle, process::CommandExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::{JobObjects::*, Threading::*};

pub struct Worker {
    stop: Arc<AtomicBool>,
    wake: Arc<(Mutex<()>, Condvar)>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Worker {
    pub fn start(hgs: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new((Mutex::new(()), Condvar::new()));
        let stopped = stop.clone();
        let notify = wake.clone();
        let thread = std::thread::spawn(move || {
            let mut delay = 0;
            while !stopped.load(Ordering::Acquire) {
                let job = match super::owned(unsafe {
                    CreateJobObjectW(std::ptr::null(), std::ptr::null())
                }) {
                    Ok(job) => job,
                    Err(e) => {
                        eprintln!("zerus-client: swarm worker job: {e}");
                        return;
                    }
                };
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                    unsafe { std::mem::zeroed() };
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if unsafe {
                    SetInformationJobObject(
                        job.as_raw_handle(),
                        JobObjectExtendedLimitInformation,
                        (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                        std::mem::size_of_val(&limits) as u32,
                    )
                } == 0
                {
                    eprintln!(
                        "zerus-client: cannot contain the swarm worker: {}",
                        std::io::Error::last_os_error()
                    );
                    return;
                }
                let start = Instant::now();
                let child = Command::new(&hgs)
                    .args(["swarm", "worker"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn();
                if let Ok(mut child) = child {
                    if unsafe {
                        AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle())
                    } == 0
                    {
                        eprintln!(
                            "zerus-client: cannot contain the swarm worker: {}",
                            std::io::Error::last_os_error()
                        );
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                    let errors = child.stderr.take().map(|mut pipe| {
                        std::thread::spawn(move || {
                            use std::io::Read;
                            let mut tail = Vec::new();
                            let mut block = [0; 4096];
                            while let Ok(n) = pipe.read(&mut block) {
                                if n == 0 {
                                    break;
                                }
                                tail.extend_from_slice(&block[..n]);
                                if tail.len() > 4096 {
                                    tail.drain(..tail.len() - 4096);
                                }
                            }
                            tail
                        })
                    });
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                if !status.success() {
                                    eprintln!("zerus-client: swarm worker exited: {status}");
                                }
                                break;
                            }
                            Err(e) => {
                                eprintln!("zerus-client: swarm worker wait: {e}");
                                break;
                            }
                            _ => {}
                        }
                        if stopped.load(Ordering::Acquire) {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        let guard = notify
                            .0
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let _ = notify.1.wait_timeout(guard, Duration::from_millis(50));
                    }
                    // Closing THIS job releases only its worker/ssh descendants.
                    drop(job);
                    if let Some(errors) = errors {
                        if let Ok(tail) = errors.join() {
                            if !tail.is_empty() {
                                eprintln!(
                                    "zerus-client: swarm worker: {}",
                                    String::from_utf8_lossy(&tail)
                                );
                            }
                        }
                    }
                } else if let Err(error) = child {
                    eprintln!("zerus-client: cannot start swarm worker: {error}");
                }
                if stopped.load(Ordering::Acquire) {
                    return;
                }
                delay = crate::windows_shell::restart_delay(delay, start.elapsed());
                let guard = notify
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let _ = notify
                    .1
                    .wait_timeout_while(guard, Duration::from_millis(delay), |_| {
                        !stopped.load(Ordering::Acquire)
                    });
            }
        });
        Self {
            stop,
            wake,
            thread: Mutex::new(Some(thread)),
        }
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake.1.notify_all();
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = thread.join();
        }
    }
}
