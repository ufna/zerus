//! Agent CLIs invoked by another provider inherit the HGS run token. Their
//! SessionStart must never rebind the parent conversation.
use super::*;
use std::io::{BufRead, BufReader};

// An inherited HGS token is not proof that a new same-provider conversation
// belongs to the interactive parent. Codex records its headless entry point in
// the rollout header; /new and /clear still use the interactive source.
fn codex_exec(event: &Value) -> bool {
    let path = Path::new(string(event, "transcript_path"));
    if !path.is_absolute() { return false; }
    let Ok(file) = std::fs::File::open(path) else { return false; };
    let mut line = String::new();
    if BufReader::new(file.take(256 * 1024)).read_line(&mut line).is_err() { return false; }
    let Ok(meta) = serde_json::from_str::<Value>(&line) else { return false; };
    meta["type"] == "session_meta" && meta["payload"]["id"] == event["session_id"]
        && meta["payload"]["source"] == "exec"
}

fn provider(event: &Value) -> Option<&'static str> {
    let id = string(event, "session_id");
    if id
        .strip_prefix("session_")
        .is_some_and(|v| uuid::Uuid::parse_str(v).is_ok())
    {
        return Some("kimi");
    }
    let path = Path::new(string(event, "transcript_path"));
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"))
    {
        return Some("codex");
    }
    let claude = nonempty_env("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"));
    if path.is_absolute() && path.starts_with(claude) && uuid::Uuid::parse_str(id).is_ok() {
        return Some("claude");
    }
    None
}

/// Return a child-scoped event, without touching parent identity, model, or
/// native transcript. Unknown foreign conversations cannot claim a child.
pub(super) fn route(record: &mut Value, event: &Value) -> Option<Value> {
    let sid = string(event, "session_id");
    if sid.is_empty()
        || sid.len() > 100
        || sid.contains(['/', '\\'])
        || sid.chars().any(char::is_control)
        || string(record, "conversation_id").is_empty()
        || sid == string(record, "conversation_id")
    {
        return None;
    }
    let known = record["subagents"].as_object().and_then(|children| {
        children
            .iter()
            .find(|(_, child)| child["external"] == true && child["conversation_id"] == sid)
            .map(|(id, _)| id.clone())
    });
    let id = if let Some(id) = known {
        id
    } else {
        if string(event, "hook_event_name") != "SessionStart" {
            return None;
        }
        let agent = provider(event)?;
        if agent == string(record, "agent") && !(agent == "codex" && codex_exec(event)) {
            return None;
        }
        let id = format!("external:{agent}:{sid}");
        let agent_home = nonempty_env(home_var(agent).ok()?)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home().join(if agent == "kimi" {
                    ".kimi-code"
                } else if agent == "codex" {
                    ".codex"
                } else {
                    ".claude"
                })
            });
        if !record["subagents"].is_object() {
            record["subagents"] = json!({});
        }
        record["subagents"][&id] = json!({"external":true,"provider":agent,"conversation_id":sid,
            "name":format!("{} {}",agent,sid.trim_start_matches("session_").chars().take(8).collect::<String>()),
            "started":now(),"transcript":event["transcript_path"],"cwd":event["cwd"],"agent_home":agent_home});
        id
    };
    let child = &mut record["subagents"][&id];
    for (from, to) in [
        ("cwd", "cwd"),
        ("transcript_path", "transcript"),
        ("model", "model"),
        ("reasoning_effort", "effort"),
        ("effort", "effort"),
    ] {
        if !string(event, from).is_empty() {
            child[to] = event[from].clone();
        }
    }
    let mut scoped = event.clone();
    scoped["agent_id"] = json!(id);
    // Keep the registered label: a native agent_type such as "default" belongs
    // to the foreign harness, not to the parent provider's profile registry.
    scoped["agent_type"] = child["name"].clone();
    Some(scoped)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_codex_exec_cannot_replace_the_interactive_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let sid = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("rollout-{sid}.jsonl"));
        let mut meta = json!({"type":"session_meta","payload":{"id":sid,"source":"exec"}});
        std::fs::write(&path,meta.to_string()+"\n").unwrap();
        let parent = json!({"agent":"codex","conversation_id":"parent","transcript":"parent-rollout","model":"parent-model","subagents":{}});
        let event = json!({"hook_event_name":"SessionStart","session_id":sid,"transcript_path":path,"cwd":"/child","model":"child-model"});
        let mut record = parent.clone();
        let scoped = route(&mut record,&event).unwrap();
        let child = string(&scoped,"agent_id");
        assert_eq!(record["conversation_id"],"parent");
        assert_eq!(record["transcript"],"parent-rollout");
        assert_eq!(record["model"],"parent-model");
        assert_eq!(record["subagents"][child]["provider"],"codex");
        let stop = json!({"hook_event_name":"Stop","session_id":sid,"last_assistant_message":"child finished"});
        let scoped = route(&mut record,&stop).unwrap();
        journal::update_activity(&mut record,&scoped);
        assert_eq!(record["subagents"][child]["state"],"finished");
        assert_eq!(record["conversation_id"],"parent");
        // A real /new still replaces the parent's conversation. Neither an
        // unrelated header nor a first headless launch can claim a child.
        for source in ["cli","vscode"] {
            meta["payload"]["source"]=json!(source);
            std::fs::write(&path,meta.to_string()+"\n").unwrap();
            assert!(route(&mut parent.clone(),&event).is_none());
        }
        meta["payload"]["source"]=json!("exec");
        meta["payload"]["id"]=json!("wrong");
        std::fs::write(&path,meta.to_string()+"\n").unwrap();
        assert!(route(&mut parent.clone(),&event).is_none());
        meta["payload"]["id"]=json!(sid);
        std::fs::write(&path,meta.to_string()+"\n").unwrap();
        let mut fresh=parent.clone();fresh["conversation_id"]=Value::Null;
        assert!(route(&mut fresh,&event).is_none());
    }
    #[test]
    fn mixed_children_keep_parent_binding_model_and_history() {
        for (parent, child, sid, path) in [
            (
                "kimi",
                "codex",
                "11111111-1111-4111-8111-111111111111",
                "/tmp/rollout-11111111-1111-4111-8111-111111111111.jsonl",
            ),
            (
                "codex",
                "kimi",
                "session_22222222-2222-4222-8222-222222222222",
                "/tmp/kimi-session",
            ),
        ] {
            let mut record = json!({"agent":parent,"conversation_id":"parent-id","model":"parent-model","transcript":"parent-path","main_done":false,"subagents":{}});
            let mut event = json!({"hook_event_name":"SessionStart","session_id":sid,"transcript_path":path,"cwd":"/tmp/test","model":"child-model"});
            let scoped = route(&mut record, &event).unwrap();
            let id = string(&scoped, "agent_id").to_owned();
            journal::update_activity(&mut record, &scoped);
            assert_eq!(record["conversation_id"], "parent-id");
            assert_eq!(record["transcript"], "parent-path");
            assert_eq!(record["model"], "parent-model");
            assert_eq!(record["subagents"][&id]["provider"], child);
            event["hook_event_name"] = json!("Stop");
            event["last_assistant_message"] = json!("child done");
            let scoped = route(&mut record, &event).unwrap();
            journal::update_activity(&mut record, &scoped);
            assert_eq!(record["subagents"][&id]["state"], "finished");
            assert_eq!(record["subagents"][&id]["detail"], "child done");
            assert_eq!(record["model"], "parent-model");
            assert_eq!(record["main_done"], false);
        }
    }
    #[test]
    fn same_provider_new_conversation_and_unknown_child_are_not_routed() {
        let mut record = json!({"agent":"codex","conversation_id":"parent","subagents":{}});
        assert!(route(&mut record,&json!({"hook_event_name":"SessionStart","session_id":"new","transcript_path":"/tmp/rollout-new.jsonl"})).is_none());
        assert!(route(
            &mut record,
            &json!({"hook_event_name":"Stop","session_id":"unknown"})
        )
        .is_none());
        assert!(route(
            &mut record,
            &json!({"hook_event_name":"SessionStart","session_id":"session_../../other"})
        )
        .is_none());
        assert_eq!(record["subagents"], json!({}));
    }
}
