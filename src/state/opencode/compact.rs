//! Reserve locally before posting one queue-only native compaction operation.
use super::*;

const UNAVAILABLE: &str = "Manual compaction is unavailable on this OpenCode service.";
const BUSY: &str = "Wait for OpenCode to finish its current work and queued messages.";
const EMPTY: &str = "Send a message before compacting this OpenCode conversation.";
const PENDING: &str = "An earlier OpenCode compaction is still pending or unconfirmed.";
const UNCERTAIN: &str =
    "OpenCode compaction is unconfirmed. Check this conversation before trying again.";
const FIELD: &str = "opencode_compaction";

fn read(path: &Path) -> Result<Option<Value>> {
    let bytes =
        labels::read_file(path, 8192).map_err(|_| "OpenCode compact receipt is unavailable")?;
    bytes
        .map(|data| {
            serde_json::from_slice(&data).map_err(|_| "Invalid OpenCode compact receipt".into())
        })
        .transpose()
}

fn matches_request(attempt: &Value, record: &Value, request: &compact_context::Request) -> bool {
    attempt["request_id"] == request.request_id
        && attempt["run_id"] == request.expected_run_id
        && attempt["conversation_id"] == request.expected_conversation_id
        && attempt["session_id"] == record["session_id"]
}

fn pending(attempt: &Value) -> bool {
    ["submitted", "compacting", "uncertain"].contains(&string(attempt, "status"))
}

fn native_status(message: &Value, attempt: &Value) -> Option<&'static str> {
    if message["id"] != attempt["native_message_id"]
        || message["type"] != "compaction"
        || message["reason"] != "manual"
    {
        return None;
    }
    match string(message, "status") {
        "completed" => Some("completed"),
        "failed" => Some("failed"),
        "cancelled" | "canceled" => Some("cancelled"),
        "running" | "pending" => Some("compacting"),
        _ => None,
    }
}

/// The exact native message is authoritative. A missing message can still be
/// waiting in the native inbox and never authorizes another offer.
fn reconcile(
    native: &dyn Native,
    record: &Value,
    attempt: &Value,
    messages: &[Value],
    deadline: Instant,
) -> Value {
    let mut result = attempt.clone();
    if !pending(attempt) || attempt["conversation_id"] != record["conversation_id"] {
        return result;
    }
    let id = string(attempt, "native_message_id");
    if !native_id(id, "msg_") {
        return result;
    }
    let native_state = messages
        .iter()
        .rev()
        .find_map(|message| native_status(message, attempt))
        .or_else(|| {
            let reply = native
                .raw_until(
                    "GET",
                    &session_path(record, &format!("/message/{id}")),
                    None,
                    deadline,
                )
                .ok()?;
            (reply.status == 200)
                .then(|| native_status(&reply.body["data"], attempt))
                .flatten()
        });
    if let Some(state) = native_state {
        result["status"] = json!(state);
        if state != "compacting" {
            result
                .as_object_mut()
                .expect("compact receipt")
                .remove("error");
        }
    }
    result
}

fn reason(available: bool, activity: bool, empty: bool, attempt: &Value) -> &'static str {
    if !available {
        UNAVAILABLE
    } else if pending(attempt) {
        PENDING
    } else if activity {
        BUSY
    } else if empty {
        EMPTY
    } else {
        ""
    }
}

pub(super) fn decorate(native: &dyn Native, record: &Value, seen: &Observed, row: &mut Value) {
    let attempt = reconcile(
        native,
        record,
        &record[FIELD],
        &seen.page.messages,
        Instant::now() + Duration::from_millis(300),
    );
    let queued = [&seen.inbox["data"], &seen.forms, &seen.permissions]
        .into_iter()
        .any(|items| items.as_array().is_none_or(|items| !items.is_empty()));
    let why = reason(
        native.manual_compact(),
        seen.active || queued,
        seen.page.messages.is_empty(),
        &attempt,
    );
    row["compact_context_supported"] = json!(why.is_empty());
    row["compact_context_reason"] = json!(why);
    if attempt["run_id"] == record["run_id"]
        && attempt["conversation_id"] == record["conversation_id"]
        && attempt.is_object()
    {
        row["compact_context_request"] = attempt;
    }
}

pub(in crate::state) fn submit(name: &str, request: &compact_context::Request) -> Result<Value> {
    if !session_id::canonical(&request.request_id)
        || request.request_id == "00000000-0000-0000-0000-000000000000"
    {
        return Err("Invalid compact request identity".into());
    }
    let _guard =
        try_bindings(Duration::from_millis(500)).ok_or("OpenCode session state is busy")?;
    let mut record = binding(name)?;
    let expected = json!({"expected_run_id":request.expected_run_id,"expected_conversation_id":request.expected_conversation_id});
    check_request_identity(&record, &expected)?;
    terminating::check_mutation(&record)?;
    let path = root()
        .join("opencode/compact-receipts")
        .join(format!("{}.json", request.request_id));
    if let Some(previous) = read(&path)? {
        if !matches_request(&previous, &record, request) {
            return Err("This request ID belongs to another OpenCode compaction".into());
        }
        return Ok(previous);
    }

    let until = Instant::now() + Duration::from_secs(8);
    let service = service::connect_within(until).map_err(|why| why.reason(&service::computer()))?;
    if !service.manual_compact {
        return Err(UNAVAILABLE.into());
    }
    let native = Bounded {
        service: &service,
        deadline: until,
    };
    verify_session(&native, &record)?;
    let previous = reconcile(&native, &record, &record[FIELD], &[], until);
    if pending(&previous) {
        return Err(PENDING.into());
    }
    let active = active_sessions(&native)?
        .get(string(&record, "conversation_id"))
        .is_some();
    let mut occupied = active;
    for suffix in ["/inbox", "/form", "/permission"] {
        occupied |= !pending_requests(&native, &record, suffix)?
            .as_array()
            .ok_or("Invalid OpenCode pending requests")?
            .is_empty();
    }
    let why = reason(true, occupied, empty_history(&native, &record)?, &previous);
    if !why.is_empty() {
        return Err(why.into());
    }

    ensure_session_id(&mut record);
    let mut receipt = json!({"request_id":request.request_id,"name":name,"session_id":record["session_id"],
        "run_id":record["run_id"],"conversation_id":record["conversation_id"],"native_message_id":new_id("msg_"),
        "at":now(),"status":"uncertain","error":UNCERTAIN});
    atomic(&path, &receipt.to_string())?;
    record[FIELD] = receipt.clone();
    save(&mut record)?;

    let reply = native.raw(
        "POST",
        &session_path(&record, "/compact"),
        Some(&json!({"id": receipt["native_message_id"], "delivery":"queue"})),
    );
    if let Ok(reply) = reply {
        let offered = &reply.body["data"];
        if reply.status == 200
            && offered["id"] == receipt["native_message_id"]
            && offered["sessionID"] == record["conversation_id"]
            && offered["type"] == "compaction"
            && offered["delivery"] == "queue"
        {
            receipt["status"] = json!("submitted");
            receipt
                .as_object_mut()
                .expect("compact receipt")
                .remove("error");
        } else if matches!(reply.status, 400 | 401 | 404) {
            receipt["status"] = json!("failed");
            receipt["error"] = json!("OpenCode refused this compaction.");
        }
    }
    if receipt != record[FIELD] {
        // A failure publishing either outcome leaves at least the original
        // durable no-replay marks and can only make the result uncertain.
        record[FIELD] = receipt.clone();
        save(&mut record).map_err(|_| UNCERTAIN)?;
        atomic(&path, &receipt.to_string()).map_err(|_| UNCERTAIN)?;
    }
    Ok(receipt)
}
