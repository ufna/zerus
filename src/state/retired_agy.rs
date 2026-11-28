//! Read-only compatibility for Antigravity bindings and archives already on disk.
use super::*;

pub(crate) const ACCOUNT: &str = "native-agy";
pub(crate) use zerus_client_core::launch::ANTIGRAVITY_RETIRED as RETIRED;

pub(crate) fn named(name: &str) -> bool {
    name.starts_with("agy/") && zerus_client_core::desktop::creation_launch::session_name(name)
}

pub(super) fn identity(record: &Value) -> bool {
    named(string(record, "name"))
        && record["agent"] == "agy"
        && record["backend"] == "agy"
        && record["account_id"] == ACCOUNT
}

/// Binding proof shared with the worktree census. This gives no action authority.
pub(super) fn valid_binding(record: &Value) -> bool {
    record["version"] == 1
        && identity(record)
        && record["rename_shadow"] != true
        && string(record, "archive_id").is_empty()
        && session_id::canonical(string(record, "session_id"))
        && session_id::canonical(string(record, "run_id"))
}

fn binding(name: &str) -> Result<Value> {
    let record = storage::read_selected_metadata(name)?;
    if !valid_binding(&record) {
        return Err("The saved Antigravity binding is unavailable".into());
    }
    Ok(record)
}

pub(super) fn inspect(name: &str) -> Result<Value> {
    Ok(row(&binding(name)?, Some(0)))
}

pub(super) fn row(record: &Value, after: Option<u64>) -> Value {
    let mut parts = string(record, "name").splitn(3, '/');
    let (agent, project, tag) = (parts.next(), parts.next(), parts.next());
    let archived = !string(record, "archive_id").is_empty();
    let mut row = json!({
        "name":record["name"], "cmd":agent, "agent":agent, "backend":record["backend"],
        "account_id":record["account_id"], "session_id":record["session_id"],
        "run_id":record["run_id"], "launch_id":record["launch_id"],
        "conversation_id":record["conversation_id"], "cwd":record["cwd"], "cwd_source":"native_session",
        "project":project, "tag":tag, "created":record["created"], "last_event_at":record["last_event_at"],
        "reply_id":record["reply_id"], "reply_at":record["reply_at"]
    });
    let controls = json!({
        "state":if archived {"archived"} else {"saved"},
        "conversation_state":if archived {"archived"} else {"saved"},
        "runtime_state":"none", "process_state":if archived {"exited"} else {"unverified"},
        "activity":"unknown", "phase":"unknown", "activity_summary":"Integration retired",
        "activity_detail":RETIRED, "tracked":true, "activity_supported":true, "resumable":false,
        "terminal_supported":false, "terminal_attach_supported":false, "native_ui_available":false,
        "attachments_supported":false, "bulk_lifecycle_supported":false, "settings_change_supported":false,
        "clear_context_supported":false, "compact_context_supported":false, "fork_supported":false,
        "auth_required":false, "interrupt_supported":false, "startup_retry_available":false,
        "send_supported":false, "send_available":false, "allowed_actions":[],
        "action_reasons":{"resume":RETIRED,"restore":RETIRED}
    });
    row.as_object_mut()
        .unwrap()
        .extend(controls.as_object().unwrap().clone());
    if archived {
        for key in [
            "archive_id",
            "archived_at",
            "completion_source",
            "completion_reason",
        ] {
            row[key] = record[key].clone();
        }
    }
    if let Some(after) = after {
        let events = record["agy_events"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        // The retired writer stored a ring of 32 events.
        let events: Vec<_> = events[events.len().saturating_sub(32)..]
            .iter()
            .filter(|event| event["seq"].as_u64().is_some_and(|seq| seq > after))
            .collect();
        row["events"] = json!(events);
        row["cursor"] = record["agy_seq"].clone();
        row["replace_events"] = json!(after == 0);
    }
    row
}

pub(super) fn dispatch(command: &str, name: &str, args: &[String]) -> Result<i32> {
    match command {
        "inspect" => {
            let mut after = 0;
            let mut archive = None;
            let mut options = args.iter();
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--skip-processes" | "--current-read" | "--read-observation" => {}
                    "--archive" if archive.is_none() => {
                        archive = Some(options.next().ok_or("Missing Antigravity archive ID")?);
                    }
                    "--after" => {
                        after = options
                            .next()
                            .and_then(|value| value.parse().ok())
                            .ok_or("Invalid Antigravity inspection cursor")?;
                    }
                    _ => return Err("Unsupported Antigravity inspection option".into()),
                }
            }
            let record = match archive {
                Some(id) => archive::read_archive(name, id)?,
                None => binding(name)?,
            };
            println!("{}", row(&record, Some(after)));
            Ok(0)
        }
        "exists" if args.is_empty() => Ok(i32::from(binding(name).is_err())),
        _ => Err(RETIRED.into()),
    }
}
