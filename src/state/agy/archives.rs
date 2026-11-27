//! A stopped ACP binding is archived locally. A restore reserves its exact
//! source until the new worker durably confirms the original conversation.
use super::*;
use native_archive_store::AGY as STORE;

pub(super) const ARCHIVE_REASON: &str = "Archive needs a stopped Antigravity session with its original conversation, account and folder";
const PENDING: &str = "Finish Restore from the original Antigravity archive";

pub(super) fn restore_pending(record: &Value) -> bool {
    let id = string(record, "agy_restore_archive");
    !id.is_empty() && STORE.contains(id) != Ok(false)
}

pub(super) fn check_resume(record: &Value) -> Result<()> {
    if restore_pending(record) {
        return Err(PENDING.into());
    }
    if STORE.has_original(record)? {
        return Err("Finish archiving the original Antigravity binding before resuming".into());
    }
    Ok(())
}

// Historical workers cleared both pins only after a confirmed close or
// authentication refusal. Other saved states require recorded process pins.
pub(super) fn stopped(record: &Value) -> bool {
    match string(record, "state") {
        "paused" | "stopped" => true,
        "running" | "unknown" => !record["agy_provider"].is_null(),
        _ => false,
    }
}

pub(super) fn archive(name: &str, dry: bool, scope: Option<&session_action::Scope>) -> Result<()> {
    let _guard = state_guard()?;
    let record = binding(name)?;
    if let Some(scope) = scope {
        scope.check(&record)?;
    }
    terminating::check_mutation(&record)?;
    if !stopped(&record)
        || !original_exited(&record)
        || !session_id::canonical(string(&record, "conversation_id"))
        || restore_pending(&record)
    {
        return Err(ARCHIVE_REASON.into());
    }
    locations(&record)?;
    if dry {
        println!("hgs: would archive {name}");
        return Ok(());
    }

    let mut completed = record.clone();
    for key in [
        "agy_restore_archive",
        "agy_restore_source",
        "agy_restore_confirmed",
        "agy_restore_launcher",
    ] {
        completed.as_object_mut().unwrap().remove(key);
    }
    completed["completion_source"] = json!("manual_archive");
    completed["completion_reason"] = json!("user_archived");
    completed["agy_archive_source"] = json!(record_source::Digest::capture(&record).hex());
    if let Some(scope) = scope {
        scope.changing();
    }
    let id = STORE.publish(&completed)?;
    // A retry after publication can retire only the unchanged original.
    let archived = archive::read_archive(name, &id)?;
    if archived["agy_archive_source"] != completed["agy_archive_source"] {
        return Err("The saved Antigravity binding changed after archive publication".into());
    }
    storage::remove_record(&record)?;
    worktree_lock::left(&record);
    if let Some(scope) = scope {
        scope.result(
            name,
            string(&record, "run_id"),
            string(&record, "conversation_id"),
            Some(&id),
        );
    } else {
        println!("hgs: archived {name} ({id})");
    }
    Ok(())
}

fn same_source(record: &Value, archived: &Value) -> bool {
    record["agy_restore_archive"] == archived["archive_id"]
        && record["agy_restore_source"] == record_source::Digest::capture(archived).hex()
        && record["run_id"] != archived["run_id"]
        && [
            "name",
            "session_id",
            "conversation_id",
            "account_id",
            "agent_home",
            "cwd",
            "agy_account_location",
            "agy_workspace",
        ]
        .iter()
        .all(|key| record[*key] == archived[*key])
}

fn confirmed(record: &Value) -> bool {
    record["agy_restore_confirmed"] == record["run_id"]
}

fn retryable(record: &Value) -> bool {
    // Before a starting worker publishes its first pin there is no proof
    // that it won't still adopt the reservation.
    record["state"] != "starting" && original_exited(record)
}

// Before spawning ACP the worker durably changes the provider pin from
// explicit null to spawn_unconfirmed. Only a fresh Restore may supersede
// a dead launcher here. Under the same writer a delayed unadopted worker
// will find a different run, and an adopted worker must already be dead.
fn abandoned_start(record: &Value) -> bool {
    let exited = |pin: &Value| {
        pin.is_object() && pinned_state(pin) == Some(crate::platform::ProcessState::Exited)
    };
    if record["state"] != "starting"
        || !exited(&record["agy_restore_launcher"])
        || record.get("agy_provider") != Some(&Value::Null)
    {
        return false;
    }
    match (record.get("agy_worker"), record.get("agy_socket")) {
        (Some(Value::Null), Some(Value::Null)) => true,
        (Some(worker), Some(Value::String(socket))) => {
            let path = Path::new(socket);
            path.is_absolute() && path.ends_with("control") && exited(worker)
        }
        _ => false,
    }
}

fn may_retry_restore(record: &Value) -> bool {
    retryable(record) || abandoned_start(record)
}

fn existing(archived: &Value) -> Result<Option<Value>> {
    let name = string(archived, "name");
    if live()?.contains_key(name) {
        return Err("A live terminal already uses the archived Antigravity name".into());
    }
    if labels::terminal_taken(name)? {
        let current = binding(name)?;
        if same_source(&current, archived) {
            return Ok(Some(current));
        }
        return Err("A saved session already uses the archived Antigravity name".into());
    }
    if parent_link::read_session(string(archived, "session_id"), name)?.is_some() {
        return Err("The archived Antigravity session already has a saved binding".into());
    }
    Ok(None)
}

pub(in crate::state) fn row(
    archived: &Value,
    after: Option<u64>,
    name_taken: Option<bool>,
) -> Value {
    let mut row = base_row(archived);
    let name = string(archived, "name");
    // Page previews may already have checked the label. A pending restore
    // can be settled only by its own archive, even after a caller crash.
    let held = if name_taken == Some(false) {
        None
    } else {
        binding(name).ok()
    };
    let pending = held.as_ref().filter(|record| same_source(record, archived));
    let free = !name_taken.unwrap_or_else(|| storage::label_taken(name));
    let can_restore = pending.is_some_and(confirmed)
        || locations(archived).is_ok() && (free || pending.is_some_and(may_retry_restore));
    for (key, value) in [
        ("state", "archived"),
        ("conversation_state", "archived"),
        ("runtime_state", "none"),
        ("process_state", "exited"),
        ("activity", "idle"),
        ("phase", "idle"),
        ("activity_summary", "Archived"),
    ] {
        row[key] = json!(value);
    }
    for key in [
        "archive_id",
        "archived_at",
        "completion_source",
        "completion_reason",
    ] {
        row[key] = archived[key].clone();
    }
    let controls = json!({
        "auth_required":false, "tracked":true, "activity_supported":true, "resumable":can_restore,
        "interrupt_supported":false, "startup_retry_available":false,
        "send_supported":false, "send_available":false,
        "restore_pending":pending.is_some(), "allowed_actions":if can_restore {vec!["restore"]} else {Vec::new()},
        "action_reasons":{"restore":"Restore needs the original Antigravity account, folder and an available session name"}
    });
    row.as_object_mut()
        .unwrap()
        .extend(controls.as_object().unwrap().clone());
    if let Some(after) = after {
        let events = archived["agy_events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| event["seq"].as_u64().is_some_and(|seq| seq > after))
            .collect::<Vec<_>>();
        row["events"] = json!(events);
        row["cursor"] = archived["agy_seq"].clone();
        row["replace_events"] = json!(after == 0);
    }
    row
}

pub(super) fn restore(
    name: &str,
    id: &str,
    dry: bool,
    scope: Option<&session_action::Scope>,
) -> Result<()> {
    let record = {
        let _guard = state_guard()?;
        let archived = archive::read_archive(name, id)?;
        if let Some(scope) = scope {
            scope.check(&archived)?;
        }
        terminating::check_mutation(&archived)?;
        let current = existing(&archived)?;
        if let Some(current) = &current {
            if confirmed(current) {
                if dry {
                    println!("hgs: would finish restoring {name} from archive {id}");
                    return Ok(());
                }
                return finish(current, &archived, scope);
            }
            if !may_retry_restore(current) {
                return Err(
                    "The original Antigravity restore is still running or unconfirmed".into(),
                );
            }
        }
        locations(&archived)?;
        if executable().is_none() {
            return Err("Install the official Antigravity CLI before restoring a session".into());
        }
        if dry {
            println!("hgs: would restore {name} from archive {id}");
            return Ok(());
        }
        let mut record = archived.clone();
        for key in [
            "archive_id",
            "archived_at",
            "completion_source",
            "completion_reason",
            "agy_archive_source",
        ] {
            record.as_object_mut().unwrap().remove(key);
        }
        record["run_id"] = json!(uuid::Uuid::new_v4().to_string());
        record["agy_restore_launcher"] = process_pin(std::process::id())?;
        record["agy_restore_archive"] = json!(id);
        record["agy_restore_source"] = json!(record_source::Digest::capture(&archived).hex());
        if let Some(scope) = scope {
            scope.changing();
        }
        start(record)?
    };
    let observed = spawn_worker(&record);
    let _guard = state_guard()?;
    let archived = archive::read_archive(name, id)?;
    let current = binding(name)?;
    if current["run_id"] != record["run_id"] || !same_source(&current, &archived) {
        return Err("The original Antigravity restore binding changed".into());
    }
    if confirmed(&current) {
        return finish(&current, &archived, scope);
    }
    // A settled refusal leaves the archive available for a fresh user
    // action. Any uncertainty retains both the reservation and the source.
    if retryable(&current) {
        storage::remove_record(&current)?;
        if current["auth_required"] == true
            || current["agy_worker_not_started"] == current["run_id"]
        {
            if let Some(scope) = scope {
                scope.unchanged();
            }
        }
    }
    observed?;
    if current["auth_required"] == true {
        return Err("Antigravity sign-in required; sign in from Terminal".into());
    }
    Err("Antigravity did not confirm the original archived conversation".into())
}

fn finish(current: &Value, archived: &Value, scope: Option<&session_action::Scope>) -> Result<()> {
    if let Some(scope) = scope {
        scope.changing();
    }
    STORE.remove(archived)?;
    if let Some(scope) = scope {
        scope.result_record(current);
    } else {
        println!(
            "hgs: restored {} ({})",
            string(current, "name"),
            string(current, "conversation_id")
        );
    }
    Ok(())
}
