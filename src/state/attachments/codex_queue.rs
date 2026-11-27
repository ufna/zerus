//! Public history of exact, positively accepted Codex queue offers.
use super::*;

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}

/// A submitted native queue receipt proves queue acceptance. Its optional client
/// identity comes only from the queue row, never from similar text in a turn.
fn codex_queue_receipt(record: &Value, agent: &str, receipt: &Value) -> Option<Value> {
    if record["agent"] != "codex"
        || !agent.is_empty()
        || !belongs(receipt, record, agent)
        || receipt["transport"] != "codex_queue"
        || receipt["codex_outbox"] != true
        || receipt["first_message"] == true
        || receipt["run_id"] != record["run_id"]
        || string(record, "run_id").is_empty()
        || receipt["conversation_id"] != record["conversation_id"]
        || !uuid(string(record, "conversation_id"))
        || !uuid(string(receipt, "request_id"))
        || !uuid(string(receipt, "native_queued_message_id"))
        || !receipt["submitted_at"]
            .as_f64()
            .is_some_and(|at| at.is_finite() && at > 0.)
    {
        return None;
    }
    let mut row = json!({"type":"CodexQueueReceipt","source":"hgs_delivery","transport":"codex_queue",
        "status":"submitted","request_id":receipt["request_id"],"message_id":receipt["request_id"],
        "native_queued_message_id":receipt["native_queued_message_id"],"at":receipt["submitted_at"],
        "run_id":receipt["run_id"],"conversation_id":receipt["conversation_id"],"detail":message_text(receipt),
        "attachments":public_files(receipt)});
    if uuid(string(receipt, "codex_client_id")) {
        row["codex_client_id"] = receipt["codex_client_id"].clone();
    }
    Some(row)
}

pub(super) fn codex_queue_page(
    record: &Value,
    agent: &str,
    cursor: &str,
    limit: usize,
) -> Result<(Vec<Value>, Value)> {
    if record["agent"] != "codex" || !agent.is_empty() || !uuid(string(record, "conversation_id")) {
        return Err("Native queue history is unavailable for this target".into());
    }
    // The all-receipt index already includes text-only rows written by older
    // versions. Its cursor advances across non-queue receipts as well.
    let (catalog, mut page) = receipt_index::scoped_page(record, agent, cursor, limit, false)?;
    let mut rows = Vec::new();
    let mut gaps = Vec::new();
    for locator in &page.locators {
        match catalog.codex_queue_original(locator) {
            Ok(receipt) => rows.extend(codex_queue_receipt(record, agent, &receipt)),
            Err(reason) => gaps.push(json!({"request_id":locator.id,"state":reason})),
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    mark_native_queued(record, &mut rows);
    if !gaps.is_empty() {
        page.metadata["state"] = json!("partial");
    }
    page.metadata["gaps"] = json!(gaps);
    Ok((rows, page.metadata))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn mark_native_queued(record: &Value, rows: &mut [Value]) {
    if rows.is_empty() {
        return;
    }
    let Ok(account) = codex_tasks::history::HistoryRoot::open(record) else {
        return;
    };
    let ids: Vec<_> = rows
        .iter()
        .map(|row| string(row, "native_queued_message_id").to_owned())
        .collect();
    let Ok(clients) = codex_sqlite::queued_clients(
        account.path(),
        string(record, "conversation_id"),
        &ids,
        std::time::Instant::now() + std::time::Duration::from_millis(80),
    ) else {
        return;
    };
    if account.verify().is_err() {
        return;
    }
    for row in rows {
        let Some(client) = clients.get(string(row, "native_queued_message_id")) else {
            continue;
        };
        if row
            .get("codex_client_id")
            .is_none_or(|saved| saved == client.as_str())
        {
            row["codex_client_id"] = json!(client);
            row["native_queue_state"] = json!("queued");
        }
    }
}

pub(super) fn mark_native_consumed(rows: &mut [Value], bodies: &Value) {
    let mut owners = std::collections::HashMap::new();
    for row in rows.iter() {
        if let Some(client) = row["codex_client_id"].as_str() {
            *owners.entry(client.to_owned()).or_insert(0usize) += 1;
        }
    }
    for receipt in rows {
        let Some(client) = receipt["codex_client_id"].as_str() else {
            continue;
        };
        if owners.get(client) != Some(&1) {
            continue;
        }
        let matched = bodies
            .as_array()
            .into_iter()
            .flatten()
            .filter(|body| {
                body["source"] == "codex_transcript"
                    && body["type"] == "UserMessageBody"
                    && body["codex_client_id"] == client
                    && body["conversation_id"] == receipt["conversation_id"]
                    && string(body, "agent_id").is_empty()
                    && body.get("inherited_from").is_none()
            })
            .take(2)
            .count();
        if matched == 1 && receipt.get("native_queue_state").is_none() {
            receipt["native_queue_state"] = json!("consumed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_codex_queue_preserves_legacy_text_without_assigning_a_turn() {
        let thread = uuid::Uuid::new_v4().to_string();
        let request = uuid::Uuid::new_v4().to_string();
        let queue = uuid::Uuid::new_v4().to_string();
        let client = uuid::Uuid::new_v4().to_string();
        let record = json!({"agent":"codex","conversation_id":thread,"run_id":"run"});
        let original = json!({"status":"submitted","transport":"codex_queue","codex_outbox":true,
            "submitted_at":2.0,"request_id":request,"text":" Same! ","native_queued_message_id":queue,
            "conversation_id":thread,"run_id":"run","attachments":[]});
        let row = codex_queue_receipt(&record, "", &original).unwrap();
        assert_eq!(row["detail"], " Same! ");
        assert_eq!(row["request_id"], request);
        assert!(row.get("codex_client_id").is_none());
        for (key, value) in [
            ("run_id", json!("other")),
            ("conversation_id", json!(uuid::Uuid::new_v4())),
            ("status", json!("in_progress")),
            ("native_queued_message_id", json!("")),
            ("request_id", json!(uuid::Uuid::nil())),
            ("codex_outbox", json!(false)),
        ] {
            let mut invalid = original.clone();
            invalid[key] = value;
            assert!(
                codex_queue_receipt(&record, "", &invalid).is_none(),
                "{key}"
            );
        }
        assert!(codex_queue_receipt(&record, "other", &original).is_none());
        let body = json!({"type":"UserMessageBody","source":"codex_transcript","detail":" Same! ",
            "conversation_id":thread,"codex_client_id":client});
        let mut rows = vec![row];
        mark_native_consumed(&mut rows, &json!([body]));
        assert!(rows[0].get("native_queue_state").is_none());
        rows[0]["codex_client_id"] = json!(client);
        mark_native_consumed(&mut rows, &json!([body, body]));
        assert!(rows[0].get("native_queue_state").is_none());
        rows.push(rows[0].clone());
        mark_native_consumed(&mut rows, &json!([body]));
        assert!(rows
            .iter()
            .all(|row| row.get("native_queue_state").is_none()));
        rows.pop();
        mark_native_consumed(&mut rows, &json!([body]));
        assert_eq!(rows[0]["native_queue_state"], "consumed");
    }
}
