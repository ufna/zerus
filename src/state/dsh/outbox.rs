//! One guarded Desktop offer and a bounded point read of its own native result.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const WAIT_PROOF: &str = "native_dsh_idle_guard";
const RECEIPT_PROOF: &str = "native_dsh_receipt";
const UNCONFIRMED: &str =
    "DeepSeek delivery uncertain; the original outbox request was not confirmed";
const PREFLIGHT: Duration = Duration::from_secs(3);
const PROOF_READ: Duration = Duration::from_millis(1500);
const REQUEST_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    request_id: String,
    expected_run_id: String,
    expected_conversation_id: String,
    input_sha256: String,
}

impl Query {
    fn validate(&self) -> Result<()> {
        if !session_id::canonical(&self.request_id)
            || !session_id::canonical(&self.expected_run_id)
            || !self
                .expected_conversation_id
                .strip_prefix("session-")
                .is_some_and(session_id::canonical)
            || !sha(&self.input_sha256)
        {
            return Err("Invalid DeepSeek outbox identity".into());
        }
        Ok(())
    }

    fn matches(&self, name: &str, receipt: &Value) -> bool {
        receipt["request_id"] == self.request_id
            && receipt["name"] == name
            && receipt["run_id"] == self.expected_run_id
            && receipt["conversation_id"] == self.expected_conversation_id
            && receipt["transport"] == "native_dsh"
            && receipt["dsh_outbox"]["input_sha256"] == self.input_sha256
            && string(receipt, "agent_id").is_empty()
    }

    fn target(&self, name: &str, record: &Value) -> Result<()> {
        if record["name"] != name
            || record["run_id"] != self.expected_run_id
            || record["conversation_id"] != self.expected_conversation_id
            || record["permissions_pending"] == true
            || record["paused"] == true
            || retirement::pending(record)
        {
            return Err("DeepSeek outbox target changed; refresh before sending".into());
        }
        Ok(())
    }

    fn before_offer(&self, name: &str, status: &str) -> Value {
        json!({"schema":1,"status":status,"proof":WAIT_PROOF,
            "request_id":self.request_id,"name":name,"run_id":self.expected_run_id,
            "conversation_id":self.expected_conversation_id,"input_sha256":self.input_sha256})
    }
}

fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(in crate::state) fn valid_message_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn attachment_ids(content: &[Value]) -> Result<Vec<String>> {
    content
        .iter()
        .filter(|part| matches!(string(part, "type"), "image" | "file"))
        .map(|part| {
            let bytes = STANDARD
                .decode(string(part, "data"))
                .map_err(|_| "Invalid attachment data")?;
            Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
        })
        .collect()
}

fn input(limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("DeepSeek outbox request exceeds its bound".into());
    }
    Ok(bytes)
}

fn receipt_path(state: &Path, id: &str) -> PathBuf {
    state.join("dsh/receipts").join(format!("{id}.json"))
}

fn read_original(state: &Path, name: &str, query: &Query) -> Result<Option<Value>> {
    let original = receipt_index::dsh_outbox_original(state, &query.request_id)
        .map_err(|error| format!("{UNCONFIRMED}: {error}"))?;
    if original
        .as_ref()
        .is_some_and(|receipt| !query.matches(name, receipt))
    {
        return Err("DeepSeek outbox request already belongs to another delivery".into());
    }
    Ok(original)
}

fn lock_request(query: &Query) -> Result<witness::RequestLock> {
    witness::request_lock_until(
        &query.request_id,
        Some(Instant::now() + Duration::from_millis(500)),
    )?
    .ok_or_else(|| UNCONFIRMED.into())
}

/// The receipt response contains only the exact binding and native message identity.
fn submitted(query: &Query, name: &str, receipt: &Value) -> Result<Value> {
    let message = string(&receipt["dsh_outbox"], "message_id");
    if receipt["status"] != "submitted" || !valid_message_id(message) {
        return Err(UNCONFIRMED.into());
    }
    Ok(
        json!({"schema":1,"status":"submitted","proof":RECEIPT_PROOF,"transport":"native_dsh",
        "request_id":query.request_id,"name":name,"run_id":query.expected_run_id,
        "conversation_id":query.expected_conversation_id,"input_sha256":query.input_sha256,
        "native_message_id":message,"submitted_at":receipt["submitted_at"]}),
    )
}

/// Never calls a send route. A pending marker only settles on the original
/// generation's durable message, from the same native inbox splice and request.
fn settle(
    state: &Path,
    name: &str,
    query: &Query,
    record: &Value,
    mut receipt: Value,
) -> Result<Value> {
    let marker = &receipt["dsh_outbox"];
    if receipt["status"] != "pending"
        || string(marker, "generation").is_empty()
        || !sha(string(marker, "content_digest"))
    {
        return Err(UNCONFIRMED.into());
    }
    let facts = rpc_within(
        record,
        "outbox-proof",
        json!({
            "sessionId":query.expected_conversation_id,"requestId":query.request_id,
            "generation":marker["generation"],"waitMs":250,
        }),
        PROOF_READ,
    )
    .map_err(|_| UNCONFIRMED.to_owned())?;
    let message = string(&facts, "messageId");
    if facts["state"] != "tracked"
        || facts["generation"] != marker["generation"]
        || facts["sessionId"] != query.expected_conversation_id
        || facts["requestId"] != query.request_id
        || !valid_message_id(message)
    {
        return Err(UNCONFIRMED.into());
    }
    let witness::Settled::Received(seq) =
        witness::classify(&facts, &query.request_id, string(marker, "content_digest"))
    else {
        return Err(UNCONFIRMED.into());
    };
    let mut delivered = facts["events"]
        .as_array()
        .ok_or(UNCONFIRMED)?
        .iter()
        .filter(|event| {
            event["type"] == "user/message"
                && event["seq"] == seq
                && event["id"] == message
                && event["rpcId"] == query.request_id
        });
    if !delivered.next().is_some_and(|event| {
        marker["attachment_ids"].is_array() && event["attachmentIds"] == marker["attachment_ids"]
    }) || delivered.next().is_some()
    {
        return Err(UNCONFIRMED.into());
    }
    receipt["dsh_outbox"]["message_id"] = json!(message);
    receipt["dsh_outbox"]["seq"] = json!(seq);
    receipt["status"] = json!("submitted");
    receipt_index::persist(&receipt_path(state, &query.request_id), &receipt)
        .map_err(|_| UNCONFIRMED.to_owned())?;
    submitted(query, name, &receipt)
}

pub(in crate::state) fn send_when_idle(args: &[String]) -> Result<i32> {
    let [name, idle, format] = args else {
        return Err("Use hgs send SESSION --when-idle --json".into());
    };
    if idle != "--when-idle" || format != "--json" {
        return Err("Use hgs send SESSION --when-idle --json".into());
    }
    let bytes = input(REQUEST_BYTES)?;
    let request: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if request["only_when_idle"] != true
        || !string(&request, "agent_id").is_empty()
        || !string(&request, "expected_compaction_id").is_empty()
    {
        return Err("Guarded DeepSeek Send requires the main native conversation".into());
    }
    let query = Query {
        request_id: string(&request, "request_id").into(),
        expected_run_id: string(&request, "expected_run_id").into(),
        expected_conversation_id: string(&request, "expected_conversation_id").into(),
        input_sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    query.validate()?;
    let (_session, record) = dispatch_lock::acquire_until(name, Instant::now() + PREFLIGHT)?;
    query.target(name, &record)?;
    let _request = lock_request(&query)?;
    let state = existing_absolute_root()?;
    private_dir(&state.join("dsh/receipts"))?;
    if let Some(previous) = read_original(&state, name, &query)? {
        match string(&previous, "status") {
            "submitted" => {
                println!("{}", submitted(&query, name, &previous)?);
                return Ok(0);
            }
            "waiting_for_idle" => {}
            _ => return Err(UNCONFIRMED.into()),
        }
    }

    // Reads and attachment preparation cannot offer a prompt. If the host is
    // unavailable at this point the original request is still safe to hold.
    let waiting = || {
        println!("{}", query.before_offer(name, "waiting_for_idle"));
        Ok(0)
    };
    let refusal = |reason: String| {
        let mut receipt = query.before_offer(name, "not_offered");
        receipt["reason"] = json!(reason);
        println!("{receipt}");
        Ok(0)
    };
    let Ok(readiness) = rpc_within(
        &record,
        "outbox-state",
        json!({"sessionId":query.expected_conversation_id}),
        PREFLIGHT,
    ) else {
        return waiting();
    };
    let generation = string(&readiness, "generation");
    if readiness["sessionId"] != query.expected_conversation_id || generation.is_empty() {
        return Err("DeepSeek outbox capability is unavailable".into());
    }
    if readiness["ready"] != true {
        return waiting();
    }
    let prepared = (|| {
        let mut content = message_content(&request)?;
        let staged = attachments::prepare_native(&request)?;
        let attachment_ids = attachment_ids(&content)?;
        prepare_files(&record, &readiness, &mut content, |params| {
            rpc_within(&record, PREPARE_FILES_RPC, params, Duration::from_secs(12))
        })?;
        Ok::<_, String>((content, staged, attachment_ids))
    })();
    let (content, staged, attachment_ids) = match prepared {
        Ok(prepared) => prepared,
        Err(reason)
            if [FILE_PREPARATION_UNCONFIRMED, HOST_UNAVAILABLE].contains(&reason.as_str())
                || reason.starts_with(INCOMPLETE) =>
        {
            return waiting();
        }
        Err(reason) => return refusal(reason),
    };
    let mut receipt = json!({
        "request_id":query.request_id,"name":name,"run_id":query.expected_run_id,
        "conversation_id":query.expected_conversation_id,"transport":"native_dsh","status":"pending",
        "digest":format!("{:x}", Sha256::digest(json!([name,request]).to_string().as_bytes())),
        "at":now(),"submitted_at":now(),"text":request["text"],"submitted_text":request["text"],
        "dsh_outbox":{"input_sha256":query.input_sha256,"generation":generation,
            "content_digest":witness::content_digest(&content),"attachment_ids":attachment_ids}});
    if let Some(target) = session_graph::target_receipt(&record) {
        delivery::bind(&mut receipt, &record, &target);
        receipt["graph_target"] = target;
    }
    let path = receipt_path(&state, &query.request_id);
    receipt["attachments"] = staged.files().clone();
    if receipt_index::persist(&path, &receipt).is_err() {
        staged.keep();
        return Err(UNCONFIRMED.into());
    }
    let response = rpc_within(
        &record,
        "outbox-send",
        json!({
            "sessionId":query.expected_conversation_id,"generation":generation,
            "requestId":query.request_id,"mode":"queue","content":content,
        }),
        Duration::from_secs(8),
    );
    if response.as_ref().is_ok_and(|answer| {
        answer["status"] == "waiting_for_idle"
            && answer["proof"] == WAIT_PROOF
            && answer["requestId"] == query.request_id
            && answer["sessionId"] == query.expected_conversation_id
            && answer["generation"] == generation
    }) {
        // Only the resident native guard's typed pre-offer refusal can release
        // the original request for a later readiness check.
        receipt["status"] = json!("waiting_for_idle");
        receipt["attachments"] = json!([]);
        if receipt_index::persist(&path, &receipt).is_err() {
            staged.keep();
            return Err(UNCONFIRMED.into());
        }
        return waiting();
    }
    staged.keep();
    println!("{}", settle(&state, name, &query, &record, receipt)?);
    Ok(0)
}

pub(in crate::state) fn status(name: &str) -> Result<i32> {
    let query: Query = serde_json::from_slice(&input(8192)?)
        .map_err(|_| "Invalid DeepSeek outbox status query")?;
    query.validate()?;
    let state = existing_absolute_root()?;
    let previous = read_original(&state, name, &query)?.ok_or(UNCONFIRMED)?;
    if previous["status"] == "submitted" {
        println!("{}", submitted(&query, name, &previous)?);
        return Ok(0);
    }
    if previous["status"] != "pending" {
        return Err(UNCONFIRMED.into());
    }
    let (_session, record) =
        dispatch_lock::acquire_until(name, Instant::now() + Duration::from_millis(500))?;
    query.target(name, &record)?;
    let _request = lock_request(&query)?;
    let current = read_original(&state, name, &query)?.ok_or(UNCONFIRMED)?;
    let result = if current["status"] == "submitted" {
        submitted(&query, name, &current)?
    } else {
        settle(&state, name, &query, &record, current)?
    };
    println!("{result}");
    Ok(0)
}
