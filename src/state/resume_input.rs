//! Codex can defer SessionStart until the first turn even after native resume.
//! Permit that turn only when the pinned process owns the exact saved rollout
//! and an empty native composer. This never fabricates a hook or confirms a run.
use super::*;
use std::fs;
use std::io::{BufRead, BufReader};

pub(super) fn pending(record: &Value) -> bool {
    record["agent"] == "codex"
        && record["run_identity_version"] == 1
        && record["startup_kind"] == "resume"
        && record["supervisor"].is_object()
        && !string(record, "conversation_id").is_empty()
        && record["expected_id"] == record["conversation_id"]
        && string(record, "requested_id").is_empty()
        && string(record, "error").is_empty()
        && record.get("session_end").is_none()
        && record["last_event_at"].as_f64().unwrap_or(0.0) == 0.0
        && ["", "unknown"].contains(&string(record, "activity"))
        && ["", "unknown"].contains(&string(record, "phase"))
}

fn owns_rollout(record: &Value) -> Result<()> {
    let path = Path::new(string(record, "transcript"))
        .canonicalize()
        .map_err(|_| "Saved conversation history is unavailable")?;
    if !path.is_file() {
        return Err("Saved conversation history is not a file".into());
    }
    let mut line = String::new();
    BufReader::new(fs::File::open(&path).map_err(|e| e.to_string())?)
        .take(64 * 1024)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let meta: Value =
        serde_json::from_str(&line).map_err(|_| "Cannot verify saved conversation history")?;
    if meta["type"] != "session_meta" || meta["payload"]["id"] != record["expected_id"] {
        return Err("Saved conversation history has a different identity".into());
    }
    let pid = record["pid"].as_u64().ok_or("Missing agent process")?;
    #[cfg(target_os = "linux")]
    let open = fs::read_dir(format!("/proc/{pid}/fd"))
        .map_err(|e| e.to_string())?
        .flatten()
        .any(|entry| fs::read_link(entry.path()).is_ok_and(|p| p == path));
    #[cfg(not(target_os = "linux"))]
    let open = {
        let output = recipes::output_timeout(
            std::process::Command::new("/usr/sbin/lsof")
                .args(["-a", "-p", &pid.to_string(), "-Fn", "--"])
                .arg(&path),
            std::time::Duration::from_secs(2),
        )?;
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.strip_prefix('n')
                    .is_some_and(|name| Path::new(name) == path)
            })
    };
    if !open {
        return Err("Codex has not opened the saved conversation yet".into());
    }
    Ok(())
}

pub(super) fn ready(record: &Value) -> Result<()> {
    if !pending(record) || pause_active(record) {
        return Err("Waiting for the agent to confirm this conversation".into());
    }
    if record["input_pending_at"].as_f64().unwrap_or(0.0) > 0.0 {
        return Err("Message sent. Waiting for the agent to confirm this conversation.".into());
    }
    if !process_alive(record) || !matches(record, live()?.get(string(record, "name"))) {
        return Err("The resumed agent is no longer running".into());
    }
    let supervisor = &record["supervisor"];
    let parent = supervisor["pid"]
        .as_u64()
        .ok_or("Missing session supervisor")?;
    if parent > u32::MAX as u64
        || string(supervisor, "start").is_empty()
        || process_start(parent as u32) != string(supervisor, "start")
    {
        return Err("The resumed session supervisor has changed".into());
    }
    let pid = record["pid"].as_u64().ok_or("Missing agent process")?;
    let process = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "ppid=,comm="])
        .output()
        .map_err(|e| e.to_string())?;
    let process = String::from_utf8_lossy(&process.stdout);
    let mut fields = process.split_whitespace();
    if fields.next().and_then(|n| n.parse::<u64>().ok()) != Some(parent)
        || fields
            .next()
            .and_then(|p| Path::new(p).file_name())
            .is_none_or(|n| n != "codex")
    {
        return Err("The saved agent process is not the resumed Codex".into());
    }
    owns_rollout(record)?;
    input::checked_terminal(record, true)?;
    if !process_alive(record) {
        return Err("The resumed agent process has changed".into());
    }
    Ok(())
}

pub(super) fn enrich(record: &Value, output: &mut Value, live: bool) {
    if !live || !pending(record) {
        return;
    }
    let reason = ready(record).err().unwrap_or_default();
    output["resume_message_can_send"] = json!(reason.is_empty());
    output["resume_message_reason"] = json!(reason);
    if reason.is_empty() {
        output["activity"] = json!("idle");
        output["phase"] = json!("idle");
        output["activity_summary"] = json!("Waiting for your next message");
    } else {
        output["phase"] = json!("starting");
        output["activity_summary"] = json!("Restoring conversation");
        output["activity_detail"] = json!(reason);
    }
}
