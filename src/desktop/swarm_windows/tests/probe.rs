//! Observability only. Handles come directly from this fixture's originals;
//! no PID lookup, process adoption, termination or extra job handle is used.
use super::*;
use std::{
    fs::OpenOptions,
    io::Read,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Weak,
};

const RUNS: &str = "ZERUS_SWARM_PROBE_RUNS";
const OUTPUT: &str = "ZERUS_SWARM_PROBE_OUTPUT";
const LOG_BYTES: usize = 2 * 1024 * 1024;
const CHILD_BYTES: u64 = 8192;
const OBSERVE_AFTER_FAILURE: Duration = Duration::from_secs(20);

struct Sink {
    file: fs::File,
    bytes: usize,
}
impl Sink {
    fn write(&mut self, value: &Value) {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        if self.bytes + bytes.len() > LOG_BYTES {
            return;
        }
        self.bytes += bytes.len();
        let _ = self.file.write_all(&bytes);
        let _ = self.file.flush();
    }
}
struct Process {
    pid: u32,
    handle: Option<OwnedHandle>,
}
struct JobSample {
    at: Instant,
    exited: bool,
    empty: Option<bool>,
    held: bool,
    error: Option<String>,
}
#[derive(Default)]
struct Observations {
    processes: Vec<Process>,
    thread: Option<OwnedHandle>,
    job: Option<JobSample>,
    phase: &'static str,
    last_status: Option<Value>,
    completed: bool,
}
pub(in crate::desktop::swarm_windows) struct Trace {
    case: usize,
    start: Instant,
    sink: Arc<Mutex<Sink>>,
    shared: Mutex<Weak<Shared>>,
    owner: Mutex<Weak<platform::FileLock>>,
    observed: Mutex<Observations>,
}
fn duplicate_sync(handle: *mut std::ffi::c_void) -> Option<OwnedHandle> {
    let mut copy = std::ptr::null_mut();
    // Read-only lifetime observation of the exact original handle. A process
    // or thread SYNCHRONIZE handle cannot terminate anything.
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut copy,
            PROCESS_SYNCHRONIZE,
            0,
            0,
        )
    };
    (ok != 0).then(|| unsafe { OwnedHandle::from_raw_handle(copy) })
}
fn signalled(handle: &Option<OwnedHandle>) -> Value {
    match handle
        .as_ref()
        .map(|h| unsafe { WaitForSingleObject(h.as_raw_handle(), 0) })
    {
        Some(WAIT_OBJECT_0) => true.into(),
        Some(WAIT_TIMEOUT) => false.into(),
        _ => Value::Null,
    }
}
impl Trace {
    pub(super) fn attach(&self, runtime: &Runtime, owner: &Arc<platform::FileLock>) {
        *self.shared.lock().unwrap() = Arc::downgrade(&runtime.shared);
        *self.owner.lock().unwrap() = Arc::downgrade(owner);
    }
    pub(super) fn thread(&self, runtime: &Runtime) {
        self.observed.lock().unwrap().thread = runtime
            .thread
            .as_ref()
            .and_then(|t| duplicate_sync(t.as_raw_handle()));
        self.record("supervisor_created");
    }
    fn record(&self, event: &str) {
        let shared = self.shared.lock().unwrap().upgrade();
        let status = shared.as_ref().map(|s| {
            let v = s.status.lock().unwrap_or_else(PoisonError::into_inner);
            json!({"state":v.state,"pid":v.pid,"starts":v.starts,
                "cleanup_unknown":v.cleanup_unknown,
                "starting_elapsed_ms":v.starting.map(|at|at.elapsed().as_millis()),
                "error":v.error.as_ref().map(|e|e.chars().take(512).collect::<String>())})
        });
        let mut state = self.observed.lock().unwrap();
        if status.is_some() {
            state.last_status = status;
        }
        let processes: Vec<_> = state
            .processes
            .iter()
            .map(|p| json!({"pid":p.pid,"original_process_exited":signalled(&p.handle)}))
            .collect();
        let job = state.job.as_ref().map(|sample| {
            json!({"sample_age_ms":sample.at.elapsed().as_millis(),"original_process_exited":sample.exited,
                "original_job_empty":sample.empty,"held":sample.held,"query_error":sample.error})
        });
        self.sink.lock().unwrap().write(&json!({"case":self.case,"event":event,
            "elapsed_ms":self.start.elapsed().as_millis(),"phase":state.phase,"status":state.last_status,
            "done":state.completed,
            "original_thread_exited":signalled(&state.thread),"original_processes":processes,
            "last_original_job_sample":job,"owner_strong":self.owner.lock().unwrap().strong_count()}));
    }
    fn settled(&self) -> bool {
        let state = self.observed.lock().unwrap();
        state.completed && signalled(&state.thread) == true
    }
}
fn trace(shared: &Shared) -> Option<Arc<Trace>> {
    shared
        .probe
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}
pub(in crate::desktop::swarm_windows) fn completed(shared: &Shared) {
    if let Some(trace) = trace(shared) {
        trace.observed.lock().unwrap().completed = true;
        trace.record("supervisor_done_after_custody_drop");
    }
}
pub(in crate::desktop::swarm_windows) fn created(shared: &Shared, child: &platform::Spawned) {
    let Some(trace) = trace(shared) else {
        return;
    };
    let mut state = trace.observed.lock().unwrap();
    // The scenario has two intended children; retain a bounded set if a
    // failed startup triggers the unchanged supervisor backoff.
    if state.processes.len() < 16 {
        state.processes.push(Process {
            pid: child.id(),
            handle: duplicate_sync(child.as_raw_handle()),
        });
    }
    state.job = None;
    drop(state);
    trace.record("original_child_created");
}
pub(in crate::desktop::swarm_windows) fn sample(
    shared: &Shared,
    exited: bool,
    tree: &io::Result<bool>,
    held: bool,
) {
    let Some(trace) = trace(shared) else {
        return;
    };
    let empty = tree.as_ref().ok().copied();
    let error = tree
        .as_ref()
        .err()
        .map(|e| e.to_string().chars().take(512).collect::<String>());
    let mut state = trace.observed.lock().unwrap();
    let changed = state
        .job
        .as_ref()
        .is_none_or(|v| (v.exited, v.empty, v.held, &v.error) != (exited, empty, held, &error));
    state.job = Some(JobSample {
        at: Instant::now(),
        exited,
        empty,
        held,
        error,
    });
    drop(state);
    if changed {
        trace.record("original_cleanup_observation");
    }
}
pub(super) fn phase<T>(shared: &Arc<Shared>, name: &'static str, body: impl FnOnce() -> T) -> T {
    let Some(trace) = trace(shared) else {
        return body();
    };
    trace.observed.lock().unwrap().phase = name;
    trace.record("phase_begin");
    struct End(Arc<Trace>);
    impl Drop for End {
        fn drop(&mut self) {
            self.0.record(if thread::panicking() {
                "phase_panic"
            } else {
                "phase_end"
            });
        }
    }
    let _end = End(trace);
    body()
}
pub(in crate::desktop::swarm_windows) fn capture(
    launch: &Launch,
    command: &mut Command,
) -> io::Result<()> {
    if std::env::var_os(RUNS).is_some() {
        if let Launch::Fixture(marker) = launch {
            // Files add no pipe writer/EOF or job lifetime. Only this private
            // fixture writes them; the report reads at most CHILD_BYTES each.
            command.stdout(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(marker.with_extension("stdout"))?,
            );
            command.stderr(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(marker.with_extension("stderr"))?,
            );
        }
    }
    Ok(())
}
fn child_output(root: &Path, extension: &str) -> Value {
    let path = root.join("started").with_extension(extension);
    match fs::File::open(path) {
        Ok(file) => {
            let total = file.metadata().map(|m| m.len()).ok();
            let mut bytes = Vec::new();
            match file.take(CHILD_BYTES).read_to_end(&mut bytes) {
                Ok(_) => json!({"bytes":total,"truncated":total.is_some_and(|n|n>CHILD_BYTES),
                    "text":String::from_utf8_lossy(&bytes)}),
                Err(error) => json!({"error":error.to_string()}),
            }
        }
        Err(error) => json!({"error":error.to_string()}),
    }
}
pub(super) fn run() -> bool {
    let Some(runs) = std::env::var_os(RUNS) else {
        return false;
    };
    let runs: usize = runs.to_str().unwrap().parse().unwrap();
    assert!((30..=64).contains(&runs));
    let output = std::path::PathBuf::from(std::env::var_os(OUTPUT).expect("probe artifact path"));
    assert!(output.is_absolute());
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let sink = Arc::new(Mutex::new(Sink { file, bytes: 0 }));
    let mut failed = Vec::new();
    for case in 1..=runs {
        let root = tempfile::tempdir().unwrap();
        let trace = Arc::new(Trace {
            case,
            start: Instant::now(),
            sink: sink.clone(),
            shared: Mutex::new(Weak::new()),
            owner: Mutex::new(Weak::new()),
            observed: Mutex::new(Observations::default()),
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            super::restart_case(root.path(), Some(&trace))
        }));
        if result.is_err() {
            failed.push(case);
            trace.record("case_failed");
            // Observation only AFTER the original bound failed. Runtime::Drop
            // has already requested stop using its original controls/custody.
            let until = Instant::now() + OBSERVE_AFTER_FAILURE;
            while !trace.settled() && Instant::now() < until {
                trace.record("after_failure_observation");
                thread::sleep(Duration::from_millis(250));
            }
        }
        trace.record(if trace.settled() {
            "physical_settlement_observed"
        } else {
            "physical_settlement_unconfirmed"
        });
        sink.lock().unwrap().write(&json!({"case":case,"event":"child_diagnostics",
            "stdout":child_output(root.path(),"stdout"),"stderr":child_output(root.path(),"stderr")}));
    }
    sink.lock()
        .unwrap()
        .write(&json!({"event":"summary","runs":runs,"failed":failed}));
    assert!(
        failed.is_empty(),
        "swarm probe failed in original cases {failed:?}; see bounded JSONL artifact"
    );
    true
}
