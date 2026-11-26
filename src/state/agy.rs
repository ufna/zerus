//! Native Antigravity sessions own an ACP child for each Zerus run. Only the
//! worker holding that child can offer input or confirm a lifecycle effect.
use super::*;
use serde::Deserialize;
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

mod acp;
mod worker;
pub(super) use worker::run as run_worker;

pub(crate) const ACCOUNT: &str = "native-agy";
const CONTROL_BYTES: u64 = 64 << 10;
const MESSAGE_BYTES: usize = 28 << 10;
const ACTION_LIMIT: Duration = Duration::from_secs(8);

pub(super) fn state_guard() -> Result<Guard> {
    storage::writer_until(Instant::now() + Duration::from_secs(2))?
        .ok_or_else(|| "Antigravity session state is busy; refresh before retrying".into())
}

fn ordinary_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MESSAGE_BYTES && !text.trim_start().starts_with('/')
}

pub(crate) fn executable() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let local = crate::platform::home().map(|home| Path::new(&home).join(".local/bin"));
    std::env::split_paths(&path)
        .chain(local)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join("agy"))
        .find(|file| crate::platform::executable(file))
}

pub(super) fn named(name: &str) -> bool {
    name.starts_with("agy/") && zerus_client_core::desktop::creation_launch::session_name(name)
}

fn binding(name: &str) -> Result<Value> {
    if !named(name) {
        return Err("Invalid Antigravity session name".into());
    }
    let record = storage::read_metadata(name)?;
    if record["agent"] != "agy"
        || record["backend"] != "agy"
        || record["account_id"] != ACCOUNT
        || record["rename_shadow"] == true
        || !string(&record, "archive_id").is_empty()
        || !session_id::canonical(string(&record, "session_id"))
        || !session_id::canonical(string(&record, "run_id"))
    {
        return Err("The saved Antigravity binding is unavailable".into());
    }
    Ok(record)
}

fn verify(current: &Value, expected: &Value) -> Result<()> {
    if [
        "name",
        "session_id",
        "run_id",
        "conversation_id",
        "account_id",
        "agent_home",
        "cwd",
        "agy_workspace",
        "agy_account_location",
    ]
    .into_iter()
    .any(|key| current[key] != expected[key])
    {
        return Err("The Antigravity session changed; nothing was submitted".into());
    }
    terminating::check_mutation(current)
}

fn process_pin(pid: u32) -> Result<Value> {
    let boot =
        process_index_writer::boot().map_err(|_| "Antigravity boot identity is unavailable")?;
    let birth = crate::platform::process_identity(pid)
        .ok_or("Antigravity process identity is unavailable")?
        .birth;
    Ok(json!({"pid":pid,"birth":birth,"boot":boot}))
}

fn pinned_state(pin: &Value) -> Option<crate::platform::ProcessState> {
    use crate::platform::ProcessState;
    if pin.is_null() {
        return Some(ProcessState::Exited);
    }
    let pid = pin["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())?;
    let birth = pin["birth"].as_str().filter(|birth| !birth.is_empty())?;
    let boot = pin["boot"].as_str().filter(|boot| !boot.is_empty())?;
    let local = process_index_writer::boot().ok()?;
    if local != boot {
        let (local_boot, _) = local.split_once(":uid:")?;
        let (original_boot, _) = boot.split_once(":uid:")?;
        return (local_boot != original_boot).then_some(ProcessState::Exited);
    }
    crate::platform::process_state(pid, Some(birth)).ok()
}

fn worker_live(record: &Value) -> bool {
    pinned_state(&record["agy_worker"]) == Some(crate::platform::ProcessState::Running)
}

fn original_exited(record: &Value) -> bool {
    ["agy_worker", "agy_provider"]
        .into_iter()
        .all(|key| pinned_state(&record[key]) == Some(crate::platform::ProcessState::Exited))
}

fn folder_id(path: &Path) -> Result<Value> {
    let (file, meta) = crate::platform::FileIdentity::at(path)
        .map_err(|_| "Antigravity location is unavailable")?;
    if !meta.is_dir() {
        return Err("Antigravity location is not a directory".into());
    }
    Ok(json!([file.device, file.inode]))
}

fn locations(record: &Value) -> Result<()> {
    let selected = Path::new(string(record, "agent_home"));
    if home_path()? != selected
        || folder_id(selected)? != record["agy_account_location"]
        || folder_id(Path::new(string(record, "cwd")))? != record["agy_workspace"]
    {
        return Err("The Antigravity account home or workspace changed".into());
    }
    Ok(())
}

fn socket_path(record: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(string(record, "agy_socket"));
    let folder = path
        .parent()
        .ok_or("Antigravity control directory is missing")?;
    if !path.is_absolute()
        || path.file_name().and_then(|name| name.to_str()) != Some("control")
        || !crate::platform::verify_private(folder, crate::platform::Private::Directory)
            .unwrap_or(false)
    {
        return Err("Antigravity control directory is unverified".into());
    }
    Ok(path)
}

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn home_path() -> Result<PathBuf> {
    let home = crate::platform::home().ok_or("User home is unavailable")?;
    crate::platform::canonical(home)
        .map_err(|_| "The Antigravity account home is unavailable".into())
}

fn start(mut record: Value) -> Result<Value> {
    if executable().is_none() {
        return Err("Install the official Antigravity CLI before starting a session".into());
    }
    locations(&record)?;
    record["agy_worker"] = Value::Null;
    record["agy_provider"] = Value::Null;
    record["agy_socket"] = Value::Null;
    record["state"] = json!("starting");
    record["phase"] = json!("starting");
    record["activity"] = json!("unknown");
    record["auth_required"] = json!(false);
    record["error"] = Value::Null;
    record["turn_started"] = json!(0);
    write(&mut record)?;
    Ok(record)
}

// Process creation is outside the state writer lock. The worker accepts only
// this exact still-starting binding and holds its own per-run control endpoint.
fn spawn_worker(record: &Value) -> Result<Value> {
    let name = string(record, "name");
    let run = string(record, "run_id");
    let binary = std::env::current_exe().map_err(|_| "Cannot locate this hgs executable")?;
    let mut child = Command::new(binary)
        .args(["__state", "agy-worker", name, run])
        .env("HGS_STATE_DIR", root())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not launch the Antigravity worker")?;
    let until = Instant::now() + ACTION_LIMIT;
    while Instant::now() < until {
        let current = binding(name)?;
        if current["run_id"] != run {
            return Err("The Antigravity launch binding changed".into());
        }
        if current["state"] == "running" && worker_live(&current)
            || current["auth_required"] == true
        {
            return Ok(current);
        }
        if let Some(error) = current["error"].as_str() {
            return Err(error.into());
        }
        if child
            .try_wait()
            .map_err(|_| "Cannot verify the Antigravity worker")?
            .is_some()
        {
            let latest = binding(name)?;
            if latest["run_id"] == run && latest["auth_required"] == true {
                return Ok(latest);
            }
            return Err(latest["error"]
                .as_str()
                .unwrap_or("Antigravity worker exited before startup")
                .into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("Antigravity startup is unconfirmed. Inspect the original session".into())
}

pub(crate) fn launch(
    name: &str,
    directory: &str,
    launch_id: Option<&str>,
    dry: bool,
) -> Result<Value> {
    if !named(name) || launch_id.is_some_and(|id| !session_id::canonical(id)) {
        return Err("Invalid Antigravity launch identity".into());
    }
    let cwd = crate::platform::canonical(directory)
        .map_err(|_| "Antigravity workspace is unavailable")?;
    if !cwd.is_dir() {
        return Err("Antigravity workspace is not a directory".into());
    }
    if dry {
        return Ok(json!({"provider":"agy","name":name,"cwd":cwd,"transport":"acp"}));
    }
    let launch = uuid::Uuid::new_v4().to_string();
    let record = {
        let _guard = state_guard()?;
        if labels::terminal_taken(name)? || live()?.contains_key(name) {
            return Err("This Antigravity name is already used; choose another name or resume the saved session".into());
        }
        let home = home_path()?;
        start(json!({
            "version":1,"name":name,"agent":"agy","backend":"agy","run_identity_version":1,
            "session_id":uuid::Uuid::new_v4(),"run_id":uuid::Uuid::new_v4(),
            "conversation_id":"","launch_id":launch_id.unwrap_or(&launch),"created":now(),
            "cwd":cwd,"launch_dir":cwd,"account_id":ACCOUNT,"agent_home":home,"agy_account_location":folder_id(&home)?,"agy_workspace":folder_id(&cwd)?,
            "agy_events":[],"agy_seq":0
        }))?
    };
    spawn_worker(&record)
}

fn resume(name: &str, scope: &session_action::Scope) -> Result<Value> {
    let record = {
        let _guard = state_guard()?;
        let mut record = binding(name)?;
        scope.check(&record)?;
        if !original_exited(&record) || record["state"] == "starting" {
            return Err(
                "Antigravity's original worker is still running or startup is unconfirmed".into(),
            );
        }
        if !session_id::canonical(string(&record, "conversation_id")) {
            return Err(
                "This Antigravity session has no exact native conversation to resume".into(),
            );
        }
        scope.changing();
        record["run_id"] = json!(uuid::Uuid::new_v4());
        start(record)?
    };
    let current = spawn_worker(&record)?;
    if current["auth_required"] == true {
        return Err("Antigravity sign-in required; sign in from Terminal".into());
    }
    scope.result_record(&current);
    Ok(current)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prompt {
    request_id: String,
    expected_run_id: String,
    expected_conversation_id: String,
    text: String,
    #[serde(default)]
    attachments: Vec<Value>,
}

fn message_request() -> Result<(Prompt, String)> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(CONTROL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read Antigravity input")?;
    if bytes.len() as u64 > CONTROL_BYTES {
        return Err("Antigravity request exceeds its bound".into());
    }
    let prompt: Prompt =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid Antigravity prompt request")?;
    if !session_id::canonical(&prompt.request_id)
        || !session_id::canonical(&prompt.expected_run_id)
        || !session_id::canonical(&prompt.expected_conversation_id)
        || !prompt.attachments.is_empty()
        || !ordinary_text(&prompt.text)
    {
        return Err("Antigravity requires an exact text request without attachments".into());
    }
    Ok((prompt, digest(&bytes)))
}

fn frame(stream: &mut UnixStream) -> Result<Value> {
    let mut bytes = Vec::new();
    let until = Instant::now() + stream.read_timeout().ok().flatten().unwrap_or(ACTION_LIMIT);
    let mut chunk = [0; 4096];
    loop {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Antigravity control read timed out".into());
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| "Cannot bound Antigravity control reads")?;
        let consumed = stream
            .read(&mut chunk)
            .map_err(|_| "Antigravity control reply is unavailable")?;
        if consumed == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..consumed]);
        if bytes.len() as u64 > CONTROL_BYTES {
            break;
        }
    }
    if bytes.len() as u64 > CONTROL_BYTES {
        return Err("Antigravity control frame is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid Antigravity control frame".into())
}

fn respond(stream: &mut UnixStream, answer: &Value) -> Result<()> {
    let bytes =
        serde_json::to_vec(answer).map_err(|_| "Cannot encode Antigravity control frame")?;
    if bytes.len() as u64 > CONTROL_BYTES {
        return Err("Antigravity control frame is too large".into());
    }
    stream
        .write_all(&bytes)
        .map_err(|_| "Antigravity control delivery is unconfirmed")?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|_| "Antigravity control delivery is unconfirmed".into())
}

fn contact(mut record: Value, mut request: Value, before: impl FnOnce()) -> Result<Value> {
    if !worker_live(&record) {
        return Err("Antigravity's original worker is unavailable; inspect the session".into());
    }
    let path = socket_path(&record)?;
    let mut stream = crate::platform::connect_unix(&path, Duration::from_millis(500))
        .map_err(|_| "Antigravity worker is unavailable")?;
    let worker_pid = record["agy_worker"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok());
    if worker_pid.is_none() || dsh::host_peer(&stream) != worker_pid {
        return Err("Antigravity worker peer changed; nothing was submitted".into());
    }
    let current = binding(string(&record, "name"))?;
    verify(&current, &record)?;
    if current["agy_worker"] != record["agy_worker"]
        || current["agy_socket"] != record["agy_socket"]
        || !worker_live(&current)
    {
        return Err("Antigravity worker changed; nothing was submitted".into());
    }
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .map_err(|_| "Cannot bound Antigravity control writes")?;
    stream
        .set_read_timeout(Some(ACTION_LIMIT))
        .map_err(|_| "Cannot bound Antigravity control reads")?;
    for key in ["name", "session_id", "run_id", "conversation_id"] {
        request[key] = record[key].take();
    }
    before();
    respond(&mut stream, &request)?;
    frame(&mut stream)
}

fn dispatch_send(name: &str, args: &[String]) -> Result<i32> {
    if args != ["--json"] {
        return Err("usage: hgs send <Antigravity session> --json".into());
    }
    let (prompt, digest) = message_request()?;
    let current = binding(name)?;
    if current["run_id"] != prompt.expected_run_id
        || current["conversation_id"] != prompt.expected_conversation_id
    {
        return Err("The Antigravity session changed; nothing was submitted".into());
    }
    if let Some(previous) = worker::receipt(&prompt.request_id, &current, &digest)? {
        return print_receipt(&previous);
    }
    let answer = contact(
        current,
        json!({"action":"send","request_id":prompt.request_id,"digest":digest,"text":prompt.text}),
        || {},
    )?;
    print_receipt(&answer)
}

fn print_receipt(answer: &Value) -> Result<i32> {
    if let Some(error) = answer["error"].as_str() {
        return Err(error.into());
    }
    println!("{answer}");
    Ok(if answer["status"] == "submitted" {
        0
    } else {
        1
    })
}

pub(super) fn action(name: &str, action: &str, scope: &session_action::Scope) -> Result<()> {
    match action {
        "resume" => resume(name, scope).map(|_| ()),
        "pause" => {
            let record = binding(name)?;
            scope.check(&record)?;
            let answer = contact(record.clone(), json!({"action":"pause"}), || {
                scope.changing()
            })?;
            if answer["status"] == "refused" {
                scope.unchanged();
                return Err(string(&answer, "error").to_owned());
            }
            if answer["status"] != "stopped" {
                return Err("Antigravity stop is unconfirmed; inspect the original session".into());
            }
            scope.result_record(&record);
            Ok(())
        }
        _ => Err("That lifecycle action is unavailable for Antigravity".into()),
    }
}

pub(super) fn interrupt(record: &Value, expected_turn: f64) -> Result<()> {
    let original = binding(string(record, "name"))?;
    if ["run_id", "conversation_id", "session_id"]
        .into_iter()
        .any(|key| original[key] != record[key])
    {
        return Err("The Antigravity session changed; nothing was stopped".into());
    }
    let answer = contact(
        original,
        json!({"action":"interrupt","turn_started":expected_turn}),
        || {},
    )?;
    if answer["status"] == "interrupted" {
        Ok(())
    } else {
        Err("Antigravity did not confirm cancellation of the original turn".into())
    }
}

pub(super) fn inspect(name: &str) -> Result<Value> {
    Ok(row(&binding(name)?, true))
}

pub(super) fn row(record: &Value, inspect: bool) -> Value {
    let live = worker_live(record) && record["state"] == "running";
    let auth = record["auth_required"] == true;
    let state = string(record, "state");
    let has_conversation = session_id::canonical(string(record, "conversation_id"));
    let exited = original_exited(record);
    let can_resume = has_conversation && state != "starting" && exited;
    let ready = live && record["phase"] == "idle" && !auth;
    let busy = live && record["phase"] == "working";
    let phase = if auth {
        "input"
    } else if busy {
        "working"
    } else if ready {
        "idle"
    } else {
        "unknown"
    };
    let parts: Vec<_> = string(record, "name").splitn(3, '/').collect();
    let mut row = json!({
        "name":record["name"],"cmd":"agy","agent":"agy","backend":"agy","account_id":ACCOUNT,
        "session_id":record["session_id"],"run_id":record["run_id"],"launch_id":record["launch_id"],
        "conversation_id":record["conversation_id"],"cwd":record["cwd"],"cwd_source":"native_session",
        "project":parts.get(1),"tag":parts.get(2),"created":record["created"],"last_event_at":record["last_event_at"],
        "state":if live {"running"} else if state=="paused" {"paused"} else if exited {"stopped"} else {"unknown"},
        "process_state":if live {"running"} else if exited {"exited"} else {"unverified"},"runtime_state":if live {"live"} else if exited {"stopped"} else {"unknown"},
        "tracked":true,"resumable":can_resume || live && has_conversation,"activity_supported":true,
        "auth_required":auth,"activity":if auth {"attention"} else if busy {"busy"} else if ready {"idle"} else {"unknown"},
        "phase":phase,
    });
    let controls = json!({
        "activity_summary":if auth {"Sign in required"} else if busy {"Working"} else if ready {"Ready"} else if exited {"Stopped"} else {"Process state unverified"},
        "activity_detail":record["error"],"turn_started":record["turn_started"],"interrupt_supported":busy,
        "reply_id":record["reply_id"],"reply_at":record["reply_at"],
        "send_supported":live && !auth,"send_available":ready,"terminal_supported":false,
        "terminal_attach_supported":false,"native_ui_available":false,"attachments_supported":false,
        "bulk_lifecycle_supported":false,"settings_change_supported":false,"clear_context_supported":false,
        "compact_context_supported":false,"fork_supported":false,
        "allowed_actions":if can_resume {vec!["resume"]} else if ready {vec!["pause"]} else {Vec::new()},
        "action_reasons":{"archive":"Antigravity archives are unavailable","terminate":"Use Stop session to preserve the native conversation"}
    });
    row.as_object_mut()
        .expect("Antigravity summary")
        .extend(controls.as_object().expect("Antigravity controls").clone());
    if inspect {
        row["events"] = record["agy_events"].clone();
        row["cursor"] = record["agy_seq"].clone();
        row["replace_events"] = json!(true);
    }
    row
}

fn inspect_options(args: &[String]) -> Result<()> {
    let mut options = args.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--skip-processes" | "--current-read" | "--read-observation" => {}
            "--after"
                if options
                    .next()
                    .is_some_and(|value| value.parse::<u64>().is_ok()) => {}
            _ => return Err("Unsupported Antigravity inspection option".into()),
        }
    }
    Ok(())
}

pub(super) fn dispatch(command: &str, name: &str, args: &[String]) -> Result<i32> {
    match command {
        "inspect" => {
            inspect_options(args)?;
            println!("{}", inspect(name)?);
            Ok(0)
        }
        "send" => dispatch_send(name, args),
        "exists" if args.is_empty() => Ok(if binding(name).is_ok() { 0 } else { 1 }),
        _ => Err(
            "This operation is unavailable for Antigravity; use its guarded session controls"
                .into(),
        ),
    }
}
