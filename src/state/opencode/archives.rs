//! Archive and Restore change saved Zerus bindings. OpenCode's server and
//! its own conversation store are shared and receive no lifecycle writes.
use super::*;
use std::collections::{HashMap, HashSet};

pub(in crate::state) const STORE: native_archive_store::Store =
    native_archive_store::Store::new("opencode/archives-v1.sqlite3", "OpenCode", valid);

const CHECK_BUDGET: Duration = Duration::from_secs(3);
pub(in crate::state) const NOT_IDLE: &str =
    "Wait for this OpenCode conversation to be idle with an empty inbox before archiving";
const ACTIVE_CHILD: &str =
    "OpenCode still has a running child session. End its work in OpenCode before archiving.";
const UNKNOWN_CHILD: &str = "OpenCode could not verify that child sessions are idle.";

fn valid(record: &Value) -> bool {
    string(record, "name").starts_with("opencode/")
        && record["agent"] == "opencode"
        && record["backend"] == "opencode"
        && record["service"] == "opencode2"
        && record["creation_pending"] != true
        && native_id(string(record, "conversation_id"), "ses_")
        && !string(record, "run_id").is_empty()
}

pub(super) fn offers(record: &Value, observed: &Observed) -> bool {
    valid(record)
        && !pending_restore(record)
        && !observed.active
        && observed.forms.as_array().is_some_and(Vec::is_empty)
        && observed.permissions.as_array().is_some_and(Vec::is_empty)
        && observed.inbox["data"].as_array().is_some_and(Vec::is_empty)
}

/// Read at most a small, exact ancestry set from the live native inventory.
fn active_ancestors(native: &dyn Native, active: &Value) -> Result<HashSet<String>> {
    let live = active.as_object().ok_or(UNKNOWN_CHILD)?;
    if live.len() > 16 {
        return Err(UNKNOWN_CHILD.into());
    }
    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    let mut ancestors = HashSet::new();
    for id in live.keys() {
        let mut next = id.clone();
        let mut chain = HashSet::new();
        loop {
            if !native_id(&next, "ses_") || !chain.insert(next.clone()) {
                return Err(UNKNOWN_CHILD.into());
            }
            if !parents.contains_key(&next) {
                if parents.len() == 32 {
                    return Err(UNKNOWN_CHILD.into());
                }
                let answer =
                    get(native, &format!("/api/session/{next}")).map_err(|_| UNKNOWN_CHILD)?;
                if answer["data"]["id"] != next {
                    return Err(UNKNOWN_CHILD.into());
                }
                let parent = match answer["data"].get("parentID") {
                    Some(Value::String(value)) => Some(value.clone()),
                    None | Some(Value::Null) => None,
                    _ => return Err(UNKNOWN_CHILD.into()),
                };
                parents.insert(next.clone(), parent);
            }
            match &parents[&next] {
                Some(parent) => {
                    ancestors.insert(parent.clone());
                    next = parent.clone();
                }
                None => break,
            }
        }
    }
    Ok(ancestors)
}

pub(super) fn decorate(
    row: &mut Value,
    record: &Value,
    native: &dyn Native,
    active: &Value,
    cached: &mut Option<Result<HashSet<String>>>,
) {
    let Some(actions) = row["allowed_actions"].as_array_mut() else {
        return;
    };
    if pending_restore(record) {
        actions.retain(|op| op != "rename" && op != "forget" && op != "archive");
        return;
    }
    if !actions.iter().any(|op| op == "archive") {
        return;
    }
    let blocked = match cached.get_or_insert_with(|| active_ancestors(native, active)) {
        Ok(ancestors) if ancestors.contains(string(record, "conversation_id")) => {
            Some(ACTIVE_CHILD)
        }
        Ok(_) => None,
        Err(_) => Some(UNKNOWN_CHILD),
    };
    if let Some(reason) = blocked {
        actions.retain(|op| op != "archive");
        row["action_reasons"]["archive"] = json!(reason);
    }
}

fn writer() -> Result<Guard> {
    // Provider paths can acquire the normal writer before the bindings lock.
    // Holding the bindings lock, we may only try the writer without waiting.
    storage::writer_until(Instant::now())?
        .ok_or_else(|| "Session metadata is busy; the saved records were kept".into())
}

fn verify_idle(
    native: &dyn Native,
    record: &mut Value,
    deadline: Instant,
    dry: bool,
) -> Result<Value> {
    if !valid(record) {
        return Err("A confirmed OpenCode conversation is required".into());
    }
    if pending_restore(record) {
        return Err("Finish the original OpenCode restore before archiving again".into());
    }
    let session = verify_session(native, record)?;
    if !pending_requests(native, record, "/form")?
        .as_array()
        .is_some_and(Vec::is_empty)
        || !pending_requests(native, record, "/permission")?
            .as_array()
            .is_some_and(Vec::is_empty)
        || !get(native, &session_path(record, "/inbox"))?["data"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        return Err(NOT_IDLE.into());
    }
    if compact::archive_pending(native, record, deadline, dry)? {
        return Err("Finish the original OpenCode compaction before archiving".into());
    }
    let active = active_sessions(native)?;
    let conversation = string(record, "conversation_id");
    if active.get(conversation).is_some() {
        return Err(NOT_IDLE.into());
    }
    if active_ancestors(native, &active)?.contains(conversation) {
        return Err(ACTIVE_CHILD.into());
    }
    if !active.as_object().is_some_and(|rows| rows.is_empty()) && active_sessions(native)? != active
    {
        return Err(UNKNOWN_CHILD.into());
    }
    Ok(session)
}

pub(in crate::state) fn archive(
    name: &str,
    dry: bool,
    scope: Option<&session_action::Scope>,
) -> Result<i32> {
    let _bindings = try_bindings(CHECK_BUDGET).ok_or("OpenCode bindings are busy")?;
    let mut record = binding(name)?;
    if let Some(scope) = scope {
        scope.check(&record)?;
    }
    let service = running_service()?;
    let native = Bounded {
        service: &service,
        deadline: Instant::now() + CHECK_BUDGET,
    };
    let session = verify_idle(&native, &mut record, native.deadline, dry)?;
    if dry {
        println!("hgs: would archive {name}");
        return Ok(0);
    }
    let _writer = writer()?;
    if binding(name)? != record {
        return Err("OpenCode binding changed before archiving".into());
    }
    let mut completed = record.clone();
    completed["version"] = json!(1);
    completed["completion_source"] = json!("manual_archive");
    completed["completion_reason"] = json!("user_archived");
    completed["title"] = session["title"].clone();
    completed
        .as_object_mut()
        .unwrap()
        .remove("opencode_restored_from");
    if let Some(scope) = scope {
        scope.changing();
    }
    let id = STORE.publish(&completed)?;
    PROVIDER.remove(&record)?;
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
    Ok(0)
}

/// A previous local restore reserved exactly this archive's conversation
/// before the old file could be retired. Only that restored binding can finish it.
fn from_archive(current: &Value, archived: &Value) -> bool {
    current["opencode_restored_from"] == archived["archive_id"]
        && current["session_id"] == archived["session_id"]
        && current["conversation_id"] == archived["conversation_id"]
        && current["cwd"] == archived["cwd"]
        && current["run_id"] != archived["run_id"]
}

pub(in crate::state) fn restore_source_held(archived: &Value) -> Result<bool> {
    let name = string(archived, "name");
    if PROVIDER.vacant(name)? {
        return Ok(false);
    }
    Ok(from_archive(&binding(name)?, archived))
}

pub(super) fn pending_restore(record: &Value) -> bool {
    let id = string(record, "opencode_restored_from");
    !id.is_empty() && STORE.contains(id) != Ok(false)
}

fn name_available(name: &str) -> Result<bool> {
    Ok(PROVIDER.vacant(name)? && !storage::label_taken(name))
}

pub(in crate::state) fn capabilities(archived: &Value, name_taken: Option<bool>) -> Value {
    let name = string(archived, "name");
    let pending = name_taken != Some(false) && restore_source_held(archived) == Ok(true);
    let free = name_taken.map_or_else(|| name_available(name) == Ok(true), |taken| !taken);
    let can_restore =
        valid(archived) && Path::new(string(archived, "cwd")).is_dir() && (pending || free);
    let mut allowed = Vec::new();
    if can_restore {
        allowed.push("restore");
    }
    if !pending {
        allowed.push("forget");
    }
    json!({"allowed_actions":allowed,"restore_pending":pending,
        "action_reasons":{"restore":"Restore needs the original OpenCode folder and an unused session name",
            "forget":"Finish the original OpenCode restore before forgetting this archive"}})
}

pub(in crate::state) fn restore(
    archived: &Value,
    dry: bool,
    scope: Option<&session_action::Scope>,
) -> Result<i32> {
    let name = string(archived, "name");
    let id = string(archived, "archive_id");
    if let Some(scope) = scope {
        scope.check(archived)?;
        scope.opencode_error();
    }
    if !valid(archived) || !Path::new(string(archived, "cwd")).is_dir() {
        return Err("Restore needs the original OpenCode conversation and folder".into());
    }
    let _bindings = try_bindings(CHECK_BUDGET).ok_or("OpenCode bindings are busy")?;
    let resumed = if name_available(name)? {
        None
    } else if restore_source_held(archived)? {
        Some(binding(name)?)
    } else {
        return Err("A live or saved session already uses this name".into());
    };
    let service = running_service()?;
    let native = Bounded {
        service: &service,
        deadline: Instant::now() + CHECK_BUDGET,
    };
    verify_session(&native, archived)?;
    let _writer = writer()?;
    if super::super::archive::read_archive(name, id)? != *archived {
        return Err("OpenCode archive changed before restore".into());
    }
    if live()?.contains_key(name) || (resumed.is_none() && !name_available(name)?) {
        return Err("A live or saved session already uses this name".into());
    }
    if resumed.is_none()
        && PROVIDER
            .id_path(string(archived, "session_id"))
            .is_some_and(|path| {
                !std::fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            })
    {
        return Err("This OpenCode session identity already has a saved binding".into());
    }
    if dry {
        println!("hgs: would restore {name} from archive {id}");
        return Ok(0);
    }
    if let Some(scope) = scope {
        scope.changing();
    }
    let record = if let Some(record) = resumed {
        record
    } else {
        let mut record = archived.clone();
        for key in [
            "archive_id",
            "archived_at",
            "state",
            "completion_source",
            "completion_reason",
            "layout",
            "opencode_compaction",
        ] {
            record.as_object_mut().unwrap().remove(key);
        }
        record["run_id"] = json!(uuid::Uuid::new_v4().to_string());
        record["opencode_restored_from"] = json!(id);
        save(&mut record)?;
        record
    };
    STORE.remove(archived)?;
    if let Some(scope) = scope {
        scope.result_record(&record);
    } else {
        println!(
            "hgs: restored {name} ({})",
            string(&record, "conversation_id")
        );
    }
    Ok(0)
}

/// Archived Activity reads the exact provider conversation without presenting
/// live controls or publishing observation state into an unrelated binding.
pub(in crate::state) fn inspect(archived: &Value) -> Value {
    let mut value = normalize(archived, None);
    let page = (|| {
        if !valid(archived) {
            return Err("The recorded OpenCode conversation is unavailable".into());
        }
        let service = running_service()?;
        let native = Bounded {
            service: &service,
            deadline: Instant::now() + CHECK_BUDGET,
        };
        verify_session(&native, archived)?;
        history::fetch_archived(&native, archived)
    })();
    match page {
        Ok(page) => {
            value["events"] = json!(history::rows(&page.messages));
            value["history_truncated"] = json!(page.truncated);
        }
        Err(_) => {
            value["inspection_reason"] = json!("The exact OpenCode archive history is unavailable");
            value["replace_events"] = json!(false);
        }
    }
    value["state"] = json!("archived");
    value["process_state"] = json!("stopped");
    value["runtime_state"] = json!("none");
    value["conversation_state"] = json!("archived");
    value["activity"] = json!("idle");
    value["activity_summary"] = json!("Archived");
    value["archive_id"] = archived["archive_id"].clone();
    value["archived_at"] = archived["archived_at"].clone();
    value["title"] = archived["title"].clone();
    value
        .as_object_mut()
        .unwrap()
        .extend(capabilities(archived, None).as_object().unwrap().clone());
    let preferences = notifications::load();
    notifications::enrich(
        &mut value,
        preferences.as_ref(),
        journal::journal_name(archived),
    );
    value
}
