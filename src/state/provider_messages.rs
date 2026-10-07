//! Native assistant messages and Claude's recorded thinking from the exact
//! conversation. Hook Stop events alone miss progress between tools.
use super::*;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

pub(super) fn read(record: &Value) -> Result<Vec<Value>> {
    let agent = string(record, "agent");
    if agent == "kimi" {
        return kimi_messages::read(record);
    }
    if !["codex", "claude"].contains(&agent) {
        return Ok(vec![]);
    }
    let id = string(record, "conversation_id");
    if uuid::Uuid::parse_str(id).is_err() {
        return Ok(vec![]);
    }
    let path = PathBuf::from(string(record, "transcript"));
    if !path.is_absolute()
        || !path
            .file_name()
            .is_some_and(|v| v.to_string_lossy().contains(id))
    {
        return Ok(vec![]);
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if agent == "codex" {
        let mut first = String::new();
        BufReader::new((&mut file).take(256 * 1024))
            .read_line(&mut first)
            .map_err(|e| e.to_string())?;
        let first: Value = serde_json::from_str(&first).map_err(|e| e.to_string())?;
        if first["type"] != "session_meta" || first["payload"]["id"] != id {
            return Err("Conversation transcript identity mismatch".into());
        }
    }
    let offset = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len()
        .saturating_sub(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let mut messages: Vec<Value> = Vec::new();
    for (index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        if (offset > 0 && index == 0) || line.len() > 1024 * 1024 || !line.ends_with(b"\n") {
            continue;
        }
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if agent == "claude" && !string(&event, "sessionId").is_empty() && event["sessionId"] != id
        {
            continue;
        }
        if agent == "claude" {
            if let Some(thinking) = claude_thinking(&event) {
                messages.push(thinking);
            }
        }
        let Some((message, role)) = search::provider_event(agent, &event) else {
            continue;
        };
        if role != "assistant" {
            continue;
        }
        // Codex can emit both response_item and event_msg for the same public
        // message. Keep genuinely repeated later messages in the conversation.
        if messages.iter().rev().take(4).any(|m| {
            m["type"] == message["type"]
                && m["detail"] == message["detail"]
                && (m["at"].as_f64().unwrap_or(0.0) - message["at"].as_f64().unwrap_or(0.0)).abs()
                    < 3.0
        }) {
            continue;
        }
        messages.push(message);
    }
    messages.sort_by(|a,b|a["at"].as_f64().unwrap_or(0.).total_cmp(&b["at"].as_f64().unwrap_or(0.)));
    if messages.len() > 100 {
        messages.drain(..messages.len() - 100);
    }
    Ok(messages)
}

// Claude records some terminal-visible progress in thinking blocks. Keep it
// separate from replies; signatures and redacted blocks are never UI content.
fn claude_thinking(event: &Value) -> Option<Value> {
    if event["type"] != "assistant"
        || event["message"]["role"] != "assistant"
        || event["isMeta"] == true
        || event["isSidechain"] == true
    {
        return None;
    }
    let text = event["message"]["content"]
        .as_array()?
        .iter()
        .filter(|part| part["type"] == "thinking")
        .filter_map(|part| part["thinking"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return None;
    }
    Some(json!({"type":"AgentThinking","source":"claude_transcript",
        "at":search::timestamp(&event["timestamp"]),
        "message_id":format!("{}:thinking",string(event,"uuid")),"agent_id":"",
        "detail":journal::clipped(&json!(text),32_000)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn public_progress_and_claude_thinking_keep_separate_roles_without_private_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = temp.path().join(format!("rollout-{id}.jsonl"));
        let at = "2026-10-04T12:00:00Z";
        let rows = [
            json!({"type":"session_meta","payload":{"id":id}}),
            json!({"type":"response_item","timestamp":at,"payload":{"type":"message","role":"assistant","channel":"commentary","content":[{"type":"output_text","text":"Checking the deployment."}]}}),
            json!({"type":"event_msg","timestamp":at,"payload":{"type":"agent_message","message":"Checking the deployment."}}),
            json!({"type":"response_item","timestamp":at,"payload":{"type":"message","role":"assistant","channel":"analysis","content":[{"type":"output_text","text":"PRIVATE_REASONING"}]}}),
            json!({"type":"event_msg","timestamp":at,"payload":{"type":"agent_message","phase":"analysis","message":"PRIVATE_REASONING"}}),
        ];
        fs::write(
            &path,
            rows.iter()
                .map(|v| v.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let record = json!({"agent":"codex","conversation_id":id,"transcript":path});
        let messages = read(&record).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["detail"], "Checking the deployment.");
        assert_eq!(messages[0]["message_phase"], "commentary");
        let native = [
            json!({"type":"session_meta","payload":{"id":id}}),
            json!({"type":"event_msg","timestamp":at,"payload":{"type":"item_completed","item":{"type":"AgentMessage","id":"progress","phase":"commentary","content":[{"type":"text","text":"Progress from current Codex."}]}}}),
            json!({"type":"response_item","timestamp":at,"payload":{"type":"message","id":"progress","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Progress from current Codex."}]}}),
            json!({"type":"response_item","timestamp":at,"payload":{"type":"message","role":"assistant","phase":"analysis","content":[{"type":"output_text","text":"PRIVATE_REASONING"}]}}),
            json!({"type":"response_item","timestamp":"2026-10-04T12:00:10Z","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Progress from current Codex."}]}}),
        ];
        fs::write(&path,native.iter().map(|v|v.to_string()+"\n").collect::<String>()).unwrap();
        let messages=read(&record).unwrap();
        assert_eq!(messages.len(),2);
        assert_eq!(messages[0]["message_id"],"progress");
        assert_eq!(messages[0]["message_phase"],"commentary");
        assert_eq!(messages[1]["message_phase"],"commentary");
        assert_eq!(messages[0]["detail"],messages[1]["detail"]);
        fs::write(
            &path,
            json!({"type":"session_meta","payload":{"id":"wrong"}}).to_string() + "\n",
        )
        .unwrap();
        assert!(read(&record).is_err());
        let claude = json!({"type":"assistant","uuid":"msg-one","timestamp":at,"sessionId":id,"message":{"role":"assistant","content":[{"type":"text","text":"Still working."},{"type":"thinking","thinking":"Read all 24 pages.","signature":"PRIVATE_SIGNATURE"},{"type":"redacted_thinking","data":"REDACTED"}]}});
        let mut other = claude.clone();
        other["sessionId"] = json!("another-conversation");
        fs::write(&path, claude.to_string() + "\n" + &other.to_string() + "\n").unwrap();
        let messages =
            read(&json!({"agent":"claude","conversation_id":id,"transcript":path})).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["type"], "AgentThinking");
        assert_eq!(messages[0]["detail"], "Read all 24 pages.");
        assert_eq!(messages[1]["type"], "AgentMessage");
        assert_eq!(messages[1]["detail"], "Still working.");
        assert_ne!(messages[0]["message_id"], messages[1]["message_id"]);
        assert!(!serde_json::to_string(&messages)
            .unwrap()
            .contains("PRIVATE_SIGNATURE"));
        assert!(!serde_json::to_string(&messages)
            .unwrap()
            .contains("REDACTED"));
        for flag in ["isMeta", "isSidechain"] {
            let mut hidden = claude.clone();
            hidden[flag] = json!(true);
            assert!(claude_thinking(&hidden).is_none());
        }
    }
}
