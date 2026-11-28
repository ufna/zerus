//! Guarded offers from the desktop outbox and exact native file evidence.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Guard<'a> {
    pub(super) input_sha256: &'a str,
    pub(super) native_queue: bool,
}

pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    let (name, native_queue) = match args {
        [name, guard, format] if guard == "--when-idle" && format == "--json" => (name, false),
        [name, guard, queue, format]
            if guard == "--when-idle" && queue == "--opencode-queue" && format == "--json" => (name, true),
        _ => return Err("Use hgs send <OpenCode session> --when-idle [--opencode-queue] --json".into()),
    };
    let input = read_request_bytes()?;
    let request = parse_request(&input)?;
    const FIELDS: &[&str] = &[
        "request_id",
        "expected_run_id",
        "expected_conversation_id",
        "text",
        "attachments",
        "only_when_idle",
    ];
    if request["only_when_idle"] != true
        || !request["text"].is_string()
        || request
            .as_object()
            .is_none_or(|value| value.keys().any(|key| !FIELDS.contains(&key.as_str())))
    {
        return Err("Automatic Send requires the exact OpenCode idle guard".into());
    }
    let _bindings = try_bindings(Duration::from_secs(2))
        .ok_or("OpenCode session writer is busy; no message was sent")?;
    expect(
        name,
        string(&request, "expected_run_id"),
        string(&request, "expected_conversation_id"),
    )?;
    let record = binding(name)?;
    check_request_identity(&record, &request)?;
    if record["name"] != *name
        || string(&record, "run_id").is_empty()
        || !native_id(string(&record, "conversation_id"), "ses_")
    {
        return Err("The exact OpenCode session changed".into());
    }
    let input_sha256 = hash(&input);
    println!("{}", send(&record, &request, Some(Guard { input_sha256: &input_sha256, native_queue }))?);
    Ok(0)
}

/// Resolve one original offer. The desktop sends only its original
/// stdin digest and identity. This command never admits input.
pub(super) fn status(name: &str) -> Result<i32> {
    let mut input = Vec::new();
    io::stdin()
        .take(8193)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > 8192 {
        return Err("OpenCode outbox status request exceeds 8 KiB".into());
    }
    let request = parse_request(&input)?;
    let input_hash = string(&request, "input_sha256");
    if input_hash.len() != 64 || !input_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("OpenCode outbox status requires the original input digest".into());
    }
    let _bindings = try_bindings(Duration::from_millis(200)).ok_or(UNCERTAIN_STATE)?;
    let request_id = string(&request, "request_id");
    let ledger = Ledger::current();
    let mut receipt = ledger.sent(request_id).ok_or(UNCERTAIN_STATE)?;
    if receipt["request_id"] != request_id
        || receipt["name"] != name
        || receipt["transport"] != "native_opencode"
        || receipt["outbox_guarded"] != true
        || receipt["outbox_input_sha256"] != input_hash
    {
        return Err(UNCERTAIN_STATE.into());
    }
    // Use only the provider path the original admitted request selected.
    // A second holder of the label cannot shadow that fixed path.
    let record = match receipt["outbox_id_layout"].as_bool() {
        Some(true) => PROVIDER.read_id(string(&receipt, "outbox_session_id"))?,
        Some(false) => PROVIDER.read_label(name)?,
        None => return Err(UNCERTAIN_STATE.into()),
    };
    if record["name"] != name || receipt.get("outbox_session_id") != Some(&record["session_id"]) {
        return Err(UNCERTAIN_STATE.into());
    }
    check_request_identity(&record, &request)?;
    check_receipt_identity(&record, &receipt)?;
    if ledger
        .removal(&record, string(&receipt, "native_message_id"))?
        .is_some()
    {
        return Err(UNCERTAIN_STATE.into());
    }
    if receipt["status"] == "pending" {
        let deadline = Instant::now() + Duration::from_millis(1200);
        let service = service::connect_within(deadline).map_err(|_| UNCERTAIN_STATE)?;
        let native = Bounded {
            service: &service,
            deadline,
        };
        verify_native(&native, &record)?;
        settle(
            &native,
            &ledger,
            &record,
            &mut receipt,
            &ledger.receipt(request_id),
            Some(deadline),
        )?;
    }
    if receipt["status"] != "submitted" {
        return Err(UNCERTAIN_STATE.into());
    }
    println!(
        "{}",
        json!({
            "schema": 1, "status": "submitted", "proof": "native_opencode_receipt",
            "transport": receipt["transport"], "request_id": receipt["request_id"],
            "name": receipt["name"], "run_id": receipt["run_id"],
            "conversation_id": receipt["conversation_id"], "input_sha256": input_hash,
            "native_message_id": receipt["native_message_id"], "submitted_at": receipt["submitted_at"],
            "queued": receipt["queued"] == true,
        })
    );
    Ok(0)
}

/// Questions and permissions remain manual, including during native work.
pub(super) fn wait_receipt(
    native: &dyn Native,
    record: &Value,
    request: &Value,
    guard: Guard<'_>,
) -> Result<Option<Value>> {
    if waiting_for_user(native, record)?
        || !guard.native_queue && active_sessions(native)?
            .get(string(record, "conversation_id"))
            .is_some()
    {
        return Ok(Some(json!({
            "schema": 1, "status": "waiting_for_idle", "proof": "native_opencode_queue",
            "request_id": request["request_id"], "name": record["name"],
            "run_id": record["run_id"], "conversation_id": record["conversation_id"],
            "input_sha256": guard.input_sha256,
        })));
    }
    Ok(None)
}

pub(super) struct Files {
    pub(super) wire: Vec<Value>,
    pub(super) digests: Vec<String>,
}

impl Files {
    pub(super) fn from_request(request: &Value) -> Result<Self> {
        let files = if request["attachments"].is_null() {
            &[][..]
        } else {
            request["attachments"]
                .as_array()
                .ok_or("Invalid attachments")?
                .as_slice()
        };
        attachments::validate_references(
            &files
                .iter()
                .map(|file| string(file, "reference"))
                .collect::<Vec<_>>(),
        )?;
        let mut wire = Vec::new();
        let mut digests = Vec::new();
        for file in files {
            let name = string(file, "name");
            let mime = string(file, "mime");
            let media_type = |value: &str| {
                !value.is_empty()
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b))
            };
            if name.is_empty()
                || name.len() > 255
                || name.chars().any(char::is_control)
                || mime.len() > 100
                || !mime
                    .split_once('/')
                    .is_some_and(|(kind, subtype)| media_type(kind) && media_type(subtype))
            {
                return Err("Invalid OpenCode attachment metadata".into());
            }
            let data = string(file, "data_base64");
            digests.push(hash(data.as_bytes()));
            wire.push(json!({"name": name, "uri": format!("data:{mime};base64,{data}")}));
        }
        Ok(Self { wire, digests })
    }
}

fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// Remove large inline bodies before saving or budgeting the history cache.
/// The encoded body digest is used only to attribute the original send.
pub(super) fn strip_file_bodies(message: &mut Value) {
    if let Some(files) = message.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            if let Some(object) = file.as_object_mut() {
                object.remove("hgs_data_sha256");
                if let Some(data) = object.remove("data") {
                    if let Some(bytes) = data.as_str() {
                        object.insert("hgs_data_sha256".into(), json!(hash(bytes.as_bytes())));
                    }
                }
            }
        }
    }
}

/// The payload from prompt/inbox uses the same file objects as delivered
/// history. Only the local bounded history cache may substitute their hashes.
pub(super) fn matches_payload(payload: &Value, receipt: &Value, cached: bool) -> bool {
    if !payload["text"].is_string() || payload["text"] != receipt["text"] {
        return false;
    }
    let empty = Vec::new();
    let files = payload["files"].as_array().unwrap_or(&empty);
    let expected = receipt["attachments"].as_array().unwrap_or(&empty);
    let digests = receipt["attachment_digests"].as_array().unwrap_or(&empty);
    (payload["files"].is_null() || payload["files"].is_array())
        && files.len() == expected.len()
        && files.len() == digests.len()
        && files
            .iter()
            .zip(expected)
            .zip(digests)
            .all(|((native, expected), digest)| {
                native["name"] == expected["name"]
                    && native["mime"] == expected["mime"]
                    && native["source"]["type"] == "inline"
                    && (native["data"]
                        .as_str()
                        .is_some_and(|data| *digest == hash(data.as_bytes()))
                        || cached
                            && native["data"].is_null()
                            && native["hgs_data_sha256"] == *digest)
            })
}
