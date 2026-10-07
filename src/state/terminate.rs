//! Explicit stop-and-archive. `kill`/`forget` retain their removal semantics.
use super::*;
use std::time::{Duration, Instant};

pub(super) fn dispatch(args: &[String], dry: bool) -> Result<i32> {
    let (name, expected_run) = match args {
        [name] => (name.as_str(), None),
        [name, flag, run] if flag == "--expected-run-id" && !run.is_empty() => {
            (name.as_str(), Some(run.as_str()))
        }
        _ => return Err("usage: hgs terminate <session> [--expected-run-id ID]".into()),
    };
    let original = {
        let _guard = lock(None)?;
        let record = read(name)?;
        if expected_run.is_some_and(|run| run != string(&record, "run_id")) {
            return Err("session changed before termination; refresh and try again".into());
        }
        let snapshot = live()?;
        if let Some(panes) = snapshot.get(name) {
            if panes.len() != 1
                || panes[0].0 != string(&record, "pane")
                || panes[0].2 != string(&record, "run_id")
                || panes[0].2.is_empty()
            {
                return Err("session terminal identity changed; refresh before terminating".into());
            }
            let target = tmux(
                &[
                    "display-message".into(),
                    "-p".into(),
                    "-t".into(),
                    string(&record, "pane").into(),
                    "#{session_id}".into(),
                ],
                true,
            )?;
            let target = String::from_utf8_lossy(&target.stdout).trim().to_owned();
            if !target
                .strip_prefix('$')
                .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
            {
                return Err("could not confirm the session terminal".into());
            }
            if !dry {
                // Pin the tmux instance, never resolve a reusable name a second time.
                // The writer lock prevents a concurrent HGS rename/relaunch here.
                tmux(&["kill-session".into(), "-t".into(), target], true)?;
            }
        } else if process_alive(&record) {
            return Err(
                "tracked agent is running without its expected terminal; no process was stopped"
                    .into(),
            );
        }
        record
    };
    if dry {
        println!("hgs: would terminate and archive {name}");
        return Ok(0);
    }
    // Supervisors and final hooks can now record the exit. On timeout the binding
    // stays in place: stopping a terminal must never silently forget its history.
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_alive(&original) {
        if Instant::now() >= deadline {
            return Err(
                "terminal closed, but the agent has not exited; its saved binding was kept".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _guard = lock(None)?;
    if let Some(archived) = archive::records()?.into_iter().find(|entry| {
        entry["name"] == original["name"]
            && entry["conversation_id"] == original["conversation_id"]
            && (entry["run_id"] == original["run_id"]
                || (!string(&original, "restore_archive_id").is_empty()
                    && entry["archive_id"] == original["restore_archive_id"]))
    }) {
        println!("hgs: archived {name} ({})", string(&archived, "archive_id"));
        return Ok(0);
    }
    let mut record = read(name)?;
    if record["run_id"] != original["run_id"] || record["created"] != original["created"] {
        return Err(
            "session binding changed while terminating; the replacement was left intact".into(),
        );
    }
    // No live instance with the same name may be removed from the catalog.
    if live()?.contains_key(name) || process_alive(&record) {
        return Err("session is still running; its saved binding was kept".into());
    }
    record["completion_source"] = json!("manual_terminate");
    record["completion_reason"] = json!("user_terminated");
    let id = archive::save(&record)?;
    println!("hgs: terminated and archived {name} ({id})");
    Ok(0)
}
