//! Read one original send outcome. Names are original receipt data, never a
//! mutable route to a live session or a reason to initialize a provider.
use super::*;
use std::io::Read;

pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    let [operation, name, format] = args else {
        return Err("usage: hgs delivery receipt <original-session-name> --json".into());
    };
    if format == "--json" && operation == "outbox-status" && name.starts_with("dsh/") {
        return dsh::outbox_status(name);
    }
    if format == "--json" && operation == "outbox-status" && name.starts_with("opencode/") {
        return opencode::outbox_status(name);
    }
    if !matches!(operation.as_str(), "receipt" | "outbox-status") || format != "--json" {
        return Err("usage: hgs delivery receipt <original-session-name> --json".into());
    }
    let mut bytes = Vec::new();
    io::stdin()
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "Invalid delivery receipt query")?;
    if bytes.len() > 8192 {
        return Err("Delivery receipt query exceeds 8 KiB".into());
    }
    if operation == "outbox-status" {
        let state = root();
        let status = if name.starts_with("claude/") {
            receipt_index::claude_outbox_status(&state, name, &bytes)
                .or_else(|_| receipt_index::terminal_outbox_status(&state, name, &bytes))?
        } else {
            receipt_index::terminal_outbox_status(&state, name, &bytes)?
        };
        println!("{status}");
        return Ok(0);
    }
    let query: receipt_index::DeliveryQuery =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid delivery receipt query")?;
    query.validate(name)?;
    let config =
        crate::config::Config::load().map_err(|_| "Delivery receipt computer is unavailable")?;
    let machine = session_action::status_machine(&config)
        .map_err(|_| "Delivery receipt computer identity is unavailable")?;
    if machine != query.machine() {
        return Err("Delivery receipt computer changed".into());
    }
    println!("{}", receipt_index::delivery_receipt(&root(), name, &query));
    Ok(0)
}

/// Closed local read only. The expected physical machine is checked before
/// opening any original receipt; the existing lookup still checks its digest.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn scoped_claude_outbox_status(args: &[String]) -> Result<i32> {
    let [name, format] = args else {
        return Err("Invalid Claude outbox status arguments".into());
    };
    if format != "--json" || !name.starts_with("claude/") {
        return Err("Invalid Claude outbox status arguments".into());
    }
    let mut bytes = Vec::new();
    io::stdin()
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "Invalid Claude outbox status query")?;
    if bytes.len() > 8192 {
        return Err("Claude outbox status query exceeds 8 KiB".into());
    }
    let config = crate::config::Config::load().map_err(|_| "Claude outbox computer is unavailable")?;
    println!("{}", read_claude_outbox_status(&config, name, &bytes)?);
    Ok(0)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_claude_outbox_status(config: &crate::config::Config, name: &str, bytes: &[u8]) -> Result<Value> {
    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct Query {
        #[serde(skip_serializing)]
        expected_machine_id: String,
        request_id: String,
        expected_run_id: String,
        expected_conversation_id: String,
        input_sha256: String,
    }
    let query: Query = serde_json::from_slice(bytes).map_err(|_| "Invalid Claude outbox status query")?;
    if !zerus_client_core::creation::canonical_uuid(&query.expected_machine_id) {
        return Err("Invalid Claude outbox machine identity".into());
    }
    if session_action::status_machine(config)? != query.expected_machine_id {
        return Err("Claude outbox computer changed".into());
    }
    let input = serde_json::to_vec(&query).map_err(|_| "Invalid Claude outbox status query")?;
    receipt_index::observe_claude_outbox(&config.state, name, &input)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn closed_claude_read_checks_machine_before_opening_the_original() {
        const OWNER: &str = "00000000-0000-4000-8000-000000000001";
        const OTHER: &str = "00000000-0000-4000-8000-000000000002";
        const ID: &str = "00000000-0000-4000-8000-000000000003";
        const RUN: &str = "00000000-0000-4000-8000-000000000004";
        const CONVERSATION: &str = "00000000-0000-4000-8000-000000000005";
        const NAME: &str = "claude/example/original";
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("sessions");
        let config = crate::config::Config {
            home: temp.path().to_string_lossy().into_owned(),
            dir: temp.path().to_owned(), state: state.clone(), selves: vec![],
            peers: vec![], base_peers: vec![], peers_from_environment: false,
            machines: Default::default(), tab: false, tab_colors: String::new(),
        };
        let swarm = temp.path().join("swarm");
        std::fs::create_dir(&swarm).unwrap();
        std::fs::set_permissions(&swarm, std::fs::Permissions::from_mode(0o700)).unwrap();
        let catalog = swarm.join("catalog.json");
        std::fs::write(&catalog, json!({"node_id":OWNER,"catalog":{
            "schema":1,"swarm_id":OTHER,"operations":{}
        }}).to_string()).unwrap();
        std::fs::set_permissions(&catalog, std::fs::Permissions::from_mode(0o600)).unwrap();
        let digest = "a".repeat(64);
        let mut query = json!({"expected_machine_id":OTHER,"request_id":ID,
            "expected_run_id":RUN,"expected_conversation_id":CONVERSATION,"input_sha256":digest});
        let read = |query: &Value| read_claude_outbox_status(&config, NAME, query.to_string().as_bytes());
        assert_eq!(read(&query).unwrap_err().to_string(), "Claude outbox computer changed");
        assert!(!state.exists());
        query["expected_machine_id"] = json!(OWNER);
        assert!(read(&query).is_err());
        assert!(!state.exists());
        let receipts = state.join("input_receipts");
        std::fs::create_dir_all(&receipts).unwrap();
        let original = receipts.join(format!("{ID}.json"));
        let bytes = json!({"request_id":ID,"name":NAME,"run_id":RUN,"conversation_id":CONVERSATION,
            "digest":digest,"status":"submitted","transport":"terminal","submitted_at":10.0,
            "claude_outbox":true,"native":"queue"}).to_string();
        std::fs::write(&original, &bytes).unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read(&query).unwrap()["proof"], "native_claude_receipt");
        let mut wrong_digest = query.clone();
        wrong_digest["input_sha256"] = json!("b".repeat(64));
        assert!(read(&wrong_digest).is_err());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), bytes);

        let mut refusal = json!({"refusal_schema":1,"request_id":ID,"name":NAME,"run_id":RUN,
            "conversation_id":CONVERSATION,"digest":digest,"status":"refused_before_input",
            "transport":"terminal","first_message":false,"reason":"session identity changed",
            "terminal_native_accept":true,"outbox_waiting":false});
        let bytes = refusal.to_string();
        std::fs::write(&original, &bytes).unwrap();
        let reply = read(&query).unwrap();
        assert_eq!(reply["proof"], "native_input_lock");
        assert_eq!(reply["status"], "not_offered");
        assert_eq!(reply["request_id"], ID);
        assert_eq!(reply["input_sha256"], digest);
        assert!(read(&wrong_digest).is_err());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), bytes);

        refusal["terminal_native_accept"] = json!(false);
        let bytes = refusal.to_string();
        std::fs::write(&original, &bytes).unwrap();
        assert!(read(&query).is_err());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), bytes);
    }
}

pub(super) fn bind(receipt: &mut Value, record: &Value, target: &Value) {
    if let Some(binding) = receipt_index::delivery_binding(record, target) {
        receipt["delivery_binding"] = binding;
    }
}
