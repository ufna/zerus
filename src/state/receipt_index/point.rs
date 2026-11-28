//! A known UUID is already a bounded locator. Only its original proves delivery;
//! this reader never opens SQLite, discovers files, or repairs missing evidence.
use super::*;
use crate::state::repositories::directory::Directory;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::state) struct DeliveryQuery {
    #[serde(default = "original_schema")]
    schema: u8,
    request_id: String,
    message_request_id: String,
    expected_machine_id: String,
    expected_session_id: String,
    expected_run_id: String,
    expected_conversation_id: String,
    provider: String,
    native_digest: String,
    #[serde(default)]
    agent_id: String,
}

fn original_schema() -> u8 { 1 }

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn provider(value: &str) -> bool {
    matches!(value, "claude" | "codex" | "kimi" | "dsh" | "opencode")
}
impl DeliveryQuery {
    pub(in crate::state) fn validate(&self, name: &str) -> Result<()> {
        if !matches!(self.schema, 1 | 2)
            || name.is_empty()
            || !bounded_identity(name, 512)
            || !uuid(&self.request_id)
            || !uuid(&self.message_request_id)
            || !uuid(&self.expected_machine_id)
            || !uuid(&self.expected_session_id)
            || self.expected_run_id.is_empty()
            || !bounded_identity(&self.expected_run_id, 128)
            || !bounded_identity(&self.expected_conversation_id, 512)
            || !bounded_identity(&self.agent_id, 512)
            || self.agent_id.contains(['/', '\\'])
            || !provider(&self.provider)
            || !hex(&self.native_digest)
        {
            return Err("Invalid delivery receipt query".into());
        }
        Ok(())
    }
    pub(in crate::state) fn machine(&self) -> &str {
        &self.expected_machine_id
    }
    fn source(&self) -> Source {
        match self.provider.as_str() {
            "dsh" => Source::Dsh,
            "opencode" => Source::OpenCode,
            _ => Source::Terminal,
        }
    }
    fn response(&self, name: &str, reason: &str) -> Value {
        json!({"schema":self.schema,"request_id":self.request_id,"message_request_id":self.message_request_id,
            "name":name,"machine_id":self.expected_machine_id,"session_id":self.expected_session_id,
            "run_id":self.expected_run_id,"conversation_id":self.expected_conversation_id,
            "provider":self.provider,"agent_id":self.agent_id,"native_digest":self.native_digest,
            "status":"unknown","reason":reason})
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::state) struct DeliveryBinding {
    pub(in crate::state) schema: u8,
    pub(in crate::state) machine_id: String,
    pub(in crate::state) session_id: String,
    pub(in crate::state) provider: String,
    pub(in crate::state) namespace: String,
}
impl DeliveryBinding {
    pub(in crate::state) fn valid(&self) -> bool {
        self.schema == 1
            && uuid(&self.machine_id)
            && uuid(&self.session_id)
            && provider(&self.provider)
            && self.namespace.strip_prefix("account:").is_some_and(hex)
    }
}

/// Capture before the first possible offer, never retrofit a replayed receipt.
/// The existing graph target has already bound its native account and machine.
pub(in crate::state) fn delivery_binding(record: &Value, target: &Value) -> Option<Value> {
    let binding = DeliveryBinding {
        schema: 1,
        machine_id: string(&target["endpoint"], "machine").into(),
        session_id: string(record, "session_id").into(),
        provider: string(record, "agent").into(),
        namespace: string(&target["endpoint"], "namespace").into(),
    };
    (binding.valid()
        && target["schema"] == 1
        && target["run_id"] == record["run_id"]
        && target["endpoint"]["provider"] == record["agent"])
        .then(|| json!(binding))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GraphTarget {
    schema: u8,
    node: String,
    name: String,
    run_id: String,
    endpoint: GraphEndpoint,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GraphEndpoint {
    machine: String,
    provider: String,
    namespace: String,
    conversation: String,
    provisional_run: String,
}

#[derive(Deserialize, Serialize)]
struct Original {
    request_id: String,
    name: String,
    run_id: String,
    conversation_id: String,
    // Keep the existing binding reader's treatment of absent/non-string
    // resolution metadata, without retaining any payload fields.
    #[serde(default)]
    resolved_conversation_id: Value,
    digest: String,
    status: String,
    transport: String,
    submitted_at: f64,
    #[serde(default)]
    first_message: bool,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    queued: bool,
    native_message_id: Option<String>,
    delivery_binding: Option<DeliveryBinding>,
    graph_target: Option<GraphTarget>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    claude_outbox: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    codex_outbox: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_queued_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    kimi_outbox: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    terminal_native_after_refusal: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    native: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    native_input: Value,
}

fn original(state: &Path, source: Source, id: &str) -> std::result::Result<Original, &'static str> {
    decode_metadata(&original_bytes(state, source, id)?, source, id)
}

pub(in crate::state) fn dsh_outbox_original(state: &Path, id: &str) -> Result<Option<Value>> {
    if !uuid(id) {
        return Err("Invalid DeepSeek outbox request".into());
    }
    match original_bytes(state, Source::Dsh, id) {
        Ok(bytes) => {
            decode_metadata(&bytes, Source::Dsh, id).map_err(str::to_owned)?;
            serde_json::from_slice(&bytes).map(Some).map_err(|error| error.to_string())
        }
        Err("missing") => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn original_bytes(state: &Path, source: Source, id: &str) -> std::result::Result<Vec<u8>, &'static str> {
    if !state.is_absolute() {
        return Err("source_unavailable");
    }
    let mut directory = Directory::open(state).map_err(|_| "source_unavailable")?;
    if !controlled_directory(&directory)? {
        return Err("source_unavailable");
    }
    for component in source.directory().split('/') {
        directory = directory
            .child(component)
            .map_err(|_| "source_unavailable")?;
        if !controlled_directory(&directory)? {
            return Err("source_unavailable");
        }
    }
    let file = directory
        .read_private_optional(&format!("{id}.json"))
        .map_err(|_| "unreadable")?
        .ok_or("missing")?;
    let (before, metadata) =
        crate::platform::FileIdentity::with_metadata(&file).map_err(|_| "unreadable")?;
    if metadata.len() > ORIGINAL_BYTES {
        return Err("oversized");
    }
    let mut bytes = Vec::new();
    (&file)
        .take(ORIGINAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    let (after, end) =
        crate::platform::FileIdentity::with_metadata(&file).map_err(|_| "unreadable")?;
    if before != after || metadata.len() != end.len() || bytes.len() as u64 != end.len() {
        return Err("changed");
    }
    directory.verify().map_err(|_| "source_unavailable")?;
    Ok(bytes)
}

fn decode_metadata(
    bytes: &[u8],
    source: Source,
    id: &str,
) -> std::result::Result<Original, &'static str> {
    // Unknown payload fields are validated and skipped by serde. Only metadata
    // is retained; typed identities/nested bindings still reject duplicates.
    let receipt: Original = serde_json::from_slice(bytes).map_err(|error| {
        if error.is_syntax() || error.is_eof() {
            "corrupt"
        } else {
            "invalid_receipt"
        }
    })?;
    // Reuse every existing source/binding constraint on a metadata-only view.
    // In particular, do not lose resolved-CID agreement or identity bounds.
    let metadata = serde_json::to_value(&receipt).map_err(|_| "invalid_receipt")?;
    Binding::parse(source, &metadata, &format!("{id}.json")).map_err(|_| "binding_changed")?;
    Ok(receipt)
}

fn controlled_directory(directory: &Directory) -> std::result::Result<bool, &'static str> {
    // Legacy state roots can be 0755. Owner-only entry mutation and a pinned
    // walk protect the binding; the receipt itself must remain a private file.
    #[cfg(unix)]
    let result = directory.owner_controls_entries();
    #[cfg(windows)]
    let result = directory.private();
    result.map_err(|_| "source_unavailable")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutboxQuery {
    request_id: String,
    expected_run_id: String,
    expected_conversation_id: String,
    input_sha256: String,
}

pub(in crate::state) fn claude_prompt_uuid(value: &str) -> bool {
    uuid(value)
}

pub(in crate::state) fn claude_outbox_prompt(receipt: &Value) -> Option<&str> {
    (receipt["claude_outbox"] == true && receipt["native"] == "transcript"
        && uuid(string(&receipt["native_input"], "uuid")))
        .then_some(string(&receipt["native_input"], "prompt_id"))
        .filter(|id| uuid(id))
}

fn terminal_outbox_identity(name: &str, query: &OutboxQuery) -> bool {
    (name.starts_with("codex/") || name.starts_with("kimi/") || name.starts_with("claude/"))
        && bounded_identity(name, 512)
        && uuid(&query.request_id)
        && bounded_identity(&query.expected_run_id, 128)
        && !query.expected_run_id.is_empty()
        && bounded_identity(&query.expected_conversation_id, 512)
        && !query.expected_conversation_id.is_empty()
        && hex(&query.input_sha256)
}

/// Read an exact terminal refusal or Kimi terminal submission. The original
/// input digest binds the attachments and message for either answer.
pub(in crate::state) fn terminal_outbox_status(state: &Path, name: &str, bytes: &[u8]) -> Result<Value> {
    let query: OutboxQuery = serde_json::from_slice(bytes).map_err(|_| "Invalid terminal outbox query")?;
    if !terminal_outbox_identity(name, &query) {
        return Err("Invalid terminal outbox identity".into());
    }
    let bytes = original_bytes(state, Source::Terminal, &query.request_id)
        .map_err(|_| "Original terminal delivery is unconfirmed")?;
    if let Ok(original) = decode_metadata(&bytes, Source::Terminal, &query.request_id) {
        if original.status == "submitted" && original.request_id == query.request_id && original.name == name
            && original.run_id == query.expected_run_id && original.conversation_id == query.expected_conversation_id
            && original.digest == query.input_sha256 && !original.first_message && original.agent_id.is_empty()
            && original.submitted_at.is_finite() && original.submitted_at > 0.0
        {
            if name.starts_with("codex/") && original.codex_outbox && original.transport == "codex_queue"
                && original.native_queued_message_id.as_deref().is_some_and(uuid)
            {
                return Ok(json!({"schema":1,"status":"submitted","proof":"native_codex_receipt","codex_outbox":true,
                    "transport":"codex_queue","native_queued_message_id":original.native_queued_message_id,
                    "request_id":original.request_id,"name":name,"run_id":original.run_id,"conversation_id":original.conversation_id,
                    "input_sha256":original.digest,"submitted_at":original.submitted_at}));
            }
            if name.starts_with("kimi/") && original.transport == "terminal"
                && (original.kimi_outbox || original.terminal_native_after_refusal)
            {
                return Ok(json!({"schema":1,"status":"submitted","proof":"native_kimi_terminal_receipt",
                    "request_id":original.request_id,"name":name,"run_id":original.run_id,
                    "conversation_id":original.conversation_id,"input_sha256":query.input_sha256,
                    "transport":"terminal","kimi_outbox":true,"submitted_at":original.submitted_at}));
            }
        }
    }
    terminal_outbox_refusal_from_bytes(name, &query, &bytes)
}

fn terminal_outbox_refusal_from_bytes(name: &str, query: &OutboxQuery, bytes: &[u8]) -> Result<Value> {
    use zerus_client_core::desktop::deferred_send::Refusal;
    let refusal = Refusal::from_cli_record(bytes).map_err(str::to_owned)?;
    if refusal.request_id != query.request_id || refusal.session != name
        || refusal.run_id != query.expected_run_id || refusal.conversation_id != query.expected_conversation_id
        || refusal.input_sha256 != query.input_sha256
    {
        return Err("Original terminal refusal does not match this message".into());
    }
    #[derive(Deserialize)]
    struct QueueGate {
        outbox_waiting: Option<bool>,
        terminal_native_accept: Option<bool>,
    }
    let gate: QueueGate = serde_json::from_slice(bytes).map_err(|_| "Invalid terminal refusal")?;
    if name.starts_with("claude/") && gate.terminal_native_accept != Some(true) {
        return Err("Original Claude delivery is unconfirmed".into());
    }
    // Older records lack the typed gate. Recognize only the historic draft
    // and idle reasons, after verifying the complete negative and payload.
    let waiting = gate.outbox_waiting.unwrap_or_else(|| {
        refusal.reason == input::DRAFT || refusal.reason == input::Gate::NotIdle.reason()
    });
    let status = if waiting { "waiting_for_idle" } else { "not_offered" };
    Ok(json!({"schema":1,"status":status,"proof":"native_input_lock","transport":"terminal",
        "request_id":refusal.request_id,"name":name,"run_id":refusal.run_id,
        "conversation_id":refusal.conversation_id,"input_sha256":refusal.input_sha256,
        "reason":refusal.reason}))
}

fn claude_outbox_identity(name: &str, query: &OutboxQuery) -> bool {
    name.starts_with("claude/")
        && terminal_outbox_identity(name, query)
        && uuid(&query.expected_run_id)
        && uuid(&query.expected_conversation_id)
}

/// Read one final native Claude receipt. No transcript search, pane input,
/// or replay can result from a missing or non-final receipt.
pub(in crate::state) fn claude_outbox_status(state: &Path, name: &str, bytes: &[u8]) -> Result<Value> {
    let query: OutboxQuery = serde_json::from_slice(bytes).map_err(|_| "Invalid Claude outbox query")?;
    if !claude_outbox_identity(name, &query) {
        return Err("Invalid Claude outbox identity".into());
    }
    let receipt = original(state, Source::Terminal, &query.request_id)
        .map_err(|_| "Original Claude delivery is unconfirmed")?;
    claude_outbox_submission(name, &query, receipt)
}

/// The explicit check classifies one bounded snapshot. In particular a failure
/// to prove submission cannot trigger a second open to look for refusal.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(in crate::state) fn observe_claude_outbox(state: &Path, name: &str, bytes: &[u8]) -> Result<Value> {
    let query: OutboxQuery = serde_json::from_slice(bytes).map_err(|_| "Invalid Claude outbox query")?;
    // Typed terminal refusals retain their existing bounded identity rules;
    // positive Claude receipts additionally require canonical run/CID UUIDs.
    if !name.starts_with("claude/") || !terminal_outbox_identity(name, &query) {
        return Err("Invalid Claude outbox identity".into());
    }
    let original = original_bytes(state, Source::Terminal, &query.request_id)
        .map_err(|_| "Original Claude delivery is unconfirmed")?;
    if claude_outbox_identity(name, &query) {
        if let Ok(receipt) = decode_metadata(&original, Source::Terminal, &query.request_id) {
            if let Ok(submission) = claude_outbox_submission(name, &query, receipt) {
                return Ok(submission);
            }
        }
    }
    terminal_outbox_refusal_from_bytes(name, &query, &original)
}

fn claude_outbox_submission(name: &str, query: &OutboxQuery, receipt: Original) -> Result<Value> {
    if !receipt.claude_outbox || receipt.status != "submitted" || receipt.transport != "terminal"
        || receipt.request_id != query.request_id || receipt.name != name
        || receipt.run_id != query.expected_run_id || receipt.conversation_id != query.expected_conversation_id
        || receipt.digest != query.input_sha256 || receipt.first_message || !receipt.agent_id.is_empty()
        || !matches!(receipt.native.as_str(), Some("transcript" | "queue"))
        || !receipt.submitted_at.is_finite() || receipt.submitted_at <= 0.0
    {
        return Err("Original Claude delivery is unconfirmed".into());
    }
    let native_input = if receipt.native == "transcript"
        && uuid(string(&receipt.native_input, "uuid")) && uuid(string(&receipt.native_input, "prompt_id"))
    {
        json!({"uuid":receipt.native_input["uuid"],"prompt_id":receipt.native_input["prompt_id"]})
    } else {
        Value::Null
    };
    Ok(json!({"schema":1,"status":"submitted","proof":"native_claude_receipt","claude_outbox":true,
        "request_id":receipt.request_id,"name":name,"run_id":receipt.run_id,
        "conversation_id":receipt.conversation_id,"input_sha256":query.input_sha256,
        "transport":receipt.transport,"native":receipt.native,"native_input":native_input,
        "submitted_at":receipt.submitted_at}))
}

/// Historical proof only: no name resolution, current-session read, SQL,
/// provider reconciliation or write. Unknown never grants replay authority.
pub(in crate::state) fn delivery_receipt(state: &Path, name: &str, query: &DeliveryQuery) -> Value {
    let fail = |reason| query.response(name, reason);
    if query.schema == 2 && matches!(query.source(), Source::Terminal) {
        if let Ok(mut negative) = input::outcome::lookup(state, &query.message_request_id) {
            for (field, expected) in [
                ("request_id", query.message_request_id.as_str()), ("name", name),
                ("machine_id", query.expected_machine_id.as_str()),
                ("session_id", query.expected_session_id.as_str()),
                ("run_id", query.expected_run_id.as_str()),
                ("conversation_id", query.expected_conversation_id.as_str()),
                ("provider", query.provider.as_str()), ("native_digest", query.native_digest.as_str()),
            ] {
                if negative[field].as_str() != Some(expected) { return fail("binding_changed"); }
            }
            if !query.agent_id.is_empty() { return fail("binding_changed"); }
            negative["message_request_id"] = negative["request_id"].clone();
            negative["request_id"] = json!(query.request_id);
            negative["agent_id"] = json!("");
            return negative;
        }
    }
    let receipt = match original(state, query.source(), &query.message_request_id) {
        Ok(value) => value,
        Err(reason) => return fail(reason),
    };
    let Some(binding) = receipt.delivery_binding else {
        return fail("binding_unavailable");
    };
    if let Some(target) = &receipt.graph_target {
        let endpoint = &target.endpoint;
        let provisional: &str = if receipt.conversation_id.is_empty() {
            &receipt.run_id
        } else {
            ""
        };
        if target.schema != 1
            || target.name != receipt.name
            || target.run_id != receipt.run_id
            || target.node != format!("node:{}", hash(&json!(endpoint).to_string()))
            || endpoint.machine != binding.machine_id
            || endpoint.provider != binding.provider
            || endpoint.namespace != binding.namespace
            || endpoint.conversation != receipt.conversation_id
            || endpoint.provisional_run != provisional
        {
            return fail("binding_changed");
        }
    }
    if !binding.valid()
        || receipt.request_id != query.message_request_id
        || receipt.name != name
        || receipt.run_id != query.expected_run_id
        || receipt.conversation_id != query.expected_conversation_id
        || receipt.agent_id != query.agent_id
        || binding.machine_id != query.expected_machine_id
        || binding.session_id != query.expected_session_id
        || binding.provider != query.provider
        || (receipt.conversation_id.is_empty() && !receipt.first_message)
    {
        return fail("binding_changed");
    }
    if !hex(&receipt.digest) || receipt.digest != query.native_digest {
        return fail("digest_mismatch");
    }
    let transport = match query.source() {
        Source::Terminal => "terminal",
        Source::Dsh => "native_dsh",
        Source::OpenCode => "native_opencode",
        Source::Question => return fail("unsupported_receipt"),
    };
    if receipt.status != "submitted" || receipt.transport != transport {
        return fail("not_submitted");
    }
    if !receipt.submitted_at.is_finite() || receipt.submitted_at < 0.0 {
        return fail("invalid_receipt");
    }
    // OpenCode allocates this exact provider ID before its first offer. Other
    // producers record no native message ID; unsupported metadata is not proof.
    let native_id_valid = match query.source() {
        Source::OpenCode => receipt.native_message_id.as_deref().is_some_and(|id| {
            id.len() > "msg_".len() && crate::state::opencode::native_id(id, "msg_")
        }),
        _ => receipt.native_message_id.is_none(),
    };
    if !native_id_valid {
        return fail("invalid_receipt");
    }
    let mut proof = query.response(name, "");
    proof.as_object_mut().unwrap().remove("reason");
    proof["status"] = json!("submitted");
    proof["proof"] = json!("original_receipt");
    proof["submitted_at"] = json!(receipt.submitted_at);
    proof["queued"] = json!(receipt.queued);
    if let Some(id) = receipt.native_message_id {
        proof["native_message_id"] = json!(id);
    }
    proof
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> Value {
        json!({"request_id":"11111111-1111-4111-8111-111111111111","name":"codex/example/read",
            "run_id":"run","conversation_id":"conversation","digest":"a".repeat(64),
            "status":"submitted","transport":"terminal","submitted_at":1.0})
    }

    #[test]
    fn typed_metadata_skips_payloads_but_keeps_resolved_binding_checks() {
        let mut value = receipt();
        value["text"] = json!("private fixture body".repeat(4000));
        value["submitted_text"] = value["text"].clone();
        value["attachments"] = json!([{"data_base64":"private fixture bytes"}]);
        let id = value["request_id"].as_str().unwrap().to_owned();
        let parsed =
            decode_metadata(&serde_json::to_vec(&value).unwrap(), Source::Terminal, &id).unwrap();
        let metadata = serde_json::to_value(parsed).unwrap();
        for field in ["text", "submitted_text", "attachments"] {
            assert!(metadata.get(field).is_none(), "{field}");
        }
        for resolved in [Value::Null, json!(""), json!("conversation"), json!(false)] {
            value["resolved_conversation_id"] = resolved;
            assert!(
                decode_metadata(&serde_json::to_vec(&value).unwrap(), Source::Terminal, &id)
                    .is_ok()
            );
        }
        value["resolved_conversation_id"] = json!("foreign");
        assert!(matches!(
            decode_metadata(&serde_json::to_vec(&value).unwrap(), Source::Terminal, &id),
            Err("binding_changed")
        ));
    }

    #[test]
    fn duplicate_metadata_and_malformed_skipped_payloads_remain_unknown() {
        let value = receipt();
        let id = value["request_id"].as_str().unwrap();
        let bytes = value.to_string();
        let duplicate = format!("{},\"name\":\"foreign\"}}", &bytes[..bytes.len() - 1]);
        assert!(decode_metadata(duplicate.as_bytes(), Source::Terminal, id).is_err());
        let malformed = format!("{},\"text\":\"unterminated", &bytes[..bytes.len() - 1]);
        assert!(matches!(
            decode_metadata(malformed.as_bytes(), Source::Terminal, id),
            Err("corrupt")
        ));
    }
}
