//! Native provider failures, separate from ordinary tool failures and prose.
use super::*;
use rusqlite::{Connection, OpenFlags};

pub(super) fn category(text: &str) -> &'static str {
    let text = text.to_lowercase();
    if ["capacity", "overload", "model_capacity"]
        .iter()
        .any(|s| text.contains(s))
    {
        "capacity"
    } else if [
        "quota",
        "usage limit",
        "usage_limit",
        "insufficient",
        "billing",
        "credit",
        "account_on_hold",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "quota"
    } else if ["rate_limit", "rate limit", "too many requests", "429"]
        .iter()
        .any(|s| text.contains(s))
    {
        "rate_limit"
    } else if [
        "auth",
        "unauthorized",
        "credential",
        "api key",
        "api_key",
        "oauth",
        "401",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "authentication"
    } else if ["context", "max_output_tokens", "max-tokens", "token limit"]
        .iter()
        .any(|s| text.contains(s))
    {
        "context_limit"
    } else if ["model_not_found", "model not found", "does not exist"]
        .iter()
        .any(|s| text.contains(s))
    {
        "model_unavailable"
    } else {
        "provider"
    }
}

pub(super) fn event(id: &str, at: f64, detail: &str, source: &str) -> Value {
    let detail = journal::clipped(&json!(detail), 1200);
    json!({"type":"StopFailure","source":source,"message_id":id,"agent_id":"","at":at,
        "detail":detail,"error_kind":category(&detail)})
}

pub(super) fn apply(output: &mut Value, error: &Value) {
    output["phase"] = json!("error");
    output["activity"] = json!("attention");
    output["last_error"] = error["detail"].clone();
    output["activity_summary"] = json!(match string(error, "error_kind") {
        "capacity" => "Model at capacity",
        "quota" => "Usage limit reached",
        "rate_limit" => "Rate limit reached",
        "authentication" => "Sign-in failed",
        "context_limit" => "Context or output limit reached",
        "model_unavailable" => "Model unavailable",
        _ => "Provider error",
    });
    output["activity_detail"] = error["detail"].clone();
    output["provider_error"] = error.clone();
    output["attention_id"] = json!(format!(
        "{}:{}",
        string(error, "source"),
        string(error, "message_id")
    ));
    output["current_tool"] = json!("");
    output["tool_detail"] = json!("");
}

fn codex(record: &Value) -> Result<Vec<Value>> {
    let conversation = string(record, "conversation_id");
    if uuid::Uuid::parse_str(conversation).is_err() {
        return Ok(vec![]);
    }
    let native_home = string(record, "agent_home");
    let native_home = if native_home.is_empty() {
        home().join(".codex")
    } else if Path::new(native_home).is_absolute() {
        PathBuf::from(native_home)
    } else {
        PathBuf::from(string(record, "launch_dir")).join(native_home)
    };
    let db = Connection::open_with_flags(
        native_home.join("logs_2.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    // Codex 0.160 omits terminal Turn errors from rollouts and hooks. Its own
    // indexed journal supplies the conversation and error emitter explicitly.
    let mut query = db
        .prepare(
            "SELECT id, ts, ts_nanos, feedback_log_body FROM logs
        WHERE thread_id = ? AND target IN ('codex_core::session::turn','codex_core::codex')
        AND feedback_log_body LIKE '%: Turn error: %' ORDER BY ts DESC, ts_nanos DESC LIMIT 8",
        )
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([conversation], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for row in rows.flatten() {
        if let Some((_, detail)) = row.3.rsplit_once(": Turn error: ") {
            if !detail.trim().is_empty() {
                errors.push(event(
                    &format!("codex-error-{}", row.0),
                    row.1 as f64 + row.2 as f64 / 1e9,
                    detail,
                    "codex_native_log",
                ));
            }
        }
    }
    errors.reverse();
    Ok(errors)
}

pub(super) fn enrich(record: &Value, output: &mut Value, live: bool) {
    let errors = if record["agent"] == "codex" {
        codex(record).unwrap_or_default()
    } else {
        vec![]
    };
    let progress = record["last_main_progress_at"].as_f64().unwrap_or_else(|| {
        // Existing sessions predate the dedicated root-progress field. Child
        // hooks and heartbeats must not acknowledge a failed parent turn.
        if errors.is_empty() { return 0.0; }
        use rusqlite::OptionalExtension;
        journal::event_db().ok().and_then(|db|db.query_row(
            "SELECT payload FROM events WHERE name=? AND conversation=?
             AND json_extract(payload,'$.agent_id')='' AND json_extract(payload,'$.type') IN
             ('SessionStart','UserPromptSubmit','UserPromptQueued','TurnStarted','TaskStarted','PreToolUse','PostToolUse','PostToolUseFailure','PermissionRequest','PermissionResult','PreCompact','PostCompact','Stop','Interrupt')
             ORDER BY seq DESC LIMIT 1",
             rusqlite::params![journal::journal_name(record),string(record,"conversation_id")],|r|r.get::<_,String>(0))
             .optional().ok().flatten()).and_then(|s|serde_json::from_str::<Value>(&s).ok())
             .and_then(|event|event["at"].as_f64()).unwrap_or(0.0)
    })
        .max(record["turn_started"].as_f64().unwrap_or(0.0));
    let error = errors
        .last()
        .filter(|e| e["at"].as_f64().unwrap_or(0.0) > progress)
        .or_else(|| {
            record["provider_error"]
                .is_object()
                .then_some(&record["provider_error"])
        });
    if live && output["process_state"] != "exited" {
        if let Some(error) = error {
            apply(output, error);
        }
    }
    output["provider_errors"] = json!(errors);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_errors_are_conversation_scoped_and_clear_on_main_progress() {
        let temp = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let db = Connection::open(temp.path().join("logs_2.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE logs(id INTEGER, ts INTEGER, ts_nanos INTEGER, thread_id TEXT, target TEXT, feedback_log_body TEXT);").unwrap();
        for (n,thread,target,body) in [(1,id.as_str(),"codex_core::session::turn","run_turn: Turn error: Selected model is at capacity. Please try a different model."),
            (2,"other","codex_core::session::turn","run_turn: Turn error: quota exhausted"),
            (3,id.as_str(),"codex_core::tools","run_turn: Turn error: tool output text"),
            (4,id.as_str(),"codex_core::session::turn","A regular status message")] {
            db.execute("INSERT INTO logs VALUES(?,100,500000000,?,?,?)",rusqlite::params![n,thread,target,body]).unwrap();
        }
        let mut record = json!({"agent":"codex","agent_home":temp.path(),"conversation_id":id,"last_event_at":150,"last_main_progress_at":90});
        let mut output = json!({"process_state":"running","phase":"tool"});
        enrich(&record, &mut output, true);
        assert_eq!(output["phase"], "error");
        assert_eq!(output["provider_error"]["error_kind"], "capacity");
        assert_eq!(output["provider_errors"].as_array().unwrap().len(), 1);
        assert_eq!(output["attention_id"], "codex_native_log:codex-error-1");
        record["last_main_progress_at"] = json!(101);
        output = json!({"phase":"working"});
        enrich(&record, &mut output, true);
        assert_eq!(output["phase"], "working");
        assert_eq!(output["provider_errors"].as_array().unwrap().len(), 1);
        record["last_main_progress_at"] = json!(90);
        output = json!({"phase":"idle"});
        enrich(&record, &mut output, false);
        assert_eq!(output["phase"], "idle");
        record["conversation_id"] = json!(uuid::Uuid::new_v4().to_string());
        assert!(codex(&record).unwrap().is_empty());
    }
    #[test]
    fn provider_failure_kinds_keep_generic_failures_visible() {
        for (message, kind) in [
            ("quota exhausted", "quota"),
            ("429 Too Many Requests", "rate_limit"),
            ("authentication_failed", "authentication"),
            ("overloaded", "capacity"),
            ("max_output_tokens", "context_limit"),
            ("model_not_found", "model_unavailable"),
            ("Connection lost", "provider"),
        ] {
            assert_eq!(category(message), kind);
        }
    }
}
