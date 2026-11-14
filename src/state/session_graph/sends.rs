//! Durable metadata for an observed Zerus send. The caller is proved against
//! its saved native process ancestry, never against an environment label alone.
//! This ledger cannot retry input and stores no prompt or attachment content.
use super::{model::Endpoint, *};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::io::{Read, Seek, Write};
use std::process::{Command, Stdio};

const MAX_INPUT: u64 = 29 * 1024 * 1024;
const MAX_RECEIPT: usize = 1024 * 1024;

#[path = "caller_binding.rs"]
mod caller_binding;
#[path = "caller_hosts.rs"]
mod caller_hosts;
#[path = "caller_hosted.rs"]
mod caller_hosted;
pub(crate) use caller_hosted::{hosted_caller, HOSTED_LIMIT};
#[cfg(all(test, unix))]
pub(crate) use caller_hosts::Named;
pub(crate) use caller_hosts::Registered;

/// OpenCode's refusal-only registration. DeepSeek ancestry uses the shared
/// state::dsh helper, independently of this service file.
pub(crate) fn caller_opencode_host() -> Option<Registered> {
    caller_hosts::opencode()
}

/// This boot's identity as the host's own writer reads it, for the tests'
/// host records.
#[cfg(all(test, unix))]
pub(crate) fn written_boot() -> Option<String> {
    caller_hosts::written_boot()
}

/// What one host folder names under `boot`, for the tests of the record rules.
#[cfg(all(test, unix))]
pub(crate) fn caller_host_folder(
    folder: &Path,
    boot: Option<&str>,
) -> std::result::Result<Vec<Named>, &'static str> {
    caller_hosts::named_in(folder, boot)
}

/// The processes this boot's record names as the DeepSeek host of
/// `agent_home`, each with its recorded birth: the very read the hosted
/// caller proof makes (zerus-63rru.28). A process named without a birth is
/// left out, as the proof leaves it out; no record names no host.
#[cfg(unix)]
pub(crate) fn caller_host_for_home(
    agent_home: &str,
) -> std::result::Result<Vec<(u32, String)>, &'static str> {
    Ok(caller_hosts::deepseek_home(agent_home)?
        .into_iter()
        .filter_map(|host| host.birth.map(|birth| (host.pid, birth)))
        .collect())
}

/// Whether a marked claim names a DeepSeek session, whose commands only the
/// hosted caller proof can bind to it: by the name's agent alone, before any
/// binding is read.
pub(crate) fn hosted_claim(name: &str) -> bool {
    matches!(
        crate::state::provider_binding::Provider::named(name),
        Some(crate::state::provider_binding::Provider::DeepSeek)
    )
}

/// The backend of a session a resident host serves, when a marked claim
/// names one by its exact route; `None` for every other claim.
pub(crate) fn caller_hosted_backend(
    name: &str,
    run: &str,
    session: Option<&str>,
) -> Option<String> {
    caller_hosts::hosted_backend(name, run, session)
}

#[cfg(test)]
pub(crate) fn caller_host_reads() -> usize {
    {
        #[cfg(unix)]
        { crate::state::dsh::recorded_host_reads() }
        #[cfg(not(unix))]
        { 0 }
    }
}

/// Explicit, bounded caller admission. Existing send attribution keeps its
/// compatibility behavior; Teams and task intake use this strict reader.
pub(crate) fn caller_record_for(
    name: &str,
    run: &str,
    session: Option<&str>,
) -> std::result::Result<Option<Value>, &'static str> {
    caller_binding::record(name, run, session, None)
}

pub(crate) fn caller_agent_for(
    name: &str,
    run: &str,
    session: Option<&str>,
    pid: u32,
    birth: &str,
) -> std::result::Result<Option<Value>, &'static str> {
    caller_binding::record(name, run, session, Some((pid, birth)))
}

pub(crate) fn caller_machine(
    config: &crate::config::Config,
) -> std::result::Result<String, &'static str> {
    caller_binding::machine(config)
}

#[cfg(test)]
pub(crate) fn caller_binding_reads() -> usize {
    caller_binding::reads()
}

fn machine() -> Option<String> {
    let config = crate::config::Config::load().ok()?;
    crate::swarm::local_node_id(&config).ok()
}

/// A verified native target is useful even when an older caller has no source
/// attribution. Do not add it to an old receipt on replay.
pub(crate) fn target_receipt(record: &Value) -> Option<Value> {
    target_receipt_for_machine(record, &machine()?)
}

/// The same target from an already verified, existing machine identity.
/// This factory never loads, initializes or saves a machine catalog.
pub(crate) fn target_receipt_for_machine(record: &Value, machine: &str) -> Option<Value> {
    let endpoint = Endpoint::from_record(machine, record)?;
    Some(json!({"schema":1,"node":endpoint.key(),"endpoint":endpoint,
        "name":record["name"],"run_id":record["run_id"]}))
}

pub(crate) fn send_origin() -> Option<Value> {
    target_receipt(&owned_record()?)
}

/// The live binding whose native agent is an actual ancestor of this CLI, or
/// whose agent window holds the Claude background job this CLI runs under.
/// Its name may be a rename shadow's: hooks and agents keep the environment
/// they started with.
fn owned_record() -> Option<Value> {
    token_record().or_else(super::super::continuations::background_session)
}

fn token_record() -> Option<Value> {
    let name = nonempty_env("HGS_SESSION")?;
    let run = nonempty_env("HGS_RUN_ID")?;
    // A hosted DeepSeek tool call has no agent process of its own; its host's
    // per-call token proves the conversation (zerus-ppd26 Part B).
    if matches!(
        crate::state::provider_binding::Provider::named(&name),
        Some(crate::state::provider_binding::Provider::DeepSeek)
    ) {
        return hosted_caller(&name, &run, nonempty_env("HGS_SESSION_ID").as_deref())
            .ok()
            .flatten();
    }
    let record = read_run(&name, &run).ok()??;
    if !process_alive(&record) || record["state"] == "archived" {
        return None;
    }
    // Ancestry through a recorded DeepSeek host proves no other agent's
    // session (zerus-ppd26.15).
    if crate::state::dsh::host_check() != crate::state::dsh::HostCheck::Outside {
        return None;
    }
    let owner = u32::try_from(record["pid"].as_u64()?).ok()?;
    let mut pid = std::process::id();
    for _ in 0..64 {
        let process = crate::platform::process_identity(pid)?;
        if pid == owner {
            return Some(record);
        }
        if process.parent == 0 || process.parent == pid {
            break;
        }
        pid = process.parent;
    }
    None
}

/// The `HGS_AGENT` value a resident DeepSeek host's bridge sets on the
/// processes of every tool call, whether or not it also has that session's
/// own markers (zerus-ppd26).
pub(crate) const HOSTED_AGENT: &str = "dsh";

/// Whether this command runs in a resident host's tool call. Such a command
/// is an agent's even when no session can be proven for it (no identity file
/// yet, an agent the native UI opened, or a host this hgs cannot prove), so a
/// guard that would otherwise take an unmarked caller for the person refuses
/// it. Like every marker this guards against accidental use, not against an
/// agent that clears its own environment.
pub(crate) fn hosted_agent_call() -> bool {
    std::env::var_os("HGS_AGENT").is_some_and(|agent| agent == HOSTED_AGENT)
}

/// Why a resident DeepSeek host's tool call (`HGS_AGENT=dsh`) cannot act as
/// its session, or `None` when this is no such call or its hosted proof holds
/// (zerus-ppd26 Part B). Such a call is never the person: a path that would
/// otherwise go on with no origin (a send without a sender, a launch without
/// a parent) refuses it with this sentence instead. A call that scrubs every
/// marker is outside this rule, as for every agent.
pub(crate) fn hosted_proof_refusal() -> Option<String> {
    if !hosted_agent_call() {
        return None;
    }
    let (Some(name), Some(run)) = (nonempty_env("HGS_SESSION"), nonempty_env("HGS_RUN_ID")) else {
        return Some("It carries no session markers.".into());
    };
    match hosted_caller(&name, &run, nonempty_env("HGS_SESSION_ID").as_deref()) {
        Ok(Some(record)) if target_receipt(&record).is_some() => None,
        Ok(Some(_)) => Some("Its session cannot be named on this computer.".into()),
        Ok(None) => Some("Its session is archived.".into()),
        Err(reason) => Some(reason.to_owned()),
    }
}

/// The calling agent's own binding under its current name, proven like a
/// send's origin.
pub(crate) fn caller_record() -> Option<Value> {
    let record = owned_record()?;
    if record["rename_shadow"] != true {
        return Some(record);
    }
    read_run(string(&record, "rename_target"), string(&record, "run_id"))
        .ok()?
        .filter(|current| current["rename_shadow"] != true)
}

fn path() -> PathBuf {
    root().join("graph/sends.sqlite3")
}

pub(super) fn open(read_only: bool) -> anyhow::Result<Connection> {
    let path = path();
    if let Ok(meta) = path.symlink_metadata() {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Nonregular graph send ledger"
        );
    }
    if !read_only {
        private_dir(path.parent().unwrap()).map_err(anyhow::Error::msg)?;
        if !path.exists() {
            crate::platform::private_open(fs::OpenOptions::new().write(true).create_new(true))
                .open(&path)?;
        }
    }
    let db = Connection::open_with_flags(
        &path,
        if read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        },
    )?;
    db.busy_timeout(Duration::from_millis(100))?;
    if !read_only {
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA max_page_count=8192;
            CREATE TABLE IF NOT EXISTS sends(id TEXT PRIMARY KEY,at REAL NOT NULL,payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS sends_at ON sends(at,id);")?;
    }
    Ok(db)
}

fn reserve(
    origin: &Value,
    route: &str,
    name: &str,
    input: &[u8],
) -> anyhow::Result<(String, Value)> {
    let request: Value = serde_json::from_slice(input)?;
    let request_id = string(&request, "request_id");
    ensure!(
        uuid::Uuid::parse_str(request_id)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == request_id),
        "Invalid send request ID"
    );
    ensure!(
        name.len() <= 512 && !name.chars().any(char::is_control) && route.len() <= 256,
        "Invalid graph send scope"
    );
    let digest = format!("{:x}", Sha256::digest(input));
    let id = model::id("event", &json!([origin["node"], request_id]));
    let event = json!({"schema":1,"id":id,"source":origin,"route":route,"name":name,
        "request_id":request_id,"expected_run_id":request["expected_run_id"],
        "expected_conversation_id":request["expected_conversation_id"],
        "digest":digest,"at":now(),"status":"unknown"});
    let db = open(false)?;
    db.execute(
        "DELETE FROM sends WHERE at<?1",
        [now() - 30.0 * 24.0 * 3600.0],
    )?;
    if let Some(prior) = db
        .query_row("SELECT payload FROM sends WHERE id=?1", [&id], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        let prior: Value = serde_json::from_str(&prior)?;
        ensure!(
            prior["digest"] == digest && prior["source"] == *origin,
            "Graph send identity changed"
        );
        return Ok((id, prior));
    }
    db.execute(
        "INSERT INTO sends VALUES(?1,?2,?3)",
        params![id, now(), event.to_string()],
    )?;
    Ok((id, event))
}

fn confirm(id: &str, mut event: Value, bytes: &[u8]) -> anyhow::Result<()> {
    let receipt: Value = serde_json::from_slice(bytes)?;
    let target = &receipt["graph_target"];
    let endpoint: Endpoint = serde_json::from_value(target["endpoint"].clone())?;
    ensure!(
        endpoint.valid() && target["schema"] == 1 && target["node"] == endpoint.key(),
        "Unverified graph target"
    );
    ensure!(
        receipt["status"] == "submitted"
            && receipt["request_id"] == event["request_id"]
            && receipt["name"] == event["name"]
            && receipt["run_id"] == event["expected_run_id"]
            && receipt["conversation_id"] == event["expected_conversation_id"]
            && target["name"] == receipt["name"]
            && target["run_id"] == receipt["run_id"]
            && endpoint.conversation == string(&receipt, "conversation_id")
            && (endpoint.provisional_run.is_empty()
                || endpoint.provisional_run == string(&receipt, "run_id"))
            && string(&receipt, "agent_id").is_empty(),
        "Graph delivery receipt did not match"
    );
    event["target"] = target.clone();
    event["status"] = json!("submitted");
    // submitted_at is supplied by terminal/native receipt, not guessed from
    // inter-host clock ordering. The local observation time remains separate.
    event["submitted_at"] = receipt["submitted_at"].clone();
    open(false)?.execute(
        "UPDATE sends SET payload=?2,at=max(at+0.001,?3) WHERE id=?1",
        params![id, event.to_string(), now()],
    )?;
    Ok(())
}

/// Deliver in a child without attributing the same send twice, preserving
/// every native delivery guard and exact exit status. Only metadata capture is bounded;
/// stdout is streamed byte-for-byte, and errors retain the normal stderr route.
pub(crate) fn forward_send(route: &str, args: &[String], origin: Value) -> crate::cli::Result<i32> {
    let executable = crate::self_exec::current()?;
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT + 1)
        .read_to_end(&mut input)?;
    if input.len() as u64 > MAX_INPUT {
        return Err(crate::cli::Error::new(1, "message payload exceeds 29 MiB"));
    }
    // The ledger digest and the receiving send's request identity both cover
    // the exact text delivered.
    let input = enveloped(input);
    let reservation = match reserve(&origin, route, &args[0], &input) {
        Ok(value) => Some(value),
        Err(_) => {
            let marker = root().join("graph/recording-gap.json");
            if !marker.exists() {
                let _ = atomic(
                    &marker,
                    &json!({"since":now(),"reason":"metadata_recording_unavailable"}).to_string(),
                );
            }
            eprintln!(
                "hgs: Graph attribution could not be recorded; ordinary send guards remain active"
            );
            None
        }
    };
    let mut file = tempfile::tempfile()?;
    file.write_all(&input)?;
    file.rewind()?;
    let mut command = Command::new(executable);
    if !route.is_empty() {
        command.arg(format!("@{route}"));
    }
    // This endpoint came from send_origin's native caller proof, never from
    // stdin. A hosted caller's child remains under its host: clearing that
    // proof would make main's found-host gate correctly refuse the child.
    // Keep the call context and use the existing native state entry so this
    // already-enveloped send cannot enter attribution recursively. Remote
    // routing still rechecks the live hosted proof before its SSH hop.
    let hosted = origin["endpoint"]["provider"] == "dsh";
    if hosted {
        command.arg("__state");
    } else {
        command
            .env_remove("HGS_SESSION")
            .env_remove("HGS_SESSION_ID")
            .env_remove("HGS_RUN_ID")
            .env_remove("HGS_AGENT")
            .env_remove("HGS_DSH_CALL_TOKEN");
    }
    command
        .arg("send")
        .args(args)
        .stdin(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = crate::platform::spawn(&mut command)?;
    let mut pipe = child.stdout.take().unwrap();
    let mut output = std::io::stdout().lock();
    let mut receipt = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = pipe.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        output.write_all(&chunk[..n])?;
        if receipt.len() <= MAX_RECEIPT {
            receipt.extend_from_slice(&chunk[..n]);
        }
    }
    output.flush()?;
    let status = child.wait()?;
    if status.success() && receipt.len() <= MAX_RECEIPT {
        if let Some((id, event)) = reservation {
            let _ = confirm(&id, event, &receipt);
        }
    }
    Ok(status.code().unwrap_or(1))
}

/// `input` with its text inside the calling agent's sender envelope. A payload
/// without text, or a caller whose reference cannot be proven, stays as it is.
fn enveloped(input: Vec<u8>) -> Vec<u8> {
    let Ok(mut request) = serde_json::from_slice::<Value>(&input) else {
        return input;
    };
    let Some(text) = request.get("text").and_then(Value::as_str) else {
        return input;
    };
    let Some(text) = crate::session_reference::sender_envelope(text) else {
        return input;
    };
    request["text"] = json!(text);
    serde_json::to_vec(&request).unwrap_or(input)
}

pub(super) fn source_ids(root: &Path) -> anyhow::Result<Vec<(String, String)>> {
    if !root.join("graph/sends.sqlite3").exists() {
        return Ok(Vec::new());
    }
    let db = open(true)?;
    let mut q = db.prepare("SELECT id,at FROM sends WHERE at>=?1 ORDER BY id LIMIT 10001")?;
    let rows = q
        .query_map([now() - 30.0 * 24.0 * 3600.0], |r| {
            Ok((r.get(0)?, format!("send-v1:{:.9}", r.get::<_, f64>(1)?)))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub(super) fn load(id: &str) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&open(true)?.query_row(
        "SELECT payload FROM sends WHERE id=?1",
        [id],
        |r| r.get::<_, String>(0),
    )?)?)
}

pub(super) fn coverage() -> anyhow::Result<Value> {
    let mut value = json!({"unknown_deliveries":0,"recording_gap":root().join("graph/recording-gap.json").exists()});
    if path().exists() {
        value["unknown_deliveries"]=json!(open(true)?.query_row("SELECT count(*) FROM sends WHERE at>=?1 AND json_extract(payload,'$.status')<>'submitted'",[now()-30.0*24.0*3600.0],|r|r.get::<_,i64>(0))?);
    }
    Ok(value)
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    #[test]
    fn existing_machine_factory_and_initializing_wrapper_keep_identical_shapes() {
        let home=tempfile::tempdir().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .env_clear().env("PATH",std::env::var_os("PATH").unwrap_or_default())
            .env("HOME",home.path()).env("HGS_CONFIG_DIR",home.path().join("config"))
            .env("HGS_STATE_DIR",home.path().join("state"))
            .args(["--exact","state::session_graph::sends::receipt_tests::private_factory_fixture","--ignored","--test-threads=1"])
            .stdin(std::process::Stdio::null()).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stdout));
    }
    #[test]
    #[ignore="private machine-initialization compatibility fixture"]
    fn private_factory_fixture() {
        let config=crate::config::Config::load().unwrap();
        let catalog=config.dir.join("swarm/catalog.json");
        let machine=uuid::Uuid::new_v4().to_string();
        let record=json!({"agent":"codex","run_id":"run","conversation_id":"cid","name":"codex/example/receipt"});
        let value=target_receipt_for_machine(&record,&machine).unwrap();
        let endpoint=Endpoint::from_record(&machine,&record).unwrap();
        assert_eq!(value,json!({"schema":1,"node":endpoint.key(),"endpoint":endpoint,"name":record["name"],"run_id":record["run_id"]}));
        assert!(!catalog.exists() && !root().join("graph").exists(),"read-only factory created state");
        let old=target_receipt(&record).unwrap();
        assert!(catalog.exists(),"ordinary wrapper stopped initializing machine identity");
        let bytes=fs::read(&catalog).unwrap();
        let persisted=crate::swarm::existing_node_id(&bytes).unwrap();
        assert_eq!(old,target_receipt_for_machine(&record,&persisted).unwrap());
        for agent in ["claude","codex","kimi","dsh","opencode"] {
            for cid in [Value::Null,json!(""),json!("cid")] {
                let mut record=record.clone();record["agent"]=json!(agent);record["conversation_id"]=cid;
                assert_eq!(target_receipt(&record),target_receipt_for_machine(&record,&persisted));
            }
        }
        assert_eq!(fs::read(catalog).unwrap(),bytes,"factory or stable wrapper changed the catalog");
        assert!(!root().join("graph").exists());
    }
}
