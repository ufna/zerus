//! The caller proof of a hosted DeepSeek tool call (zerus-ppd26 Part B). One
//! native host process serves every DeepSeek session of its agent home, so no
//! hosted session has an agent process of its own: process ancestry reaches
//! only the host. Each tool call of a host this hgs started carries its own
//! session's markers and a fresh per-call token the host keeps while that
//! call (or a job it started) is live; this proof ties the two together.
#[cfg(unix)]
use super::caller_hosts;
use super::*;
#[cfg(unix)]
use crate::state::provider_binding::Provider;

/// What this proof establishes and what it does not, stated with its result.
pub(crate) const HOSTED_LIMIT: &str = "One DeepSeek host process serves every session of its agent home: process ancestry proves only that this command runs under the host that serves the session, and the host's per-call token proves which conversation's tool call started it. A same-user process that reads another tool call's environment can copy that call's token; that is deliberate instrumentation and outside this proof.";

/// The longest the proof waits for the host to answer for a token.
#[cfg(unix)]
const TOKEN_WAIT: Duration = Duration::from_secs(2);

#[cfg(unix)]
const NOT_HOSTED: &str = "The markers name no DeepSeek session.";
#[cfg(unix)]
const NO_TOKEN: &str = "This command carries no DeepSeek per-call token; only a tool call of a host started by this hgs version can be proven.";
#[cfg(unix)]
const BINDING: &str = "Cannot verify the DeepSeek session binding the markers name.";
#[cfg(unix)]
const OTHER_HOME: &str = "This command's DeepSeek home is not the session's agent home.";
#[cfg(unix)]
const NO_HOST: &str = "No live DeepSeek host is recorded for the session's agent home.";
#[cfg(unix)]
const NOT_UNDER_HOST: &str =
    "This command does not run under the DeepSeek host that serves the session.";
#[cfg(unix)]
const ANCESTRY: &str = "Cannot read this command's process ancestry up to its DeepSeek host.";
#[cfg(unix)]
const OTHER_CONVERSATION: &str =
    "The DeepSeek host holds this tool call's token for another conversation.";
#[cfg(unix)]
const HOST_CHANGED: &str = "The DeepSeek host changed while this tool call was being proven.";

/// Proves that this process is a tool call of the hosted DeepSeek session
/// its markers name and returns that session's binding, in the shape
/// `caller_record_for` returns: `Ok(Some(binding))` when proven, `Ok(None)`
/// for an archived binding and `Err(reason)` for every refusal. The binding
/// carries `caller_proof` (`kind` `hosted_call`, the host's `host_pid` and
/// [`HOSTED_LIMIT`]).
///
/// 1. The claim is `name`, `run` and `session` (`HGS_SESSION`, `HGS_RUN_ID`,
///    `HGS_SESSION_ID`), with this process's own `HGS_DSH_CALL_TOKEN` (64
///    lowercase hex) and `DSH_HOME`. A name outside `dsh/` is not hosted.
/// 2. The binding is read by its exact route: a claimed session ID through
///    the session index (its ID layout record or its label, as `hgs self`
///    reads a claim), else the label. It must be a DeepSeek binding whose
///    name or launch label is `name`, whose run is `run` and, when claimed,
///    whose session ID is `session`.
/// 3. The binding's agent home selects the one host folder that `ensure_host`
///    and every host request use. Its record is read through the Teams
///    proof's own reader (`caller_hosts`, this boot's records only), and only
///    the processes it names with a recorded birth count.
/// 4. One of those processes, with the same birth, is an ancestor of this
///    process: at most 64 steps, and parent 0, a loop or an unreadable step
///    refuses.
/// 5. This process's `DSH_HOME` is the binding's agent home (the native host
///    sets it on every tool call).
/// 6. The host, asked over its private socket within 2 s, knows the token as
///    a live tool call of the binding's conversation, the top-level one for a
///    child agent. A host whose adapter does not advertise call tokens, no
///    answer, an unknown token or another conversation refuses. The host's
///    birth is read again afterwards.
///
/// The limit is [`HOSTED_LIMIT`]: the token shows which conversation's tool
/// call started this command, not that no other same-user process read it.
#[cfg(unix)]
pub(crate) fn hosted_caller(
    name: &str,
    run: &str,
    session: Option<&str>,
) -> std::result::Result<Option<Value>, &'static str> {
    // 1. The claim.
    if !matches!(Provider::named(name), Some(Provider::DeepSeek)) {
        return Err(NOT_HOSTED);
    }
    let token = std::env::var("HGS_DSH_CALL_TOKEN")
        .ok()
        .filter(|token| {
            token.len() == 64
                && token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or(NO_TOKEN)?;
    let home = std::env::var("DSH_HOME").map_err(|_| OTHER_HOME)?;
    if run.is_empty() || session.is_some_and(str::is_empty) {
        return Err(BINDING);
    }
    // 2. The binding by its exact route: a claimed ID through the session
    // index (its ID layout record or its label), else the label.
    let record = match session {
        Some(id) => crate::state::parent_link::read_session(id, name)
            .map_err(|_| BINDING)?
            .ok_or(BINDING)?,
        None => Provider::DeepSeek.read_label(name).map_err(|_| BINDING)?,
    };
    if record["backend"] != "dsh"
        || (record["name"] != name && record["launch_label"] != name)
        || record["run_id"] != run
        || session.is_some_and(|id| record["session_id"] != id)
    {
        return Err(BINDING);
    }
    if record["state"] == "archived" {
        return Ok(None);
    }
    let agent_home = string(&record, "agent_home");
    let conversation = string(&record, "conversation_id");
    if agent_home.is_empty() || conversation.is_empty() {
        return Err(BINDING);
    }
    // 5. The native host's own home for this call.
    if home != agent_home {
        return Err(OTHER_HOME);
    }
    // 3. The host record of that agent home, through the Teams reader.
    let named: Vec<_> = caller_hosts::deepseek_home(agent_home)?
        .into_iter()
        .filter_map(|host| host.birth.map(|birth| (host.pid, birth)))
        .collect();
    if named.is_empty() {
        return Err(NO_HOST);
    }
    // 4. That host, with its recorded birth, is an ancestor.
    let mut pid = std::process::id();
    let (host, birth) = 'walk: {
        for _ in 0..64 {
            let identity = crate::platform::process_identity(pid).ok_or(ANCESTRY)?;
            if let Some(found) = named
                .iter()
                .find(|(host, birth)| *host == pid && *birth == identity.birth)
            {
                break 'walk found.clone();
            }
            if identity.parent == 0 {
                return Err(NOT_UNDER_HOST);
            }
            if identity.parent == pid {
                return Err(ANCESTRY);
            }
            pid = identity.parent;
        }
        return Err(ANCESTRY);
    };
    // 6. The host's per-call token names this conversation.
    if crate::state::dsh::call_token_conversation(agent_home, &token, TOKEN_WAIT)? != conversation {
        return Err(OTHER_CONVERSATION);
    }
    if crate::platform::process_identity(host).is_none_or(|identity| identity.birth != birth) {
        return Err(HOST_CHANGED);
    }
    let mut record = record;
    record["caller_proof"] = json!({"kind":"hosted_call","host_pid":host,"limit":HOSTED_LIMIT});
    Ok(Some(record))
}

/// DeepSeek hosts run only on Linux and macOS; nothing else proves one.
#[cfg(not(unix))]
pub(crate) fn hosted_caller(
    _name: &str,
    _run: &str,
    _session: Option<&str>,
) -> std::result::Result<Option<Value>, &'static str> {
    Err("DeepSeek tool calls are proven only on Linux and macOS.")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{Arc, Mutex};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const CONVERSATION: &str = "session-91250d76-ca68-48b6-87f8-3a0e4400c314";
    const NAME: &str = "dsh/project/hosted";

    /// Runs one fixture alone in a child test process with a private state
    /// root, native home and call token, so no environment change reaches
    /// another test.
    fn isolated(fixture: &str) {
        let state = tempfile::tempdir().unwrap();
        let root = state.path().join("s");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("state::session_graph::sends::caller_hosted::tests::{fixture}"),
                "--ignored",
                "--test-threads=1",
            ])
            .env("HGS_STATE_DIR", &root)
            .env("HGS_CONFIG_DIR", state.path().join("config"))
            .env("DSH_HOME", root.join("native"))
            .env("HGS_DSH_CALL_TOKEN", TOKEN)
            .env_remove("HGS_SESSION")
            .env_remove("HGS_SESSION_ID")
            .env_remove("HGS_RUN_ID")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && text.contains("1 passed"),
            "{text}{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn a_hosted_call_is_its_session_only_under_its_host_with_its_conversations_token() {
        isolated("proof_fixture");
    }

    #[test]
    fn a_command_without_a_per_call_token_is_never_a_hosted_session() {
        isolated("token_fixture");
    }

    /// A stand-in host socket: `mode` chooses the answer to the next request.
    fn host(socket: PathBuf, mode: Arc<Mutex<(&'static str, String)>>) {
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap_or_default();
                let (mode, conversation) = mode.lock().unwrap().clone();
                let value = match (mode, request["method"].as_str()) {
                    ("silent", _) => {
                        std::thread::sleep(Duration::from_secs(4));
                        continue;
                    }
                    ("older", Some("ping")) => json!({"protocol":1,"version":"0.2.1-rc.2"}),
                    (_, Some("ping")) => {
                        json!({"protocol":1,"version":"0.2.1-rc.2","callTokens":true})
                    }
                    ("unknown", Some("call-token")) => json!({"known":false}),
                    (_, Some("call-token")) if request["params"]["token"] == TOKEN => {
                        json!({"known":true,"conversation":conversation})
                    }
                    (_, Some("call-token")) => json!({"known":false}),
                    _ => {
                        let _ = writeln!(
                            stream,
                            "{}",
                            json!({"ok":false,"error":"Unsupported HGS native request"})
                        );
                        continue;
                    }
                };
                let _ = writeln!(stream, "{}", json!({"ok":true,"value":value}));
            }
        });
    }

    fn running(dir: &Path, pid: u32, birth: &str, boot: Option<&str>) {
        let mut record =
            json!({"state":"running","version":2,"pid":pid,"birth":birth,"pid_file":[0,0,0,0]});
        if let Some(boot) = boot {
            record["boot"] = json!(boot);
        }
        fs::write(dir.join("host.identity.json"), record.to_string()).unwrap();
    }

    #[test]
    #[ignore = "run by its isolated wrapper"]
    fn proof_fixture() {
        let home = std::env::var("DSH_HOME").unwrap();
        let mut binding = json!({"backend":"dsh","agent":"dsh","name":NAME,"agent_home":home,
            "conversation_id":CONVERSATION,"run_id":"run-1","created":1.0,"paused":false});
        Provider::DeepSeek.save(&mut binding).unwrap();
        let id = string(&binding, "session_id").to_owned();
        assert!(!id.is_empty());
        let dir = crate::state::storage::root()
            .join("dsh/hosts")
            .join(crate::state::dsh::host_key(&home));
        crate::platform::private_dir(&dir).unwrap();
        let mode = Arc::new(Mutex::new(("answer", CONVERSATION.to_owned())));
        host(dir.join("host.sock"), mode.clone());
        let set = |value: &'static str, conversation: &str| {
            *mode.lock().unwrap() = (value, conversation.to_owned())
        };
        let me = std::process::id();
        let birth = crate::platform::process_identity(me).unwrap().birth;
        let boot = caller_hosts::written_boot();
        running(&dir, me, &birth, boot.as_deref());

        // Through the export the Teams proof calls.
        let proven = crate::state::session_graph::hosted_caller(NAME, "run-1", Some(&id))
            .unwrap()
            .unwrap();
        assert_eq!(proven["session_id"], id.as_str());
        assert_eq!(
            proven["caller_proof"],
            json!({"kind":"hosted_call","host_pid":me,"limit":HOSTED_LIMIT})
        );
        assert!(
            hosted_caller(NAME, "run-1", None).unwrap().is_some(),
            "the label route"
        );

        // The claim must be the binding's own.
        assert_eq!(hosted_caller(NAME, "run-2", Some(&id)), Err(BINDING));
        assert_eq!(
            hosted_caller("dsh/project/other", "run-1", Some(&id)),
            Err(BINDING)
        );
        assert_eq!(
            hosted_caller(NAME, "run-1", Some("6a4c5887-557a-4f31-b1c8-ad0a94c3c705")),
            Err(BINDING)
        );
        assert_eq!(
            hosted_caller("claude/project/hosted", "run-1", Some(&id)),
            Err(NOT_HOSTED)
        );

        // The host must hold this token for this conversation, and say so in time.
        set("unknown", CONVERSATION);
        assert!(hosted_caller(NAME, "run-1", Some(&id))
            .unwrap_err()
            .contains("knows no live tool call"));
        set("answer", "session-00000000-0000-4000-8000-000000000001");
        assert_eq!(
            hosted_caller(NAME, "run-1", Some(&id)),
            Err(OTHER_CONVERSATION)
        );
        set("older", CONVERSATION);
        assert!(hosted_caller(NAME, "run-1", Some(&id))
            .unwrap_err()
            .contains("predates per-call tokens"));
        set("silent", CONVERSATION);
        let started = std::time::Instant::now();
        assert!(hosted_caller(NAME, "run-1", Some(&id))
            .unwrap_err()
            .contains("did not confirm"));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        set("answer", CONVERSATION);
        assert!(hosted_caller(NAME, "run-1", Some(&id)).unwrap().is_some());

        // The recorded host, with its birth, this boot, must be an ancestor.
        // Above the host the walk refuses either at parent 0 or at the first
        // ancestor it cannot read; both are refusals.
        let refused = |result: std::result::Result<Option<Value>, &'static str>| {
            assert!(
                matches!(result, Err(NOT_UNDER_HOST) | Err(ANCESTRY)),
                "{result:?}"
            )
        };
        running(&dir, me, "1", boot.as_deref());
        refused(hosted_caller(NAME, "run-1", Some(&id)));
        running(
            &dir,
            me,
            &birth,
            Some("00000000-0000-4000-8000-000000000002"),
        );
        assert_eq!(hosted_caller(NAME, "run-1", Some(&id)), Err(NO_HOST));
        let starting = json!({"state":"starting","version":2,"attempt":"00000000-0000-4000-8000-000000000003",
            "boot":boot,"launcher":{"pid":me,"birth":birth},"phase":"spawning","pid_file":[me,[0,0,0,0]],"previous":null});
        fs::write(dir.join("host.identity.json"), starting.to_string()).unwrap();
        assert_eq!(hosted_caller(NAME, "run-1", Some(&id)), Err(NO_HOST));
        let mut other = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let other_birth = crate::platform::process_identity(other.id()).unwrap().birth;
        running(&dir, other.id(), &other_birth, boot.as_deref());
        refused(hosted_caller(NAME, "run-1", Some(&id)));
        let _ = other.kill();
        let _ = other.wait();
        running(&dir, me, &birth, boot.as_deref());
        assert!(hosted_caller(NAME, "run-1", Some(&id)).unwrap().is_some());

        // The native home of this call must be the session's.
        std::env::set_var("DSH_HOME", format!("{home}-other"));
        assert_eq!(hosted_caller(NAME, "run-1", Some(&id)), Err(OTHER_HOME));
        std::env::set_var("DSH_HOME", &home);

        // An archived binding is established and not live.
        binding["state"] = json!("archived");
        Provider::DeepSeek.save(&mut binding).unwrap();
        assert_eq!(hosted_caller(NAME, "run-1", Some(&id)), Ok(None));
    }

    #[test]
    #[ignore = "run by its isolated wrapper"]
    fn token_fixture() {
        for token in [
            None,
            Some(""),
            Some("XYZ"),
            Some(&TOKEN.to_uppercase()[..]),
            Some(&TOKEN[1..]),
        ] {
            match token {
                Some(value) => std::env::set_var("HGS_DSH_CALL_TOKEN", value),
                None => std::env::remove_var("HGS_DSH_CALL_TOKEN"),
            }
            assert_eq!(
                hosted_caller(NAME, "run-1", None),
                Err(NO_TOKEN),
                "{token:?}"
            );
        }
    }
}
