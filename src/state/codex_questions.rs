//! Optional Codex questions are asynchronous messages, not blocking tool calls.
//! Preserve them independently of tool completion and acknowledge exact native
//! questionItemIds. Skip is a local dismissal and never fabricates a user answer.
use super::question_terminal::{Native, Transport};
use super::*;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};

fn cache_path(record: &Value) -> Result<PathBuf> {
    Ok(absolute_root()?.join("codex_questions").join(format!(
        "{:x}.json",
        Sha256::digest(json!([record["agent_home"], record["conversation_id"]]).to_string())
    )))
}
fn parse_reply(text: &str) -> Vec<Value> {
    let text = text.trim();
    let text = if text.starts_with("# Context from my IDE setup:\n") {
        let Some((_, request)) = text.rsplit_once("\n## My request for Codex:\n") else {
            return vec![];
        };
        request.trim()
    } else {
        text
    };
    let Some(body) = text
        .strip_prefix("<send_user_message_question_reply>")
        .and_then(|s| s.strip_suffix("</send_user_message_question_reply>"))
    else {
        return vec![];
    };
    match serde_json::from_str::<Value>(body.trim()) {
        Ok(Value::Array(a)) => a,
        Ok(v @ Value::Object(_)) => vec![v],
        _ => vec![],
    }
}
fn observe(record: &Value, cache: &mut Value, event: &Value) {
    let payload = &event["payload"];
    if event["type"] == "response_item"
        && payload["type"] == "function_call"
        && matches!(
            string(payload, "name"),
            "request_user_input_async" | "functions.request_user_input_async"
        )
    {
        let call = string(payload, "call_id");
        if call.is_empty() || call.len() > 200 {
            return;
        }
        let Ok(args) = serde_json::from_str::<Value>(string(payload, "arguments")) else {
            return;
        };
        let Some(items) = args["questions"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 16)
        else {
            return;
        };
        for (index, item) in items.iter().enumerate() {
            let title = string(item, "title");
            if title.trim().is_empty()
                || title.len() > 8192
                || title
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                continue;
            }
            let options = match item.get("options") {
                None | Some(Value::Null) => vec![],
                Some(Value::Array(a)) if a.len() <= 20 => a.clone(),
                _ => continue,
            };
            if options.iter().any(|o| {
                o.as_str().is_none_or(|s| {
                    s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control)
                })
            }) {
                continue;
            }
            let native_id = json!(["request_user_input_async", call, index]).to_string();
            let id = format!("codex-async:{call}:{index}");
            if !cache["items"][&id].is_null() {
                continue;
            }
            let choices: Vec<_> = options
                .iter()
                .enumerate()
                .map(|(i, o)| json!({"id":format!("opt_{i}"),"label":o,"description":""}))
                .collect();
            let mut card = json!({"question_id":id,"tool_call_id":call,"native_question_id":native_id,
                "run_id":record["run_id"],"conversation_id":record["conversation_id"],"source":"codex_async",
                "created_at":search::timestamp(&event["timestamp"]),"optional":true,"can_skip":true,"can_answer":false,
                "questions":[{"id":"q_0","question":title,"options":choices,"allow_other":true,"required":false,"multi_select":false}]});
            card["question_hash"] = json!(questions::fingerprint(&card));
            cache["items"][id] = card;
        }
    }
    if event["type"] == "response_item" && payload["type"] == "function_call_output" {
        if let Some(items) = cache["items"].as_object_mut() {
            let accepted = serde_json::from_str::<Value>(string(payload, "output"))
                .ok()
                .is_some_and(|v| v["accepted"] == true);
            for card in items
                .values_mut()
                .filter(|c| c["tool_call_id"] == payload["call_id"])
            {
                card["accepted"] = json!(accepted);
            }
        }
    }
    let texts: Vec<&str> = if event["type"] == "event_msg" && payload["type"] == "user_message" {
        vec![string(payload, "message")]
    } else if event["type"] == "response_item"
        && payload["type"] == "message"
        && payload["role"] == "user"
    {
        payload["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["text"].as_str())
            .collect()
    } else {
        vec![]
    };
    for reply in texts.into_iter().flat_map(parse_reply) {
        if let Some(items) = cache["items"].as_object_mut() {
            for card in items.values_mut() {
                if reply["questionItemId"] == card["native_question_id"]
                    && reply["question"] == card["questions"][0]["question"]
                    && reply["answer"].is_string()
                {
                    card["response"] = json!({"type":"interaction.resolved","time":search::timestamp(&event["timestamp"])*1000.,
                    "response":{"answers":{string(&reply,"question"):reply["answer"]}}});
                }
            }
        }
    }
}
fn read_cache(record: &Value) -> Result<Value> {
    let path = PathBuf::from(string(record, "transcript"));
    let id = string(record, "conversation_id");
    if uuid::Uuid::parse_str(id).is_err()
        || string(record, "run_id").is_empty()
        || !path.is_absolute()
        || !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains(id))
    {
        return Ok(json!({"items":{}}));
    }
    let cache_path = cache_path(record)?;
    let _guard = lock(Some(&cache_path.with_extension("lock")))?;
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .by_ref()
        .take(256 * 1024)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let header: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if header["type"] != "session_meta" || header["payload"]["id"] != id {
        return Err("Conversation transcript identity mismatch".into());
    }
    let mut cache: Value = fs::read_to_string(&cache_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({"items":{}}));
    let inode = json!([meta.dev(), meta.ino()]);
    let offset = cache["offset"].as_u64().unwrap_or(0);
    if cache["inode"] != inode || offset > meta.len() {
        cache = json!({"items":{},"offset":0,"inode":inode});
    }
    // Questions can stay unanswered long after they leave the recent-message
    // tail. Index the entire exact conversation once, then only appended bytes.
    // Reindex old tail-only caches while retaining explicit local dismissals.
    if cache["scan_version"] != 2 {
        cache["offset"] = json!(0);
        cache["scan_version"] = json!(2);
    }
    let offset = cache["offset"].as_u64().unwrap_or(0);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    // Initial tail can begin inside a JSON line; never interpret that fragment.
    if cache.get("initialized").is_none() && offset > 0 {
        line.clear();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
    }
    loop {
        let before = reader.stream_position().map_err(|e| e.to_string())?;
        line.clear();
        let count = reader
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if !line.ends_with('\n') {
            if count > 1024 * 1024 {
                reader.skip_until(b'\n').map_err(|e| e.to_string())?;
                cache["offset"] = json!(reader.stream_position().map_err(|e| e.to_string())?);
                continue;
            }
            cache["offset"] = json!(before);
            break;
        }
        cache["offset"] = json!(reader.stream_position().map_err(|e| e.to_string())?);
        if count <= 1024 * 1024
            && (line.contains("request_user_input_async")
                || line.contains("function_call_output")
                || line.contains("send_user_message_question_reply"))
        {
            if let Ok(event) = serde_json::from_str(&line) {
                observe(record, &mut cache, &event);
            }
        }
    }
    cache["initialized"] = json!(true);
    let serialized = cache.to_string();
    if fs::read_to_string(&cache_path).ok().as_deref() != Some(&serialized) {
        atomic(&cache_path, &serialized)?;
    }
    Ok(cache)
}
pub(super) fn current(record: &Value) -> Result<Vec<Value>> {
    if string(record, "agent") != "codex" {
        return Ok(vec![]);
    }
    Ok(read_cache(record)?["items"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter(|c| c["accepted"] == true && c["response"].is_null() && c["skipped"] != true)
        .cloned()
        .map(|mut card| {
            card["run_id"] = record["run_id"].clone();
            card["question_hash"] = json!(questions::fingerprint(&card));
            card
        })
        .collect())
}
pub(super) fn messages(record: &Value) -> Result<Vec<Value>> {
    if string(record, "agent") != "codex" {
        return Ok(vec![]);
    }
    let cache = read_cache(record)?;
    Ok(cache["items"].as_object().into_iter().flat_map(|m|m.values()).filter(|c|c["accepted"]==true).map(|card|{
        let question=&card["questions"][0];
        let choices=question["options"].as_array().unwrap().iter().map(|o|format!("\n- {}",string(o,"label"))).collect::<String>();
        let status=if card["skipped"]==true {"Skipped"}else if !card["response"].is_null(){"Answered"}else{"Optional"};
        json!({"type":"AgentMessage","source":"codex_transcript","message_id":card["question_id"],"agent_id":"",
            "at":card["created_at"],"message_phase":"commentary","detail":format!("{}\n{}{}",status,string(question,"question"),choices)})
    }).collect())
}
fn unambiguous(record: &Value, card: &Value) -> Result<bool> {
    Ok(current(record)?
        .iter()
        .filter(|c| c["questions"] == card["questions"])
        .count()
        == 1)
}
pub(super) fn response(record: &Value, card: &Value) -> Result<Option<Value>> {
    let cache = read_cache(record)?;
    let response = &cache["items"][string(card, "question_id")]["response"];
    Ok((!response.is_null()).then(|| response.clone()))
}
fn panel_body(screen: &str, card: &Value) -> Result<Option<String>> {
    let plain = question_terminal::plain(screen)?;
    let lines: Vec<_> = plain.lines().collect();
    let Some(footer) = lines
        .iter()
        .rposition(|line| line.contains("enter submit") && line.contains("ctrl+] skip"))
    else {
        return Ok(None);
    };
    let Some(body_end) = (0..footer).rev().find(|i| !lines[*i].trim().is_empty()) else {
        return Ok(None);
    };
    let Some(separator) = (0..body_end).rev().find(|i| lines[*i].trim().is_empty()) else {
        return Ok(None);
    };
    let Some(title_end) = (0..separator).rev().find(|i| !lines[*i].trim().is_empty()) else {
        return Ok(None);
    };
    let title_start = (0..title_end)
        .rev()
        .find(|i| lines[*i].trim().is_empty())
        .map_or(0, |i| i + 1);
    let compact = |s: &str| s.split_whitespace().collect::<String>();
    if compact(&lines[title_start..=title_end].join("\n"))
        != compact(string(&card["questions"][0], "question"))
    {
        return Ok(None);
    }
    let body = lines[separator + 1..=body_end].join("\n");
    let options = card["questions"][0]["options"]
        .as_array()
        .ok_or("missing options")?;
    if !options
        .iter()
        .all(|o| compact(&body).contains(&compact(string(o, "label"))))
    {
        return Ok(None);
    }
    Ok(Some(body))
}
fn visible_panel(screen: &str, card: &Value) -> Result<bool> {
    Ok(panel_body(screen, card)?.is_some())
}
fn panel_empty(screen: &str, card: &Value) -> Result<bool> {
    let Some(body) = panel_body(screen, card)? else {
        return Ok(false);
    };
    let options = card["questions"][0]["options"].as_array().unwrap();
    if options.is_empty() {
        return Ok(body.trim() == "Type your answer");
    }
    let index = options.len() + 1;
    // An unselected Other row displays its saved draft instead of the label.
    // Never overwrite either a visible draft or one stored behind a selection.
    Ok(body.lines().last().is_some_and(|line| {
        let row = line.trim().trim_start_matches('›').trim();
        row == format!("{index}. Other") || row == format!("{index}. Other (write an answer)")
    }))
}
pub(super) fn available(record: &Value, card: &Value) -> Result<()> {
    let mut terminal = Native::new(record, card);
    let screen = terminal.screen()?;
    if visible_panel(&screen, card)? {
        if !unambiguous(record, card)? {
            return Err(
                "Several pending questions have the same text. Choose one in Terminal.".into(),
            );
        }
        if !panel_empty(&screen, card)? {
            return Err("This question has a draft in Terminal. Finish or clear it there.".into());
        }
        return Ok(());
    }
    input::checked_terminal(record, true)
}
fn answer_text(card: &Value, answer: &Value) -> Result<String> {
    if let Some(id) = answer["selected_option_ids"]
        .as_array()
        .and_then(|a| a.first())
    {
        card["questions"][0]["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == *id)
            .map(|o| string(o, "label").to_owned())
            .ok_or("unknown answer option".into())
    } else {
        Ok(string(answer, "text").to_owned())
    }
}
fn envelope(card: &Value, text: &str) -> String {
    format!(
        "<send_user_message_question_reply>\n{}\n</send_user_message_question_reply>",
        json!([{"questionItemId":card["native_question_id"],"question":card["questions"][0]["question"],"answer":text}])
    )
}
pub(super) fn answer(
    record: &Value,
    card: &Value,
    answers: &Value,
    request_id: &str,
) -> Result<()> {
    let mut terminal = Native::new(record, card);
    let screen = terminal.screen()?;
    if answers[0]["skip"] == true {
        if visible_panel(&screen, card)? && unambiguous(record, card)? {
            terminal.key("C-]")?;
        }
        let path = cache_path(record)?;
        let _guard = lock(Some(&path.with_extension("lock")))?;
        let mut cache: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        cache["items"][string(card, "question_id")]["skipped"] = json!(true);
        atomic(&path, &cache.to_string()).map_err(|e| {
            if terminal.touched() {
                format!("delivery uncertain: {e}")
            } else {
                e
            }
        })?;
        return Ok(());
    }
    available(record, card)?;
    let text = answer_text(card, &answers[0])?;
    if visible_panel(&screen, card)? {
        // Native paste selects Other and submits its own exact question envelope.
        let result: Result<()> = (|| {
            terminal.paste(&text)?;
            std::thread::sleep(Duration::from_millis(180));
            let screen = terminal.screen()?;
            if !visible_panel(&screen, card)? || !question_terminal::plain(&screen)?.contains(&text)
            {
                return Err("question or answer draft changed before submission".into());
            }
            terminal.key("Enter")
        })();
        if let Err(error) = result {
            return Err(if terminal.touched() {
                format!("delivery uncertain: {error}")
            } else {
                error
            });
        }
    } else {
        let payload = json!({"request_id":request_id,"text":envelope(card,&text),"expected_run_id":record["run_id"],"expected_conversation_id":record["conversation_id"]});
        input::submit(string(record, "name"), payload.to_string().as_bytes(), None)?;
    }
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(response) =
            response(record, card).map_err(|e| format!("delivery uncertain: {e}"))?
        {
            return if response["response"]["answers"][string(&card["questions"][0], "question")]
                == text
            {
                Ok(())
            } else {
                Err("delivery uncertain: question received a different answer".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("delivery uncertain: Codex has not recorded the answer; inspect Terminal before retrying".into());
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Value {
        json!({"type":"response_item","timestamp":"2026-10-07T15:00:00Z","payload":{"type":"function_call","name":"request_user_input_async","call_id":"call_one","arguments":json!({"questions":[{"title":"Which option?","options":["One","Two"]},{"title":"Any notes?"}]}).to_string()}})
    }
    #[test]
    fn accepted_is_not_answered_and_each_optional_question_has_an_exact_identity() {
        let record = json!({"run_id":"run","conversation_id":"conv"});
        let mut cache = json!({"items":{}});
        observe(&record, &mut cache, &request());
        assert_eq!(cache["items"].as_object().unwrap().len(), 2);
        observe(
            &record,
            &mut cache,
            &json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call_one","output":"{\"accepted\":true}"}}),
        );
        let card = cache["items"]["codex-async:call_one:0"].clone();
        assert!(card["response"].is_null());
        assert_eq!(card["optional"], true);
        let reply = envelope(&card, "One");
        assert_eq!(
            parse_reply(&reply)[0]["questionItemId"],
            "[\"request_user_input_async\",\"call_one\",0]"
        );
        let wrong = reply.replace("call_one", "call_other");
        observe(
            &record,
            &mut cache,
            &json!({"type":"event_msg","payload":{"type":"user_message","message":wrong}}),
        );
        assert!(cache["items"]["codex-async:call_one:0"]["response"].is_null());
        observe(
            &record,
            &mut cache,
            &json!({"type":"event_msg","payload":{"type":"user_message","message":reply}}),
        );
        assert!(!cache["items"]["codex-async:call_one:0"]["response"].is_null());
        assert!(cache["items"]["codex-async:call_one:1"]["response"].is_null());
        observe(&record, &mut cache, &request());
        assert!(!cache["items"]["codex-async:call_one:0"]["response"].is_null());
        assert_eq!(
            cache["items"]["codex-async:call_one:1"]["questions"][0]["options"],
            json!([])
        );
    }
    #[test]
    fn only_complete_native_question_panels_accept_input() {
        let mut cache = json!({"items":{}});
        observe(&json!({}), &mut cache, &request());
        let card = &cache["items"]["codex-async:call_one:0"];
        let screen="Which option?\n\n› 1. One\n  2. Two\n  3. Other (write an answer)\nenter submit   ctrl+] skip\n";
        assert!(visible_panel(screen, card).unwrap());
        assert!(panel_empty(screen, card).unwrap());
        assert!(!panel_empty(
            &screen.replace("Other (write an answer)", "existing draft"),
            card
        )
        .unwrap());
        let freeform = &cache["items"]["codex-async:call_one:1"];
        assert!(panel_empty(
            "Any notes?\n\nType your answer\n\nenter submit   ctrl+] skip",
            freeform
        )
        .unwrap());
        assert!(!panel_empty(
            "Any notes?\n\nMy existing draft\n\nenter submit   ctrl+] skip",
            freeform
        )
        .unwrap());
        for wrong in [
            screen.replace("Which option?", "Different?"),
            screen.replace("Two", "Hidden"),
            screen.replace("ctrl+] skip", "esc cancel"),
        ] {
            assert!(!visible_panel(&wrong, card).unwrap());
        }
    }
}
