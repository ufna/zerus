//! OpenCode 2 history: `/message` cursor pages read newest first, queued
//! inbox items, and the Activity events both become.
use super::super::*;
#[cfg(test)]
use super::service::Response;
use super::{catalog::encode, native_id, queued_message_id, session_path, Native};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::{Duration, Instant};

/// The newest window of a conversation, oldest first.
pub(super) struct Page {
    pub messages: Vec<Value>,
    pub truncated: bool,
}

#[derive(Clone, Copy)]
struct Bounds {
    messages: usize,
    bytes: usize,
}
impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            messages: 10_000,
            bytes: 16 * 1024 * 1024,
        }
    }
}
const PAGE: usize = 100;
const INBOX: usize = 256;
const CURSOR: usize = 4096;
const TEXT: usize = 32000;

/// Messages read earlier, oldest first. Only messages older than the oldest
/// message of a freshly read page are ever taken from it: messages change in
/// place while OpenCode streams them.
struct Cache {
    messages: Vec<Value>,
    truncated: bool,
}

fn cache_path(record: &Value) -> PathBuf {
    root().join("opencode/history").join(format!(
        "{:x}.json",
        Sha256::digest(string(record, "conversation_id").as_bytes())
    ))
}
fn load_at(path: &Path, record: &Value) -> Option<Cache> {
    let file = std::fs::File::open(path).ok()?;
    let limit = Bounds::default().bytes as u64 + 1024 * 1024;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    if value["conversation_id"] != record["conversation_id"] {
        return None;
    }
    let messages = value["messages"].as_array()?;
    if messages.iter().any(|m| !valid(m)) {
        return None;
    }
    Some(Cache {
        messages: messages.clone(),
        truncated: value["truncated"] != false,
    })
}
/// Best effort: the cache only saves reading older pages again.
fn save_at(path: &Path, record: &Value, page: &Page) {
    let value = json!({"conversation_id": record["conversation_id"], "truncated": page.truncated, "messages": page.messages});
    if atomic(path, &value.to_string()).is_ok() {
        if let Some(dir) = path.parent() {
            prune(dir, path, CACHE_FILES, CACHE_BYTES);
        }
    }
}
/// Best effort: an unchanged cache in use counts as recent for pruning.
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().append(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}
/// Forgetting a session drops its cache; a later binding reads it again.
pub(super) fn forget(record: &Value) {
    let _ = std::fs::remove_file(cache_path(record));
}

/// The cache directory keeps at most this many conversations and bytes.
const CACHE_FILES: usize = 64;
const CACHE_BYTES: u64 = 256 * 1024 * 1024;
/// A temporary file left by an interrupted save is removed after this long.
const STALE_TEMP: Duration = Duration::from_secs(3600);

fn cache_name(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
/// Best effort: removes the least recently used caches beyond the bounds,
/// never `keep`, and temporary files of interrupted saves.
fn prune(dir: &Path, keep: &Path, files: usize, bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    let mut caches = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !meta.is_file() {
            continue;
        }
        if name.starts_with(".hgs-") {
            if now
                .duration_since(modified)
                .is_ok_and(|age| age > STALE_TEMP)
            {
                let _ = std::fs::remove_file(entry.path());
            }
        } else if cache_name(&name) && entry.path() != keep {
            caches.push((modified, meta.len(), entry.path()));
        }
    }
    let mut count = 1;
    let mut total = std::fs::metadata(keep).map_or(0, |m| m.len());
    caches.sort_by_key(|cache| std::cmp::Reverse(cache.0));
    for (_, len, path) in caches {
        count += 1;
        total = total.saturating_add(len);
        if count > files || total > bytes {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn valid(message: &Value) -> bool {
    message["id"]
        .as_str()
        .is_some_and(|id| native_id(id, "msg_"))
}

/// Reads the newest window of `/message` with the cache, within 20 seconds.
pub(super) fn fetch(native: &dyn Native, record: &Value) -> Result<Page> {
    fetch_published(native, record, &cache_path(record), publish_lock, || {
        super::binding(string(record, "name")).ok()
    })
}
/// Archived activity has no live binding. It can use the conversation's
/// cache as a read aid but cannot publish, refresh or evict it.
pub(super) fn fetch_archived(native: &dyn Native, record: &Value) -> Result<Page> {
    let cached = load_at(&cache_path(record), record);
    let deadline = Instant::now() + Duration::from_secs(3);
    scan(native, record, cached.as_ref(), Bounds::default(), deadline).map(|(page, _)| page)
}

/// `fetch` with a cache path of the caller's own, saved without a binding.
#[cfg(test)]
pub(super) fn fetch_at(native: &dyn Native, record: &Value, cache: &Path) -> Result<Page> {
    fetch_with(native, record, cache, |page| save_at(cache, record, page))
}
/// `fetch` with its lock and current binding given: a changed read is saved
/// only through `publish_with`.
fn fetch_published<G>(
    native: &dyn Native,
    record: &Value,
    cache: &Path,
    lock: impl FnOnce() -> Option<G>,
    current: impl FnOnce() -> Option<Value>,
) -> Result<Page> {
    fetch_with(native, record, cache, |page| {
        publish_with(lock, current, record, cache, page)
    })
}
fn fetch_with(
    native: &dyn Native,
    record: &Value,
    cache: &Path,
    save: impl FnOnce(&Page),
) -> Result<Page> {
    let cached = load_at(cache, record);
    let deadline = Instant::now() + Duration::from_secs(20);
    let (page, changed) = scan(native, record, cached.as_ref(), Bounds::default(), deadline)?;
    if changed {
        save(&page);
    } else if cached.is_some() {
        touch(cache);
    }
    Ok(page)
}
/// How long a read waits for the bindings lock before it skips its save.
const PUBLISH_WAIT: Duration = Duration::from_millis(200);
/// Set once a read in this process found the lock busy: later reads (the
/// rest of a listing) try once and never wait again.
static PUBLISH_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The bindings lock for a read's save, within `PUBLISH_WAIT` until a read
/// in this process found it busy.
fn publish_lock() -> Option<super::Bindings> {
    use std::sync::atomic::Ordering::Relaxed;
    let wait = if PUBLISH_BUSY.load(Relaxed) {
        Duration::ZERO
    } else {
        PUBLISH_WAIT
    };
    let guard = super::try_bindings(wait);
    if guard.is_none() {
        PUBLISH_BUSY.store(true, Relaxed);
    }
    guard
}
/// Saves the cache only under the bindings lock, which Forget holds while it
/// removes a binding and its cache, and only while this exact binding (run
/// and conversation) is still the one under its name. A Forget, rename or
/// re-creation that ran during the read wins: nothing is saved and this
/// conversation's cache is removed. Without the lock the save is skipped;
/// an action that already holds it saves at once.
fn publish_with<G>(
    lock: impl FnOnce() -> Option<G>,
    current: impl FnOnce() -> Option<Value>,
    record: &Value,
    cache: &Path,
    page: &Page,
) {
    let Some(_guard) = lock() else {
        return;
    };
    publish_locked(record, cache, page, current().as_ref());
}
fn publish_locked(record: &Value, cache: &Path, page: &Page, current: Option<&Value>) {
    let same = current.is_some_and(|binding| {
        binding["run_id"] == record["run_id"]
            && binding["conversation_id"] == record["conversation_id"]
    });
    if same {
        save_at(cache, record, page);
    } else {
        let _ = std::fs::remove_file(cache);
    }
}

/// Pages newest first until the end, a budget or a page that reaches back
/// into the cache. Returns the window and whether the cache should be saved.
fn scan(
    native: &dyn Native,
    record: &Value,
    cached: Option<&Cache>,
    bounds: Bounds,
    deadline: Instant,
) -> Result<(Page, bool)> {
    let mut fresh: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0usize;
    let mut cursor: Option<String> = None;
    let mut truncated = false;
    let mut older: Option<&Cache> = None;
    let mut splice = 0;
    let mut page_limit = PAGE;
    'pages: loop {
        let query = match &cursor {
            None => format!("/message?limit={page_limit}&order=desc"),
            Some(next) => format!("/message?limit={page_limit}&cursor={}", encode(next)),
        };
        let response = match native.raw_until("GET", &session_path(record, &query), None, deadline)
        {
            Ok(response) => response,
            // Inline file bodies can exceed the HTTP budget even when the
            // projected message window is small. Retry the same cursor with
            // fewer messages and keep the original deadline.
            Err(error) if error == super::service::RESPONSE_TOO_LARGE && page_limit > 1 => {
                page_limit /= 10;
                continue;
            }
            // A later page that runs out of time leaves the newest window.
            Err(_) if cursor.is_some() && Instant::now() >= deadline => {
                truncated = true;
                break;
            }
            Err(error) => return Err(error),
        };
        if !(200..300).contains(&response.status) {
            return Err(format!(
                "OpenCode history could not be read (HTTP {})",
                response.status
            ));
        }
        let data = response.body["data"]
            .as_array()
            .ok_or("Invalid OpenCode history")?;
        let mut added = false;
        for message in data {
            if !valid(message) || !seen.insert(string(message, "id").to_owned()) {
                continue;
            }
            let mut message = message.clone();
            super::outbox::strip_file_bodies(&mut message);
            let size = message.to_string().len();
            if fresh.len() >= bounds.messages || bytes.saturating_add(size) > bounds.bytes {
                truncated = true;
                break 'pages;
            }
            bytes += size;
            fresh.push(message);
            added = true;
        }
        if let (Some(cache), Some(oldest)) = (cached, fresh.last()) {
            if let Some(index) = cache.messages.iter().position(|m| m["id"] == oldest["id"]) {
                older = Some(cache);
                splice = index;
                break;
            }
        }
        let Some(next) = response.body["cursor"]["next"].as_str() else {
            break;
        };
        if data.is_empty() {
            break;
        }
        if !added || next.len() > CURSOR || Instant::now() >= deadline {
            truncated = true;
            break;
        }
        cursor = Some(next.to_owned());
    }
    fresh.reverse();
    let mut messages = match older {
        Some(cache) => {
            truncated |= cache.truncated;
            let mut messages: Vec<Value> = cache.messages[..splice]
                .iter()
                .filter(|m| !seen.contains(string(m, "id")))
                .cloned()
                .collect();
            messages.extend(fresh);
            messages
        }
        None => fresh,
    };
    if older.is_some() && trim(&mut messages, bounds) {
        truncated = true;
    }
    let changed = cached.is_none_or(|cache| {
        cache.truncated != truncated
            || cache.messages.len() != messages.len()
            || cache.messages.last().map(|m| &m["id"]) != messages.last().map(|m| &m["id"])
    });
    Ok((
        Page {
            messages,
            truncated,
        },
        changed,
    ))
}

/// Drops the oldest messages beyond the bounds; true when any were dropped.
fn trim(messages: &mut Vec<Value>, bounds: Bounds) -> bool {
    let mut bytes = 0usize;
    let mut keep = 0;
    for message in messages.iter().rev() {
        let size = message.to_string().len();
        if keep >= bounds.messages || bytes.saturating_add(size) > bounds.bytes {
            break;
        }
        bytes += size;
        keep += 1;
    }
    let drop = messages.len() - keep;
    messages.drain(..drop);
    drop > 0
}

fn seconds(value: &Value) -> f64 {
    value["time"]["created"].as_f64().unwrap_or(0.0) / 1000.0
}
fn event(kind: &str, detail: String, tool: &str, at: f64, id: String, reply: Value) -> Value {
    json!({"type":kind,"detail":detail,"tool":tool,"at":at,"source":"opencode_native","agent_id":"","message_id":id,"reply_id":reply})
}
/// Text of a value that may be a string or a JSON structure, clipped.
fn shown(value: &Value) -> String {
    if value.is_string() {
        journal::clipped(value, TEXT)
    } else if value.is_null() {
        String::new()
    } else {
        journal::clipped(&json!(value.to_string()), TEXT)
    }
}
/// A failure's message; never its raw provider response.
fn error_text(error: &Value) -> String {
    let message = error["message"]
        .as_str()
        .or(error.as_str())
        .or(error["type"].as_str())
        .unwrap_or("OpenCode turn failed");
    journal::clipped(&json!(message), TEXT)
}
fn part_id(part: &Value, index: usize) -> String {
    part["id"]
        .as_str()
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        })
        .map_or_else(|| format!("ordinal:{index}"), str::to_owned)
}

/// A full inspection may read one bounded page of the newest running shell's
/// public output. It stays out of the cached history and the listing path.
pub(super) fn decorate_live_shell(
    native: &dyn Native,
    record: &Value,
    messages: &[Value],
    events: &mut Value,
) {
    let cwd = string(record, "cwd");
    if cwd.is_empty() {
        return;
    }
    for message in messages
        .iter()
        .rev()
        .take(32)
        .take_while(|m| m["type"] != "idle")
    {
        let id = string(message, "id");
        let shell = match string(message, "type") {
            "shell" if message["status"] == "running" => {
                Some((string(message, "shellID"), format!("{id}:result")))
            }
            "assistant" if message["time"]["completed"].is_null() => {
                message["content"].as_array().and_then(|parts| {
                    parts.iter().enumerate().rev().find_map(|(index, part)| {
                        let state = &part["state"];
                        let shell = string(&state["metadata"], "shellID");
                        (part["type"] == "tool"
                            && part["name"] == "shell"
                            && state["status"] == "running"
                            && native_id(shell, "sh_"))
                        .then(|| (shell, format!("{id}:{}:result", part_id(part, index))))
                    })
                })
            }
            _ => None,
        };
        let Some((shell, slot)) = shell.filter(|(shell, _)| native_id(shell, "sh_")) else {
            continue;
        };
        let Some(event) = events
            .as_array_mut()
            .into_iter()
            .flatten()
            .find(|event| event["type"] == "ToolProgress" && event["message_id"] == slot)
        else {
            return;
        };
        if let Some(text) = live_shell_text(native, cwd, shell) {
            event["detail"] = json!(text);
        }
        return;
    }
}

fn live_shell_text(native: &dyn Native, cwd: &str, shell: &str) -> Option<String> {
    const OUTPUT: u64 = 4096;
    let deadline = Instant::now() + Duration::from_millis(350);
    let path = format!(
        "/api/shell/{shell}/output?location%5Bdirectory%5D={}&limit={OUTPUT}",
        encode(cwd)
    );
    let read = |path| {
        let response = native.raw_until("GET", path, None, deadline).ok()?;
        (response.status == 200).then_some(response.body)
    };
    let first = read(&path)?;
    let start = first["data"]["size"]
        .as_u64()
        .unwrap_or(0)
        .saturating_sub(OUTPUT);
    let page = if start == 0 {
        first
    } else {
        read(&format!("{path}&cursor={start}"))?
    };
    let text = journal::clipped(&page["data"]["output"], OUTPUT as usize);
    if text.is_empty() {
        None
    } else if start == 0 {
        Some(text)
    } else {
        Some(format!("…\n{text}"))
    }
}

/// The native message or tool slot stays the same as its contents change.
fn activity(kind: &str, detail: String, tool: &str, at: f64, id: String) -> Value {
    let key = format!("opencode:{id}");
    let mut row = event(kind, detail, tool, at, id, Value::Null);
    row["activity_key"] = json!(key);
    row
}

fn compaction(message: &Value, at: f64, id: &str) -> Option<Value> {
    let (kind, detail) = match string(message, "status") {
        "running" => ("PreCompact", String::new()),
        "completed" => ("PostCompact", journal::clipped(&message["summary"], TEXT)),
        "failed" if message["error"]["type"] == "aborted" => ("CompactCancelled", String::new()),
        "failed" => ("CompactFailure", error_text(&message["error"])),
        _ => return None,
    };
    Some(activity(kind, detail, "", at, id.to_owned()))
}

fn tool_rows(rows: &mut Vec<Value>, part: &Value, id: &str, index: usize, at: f64) {
    let call = format!("{id}:{}", part_id(part, index));
    let name = string(part, "name");
    let state = &part["state"];
    let started = part["time"]["created"].as_f64().map_or(at, |t| t / 1000.0);
    rows.push(activity(
        "PreToolUse",
        shown(&state["input"]),
        name,
        started,
        call.clone(),
    ));
    let status = string(state, "status");
    let kind = match status {
        "running" => "ToolProgress",
        "completed" => "PostToolUse",
        "error" => "PostToolUseFailure",
        _ => return,
    };
    let mut parts = state["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|content| content["type"] == "text")
        .filter_map(|content| content["text"].as_str())
        .collect::<Vec<_>>();
    let error = (status == "error").then(|| error_text(&state["error"]));
    if let Some(error) = error.as_deref().filter(|error| !parts.contains(error)) {
        parts.push(error);
    }
    let ended = part["time"]["completed"]
        .as_f64()
        .map_or(started, |t| t / 1000.0);
    rows.push(activity(
        kind,
        journal::clipped(&json!(parts.join("\n")), TEXT),
        name,
        ended,
        format!("{call}:result"),
    ));
}

fn shell_rows(rows: &mut Vec<Value>, message: &Value, at: f64, id: &str) {
    let status = string(message, "status");
    let result = match status {
        "running" => "ToolProgress",
        "exited" if message["exit"] == 0 => "PostToolUse",
        "exited" | "timeout" | "killed" => "PostToolUseFailure",
        _ => return,
    };
    rows.push(activity(
        "PreToolUse",
        journal::clipped(&message["command"], TEXT),
        "shell",
        at,
        id.to_owned(),
    ));
    let output = journal::clipped(&message["output"]["output"], TEXT);
    rows.push(activity(
        result,
        output,
        "shell",
        at,
        format!("{id}:result"),
    ));
}

/// Public Activity events from the bounded native history window.
pub(super) fn rows(messages: &[Value]) -> Vec<Value> {
    let mut rows = Vec::new();
    for message in messages {
        let id = string(message, "id");
        let at = seconds(message);
        match string(message, "type") {
            "user" => rows.push(event(
                "UserPromptSubmit",
                string(message, "text").to_owned(),
                "",
                at,
                id.to_owned(),
                Value::Null,
            )),
            "assistant" => {
                let first = rows.len();
                let completed = message["time"]["completed"].as_f64();
                let mut reply_at = None;
                let mut texts = Vec::new();
                for (index, part) in message["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    match string(part, "type") {
                        "text" => {
                            reply_at.get_or_insert(rows.len());
                            texts.push(string(part, "text"));
                        }
                        "tool" => {
                            tool_rows(&mut rows, part, id, index, at);
                        }
                        _ => {}
                    }
                }
                let text = texts.join("\n");
                if let Some(index) = reply_at.filter(|_| !text.trim().is_empty()) {
                    let done = completed.map_or(at, |t| t / 1000.0);
                    rows.insert(
                        index,
                        event("AgentMessage", text, "", done, id.to_owned(), json!(id)),
                    );
                }
                // OpenCode still changes this message; history must not keep
                // these rows until it completes.
                if completed.is_none() {
                    for row in &mut rows[first..] {
                        row["streaming"] = json!(true);
                    }
                }
            }
            "compaction" => rows.extend(compaction(message, at, id)),
            "shell" => shell_rows(&mut rows, message, at, id),
            "idle" => {
                let (kind, detail) = match string(message, "outcome") {
                    "succeeded" => ("Stop", String::new()),
                    "failed" => ("StopFailure", error_text(&message["error"])),
                    other => ("Stop", journal::clipped(&json!(other), 200)),
                };
                rows.push(event(kind, detail, "", at, id.to_owned(), Value::Null));
            }
            _ => {}
        }
    }
    rows
}

/// The latest turn's failure: the newest idle message when it failed and no
/// newer prompt or turn superseded it.
pub(super) fn last_error(messages: &[Value]) -> Option<Value> {
    for message in messages.iter().rev() {
        match string(message, "type") {
            "user" => return None,
            "idle" if message["outcome"] == "failed" => {
                return Some(
                    json!({"id": message["id"], "message": error_text(&message["error"]), "at": seconds(message)}),
                );
            }
            "idle" => return None,
            _ => {}
        }
    }
    None
}

/// Inbox items waiting for this session. After Stop OpenCode keeps them
/// without running them (not delivered); when the session's next message
/// starts a run, the waiting message runs first and that message after it
/// (OpenCode 2.0.32, native `test_11`, one waiting message). Zerus never
/// posts them again.
pub(super) fn queued_rows(inbox: &Value, conversation: &str, stopped: bool) -> Vec<Value> {
    let items: Vec<_> = inbox["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "user" && valid(item))
        .take(INBOX)
        .collect();
    let mut ids = std::collections::HashSet::new();
    let mut duplicates = std::collections::HashSet::new();
    for item in &items {
        let id = string(item, "id");
        if !ids.insert(id) {
            duplicates.insert(id);
        }
    }
    items
        .into_iter()
        .map(|item| {
            let id = string(item, "id");
            let created = seconds(item);
            let mut row = event(
                "UserPromptSubmit",
                string(&item["payload"], "text").to_owned(),
                "",
                created,
                id.to_owned(),
                Value::Null,
            );
            row["delivery_state"] = json!(if stopped { "not_delivered" } else { "queued" });
            // The inbox describes the current native entry by ID, including
            // its own creation time. Older or incomplete rows still render.
            if !conversation.is_empty()
                && item["sessionID"] == conversation
                && item["delivery"] == "queue"
                && queued_message_id(id)
                && !duplicates.contains(id)
                && created > 0.0
                && created <= 8.64e12
            {
                row["queued_at"] = json!(created);
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn load(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/opencode2/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }
    fn chronological(page: Value) -> Vec<Value> {
        let mut m = page["data"].as_array().unwrap().clone();
        m.reverse();
        m
    }

    #[test]
    fn user_assistant_and_idle_messages_become_activity_events() {
        let rows = rows(&chronological(load("messages")));
        let types: Vec<_> = rows.iter().map(|r| r["type"].as_str().unwrap()).collect();
        assert_eq!(types, vec!["UserPromptSubmit", "AgentMessage", "Stop"]);
        assert_eq!(rows[0]["detail"], "hello");
        assert_eq!(rows[1]["detail"], "Native OpenCode reply");
        assert_eq!(rows[1]["reply_id"], rows[1]["message_id"]);
        assert!(rows.iter().all(|r| r["source"] == "opencode_native"
            && r["message_id"].as_str().unwrap().starts_with("msg_")));
    }

    #[test]
    fn tool_parts_become_pre_and_post_tool_events() {
        let rows = rows(&chronological(load("messages_question")));
        let pre = rows.iter().find(|r| r["type"] == "PreToolUse").unwrap();
        assert_eq!(pre["tool"], "question");
        let post = rows.iter().find(|r| r["type"] == "PostToolUse").unwrap();
        assert!(post["detail"]
            .as_str()
            .unwrap()
            .contains("User has answered"));
    }

    #[test]
    fn compactions_are_distinct_ordered_reports_with_stable_identity() {
        let summary = "## Objective\nExample work\n\n### Next steps\n- Verify the fixture";
        let mut first = json!({"type":"compaction","id":"msg_first","time":{"created":1000},
            "status":"running","summary":summary,"providerContext":{"private":"opaque"}});
        let running = rows(&[first.clone()]);
        assert_eq!(running[0]["type"], "PreCompact");
        assert_eq!(running[0]["detail"], "");
        first["status"] = json!("completed");
        let mut second = first.clone();
        second["id"] = json!("msg_second");
        let rows = rows(&[
            first,
            json!({"type":"idle","id":"msg_middle","outcome":"succeeded"}),
            second,
        ]);
        assert_eq!(
            rows.iter()
                .map(|row| row["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["PostCompact", "Stop", "PostCompact"]
        );
        assert_eq!(rows[0]["detail"], summary);
        assert_eq!(rows[0]["activity_key"], running[0]["activity_key"]);
        assert_ne!(rows[0]["activity_key"], rows[2]["activity_key"]);
        assert!(!json!(rows).to_string().contains("opaque"));
    }

    #[test]
    fn tool_progress_and_public_results_keep_the_same_slot() {
        let mut message = json!({"type":"assistant","id":"msg_step","time":{"created":1000},"content":[
            {"type":"reasoning","text":"opaque reasoning"},
            {"type":"tool","id":"unique_call","name":"shell","time":{"created":2000},"state":{
                "status":"running","input":{"command":"printf example"},"metadata":{"private":"opaque tool metadata"}}}]});
        let mut streaming = message.clone();
        streaming["content"][1]["state"]["status"] = json!("streaming");
        assert_eq!(rows(&[streaming]).len(), 1);
        let progress = rows(&[message.clone()]);
        assert_eq!(progress[1]["type"], "ToolProgress");
        assert_eq!(progress[1]["streaming"], true);
        message["time"]["completed"] = json!(3000);
        message["content"][1]["time"]["completed"] = json!(3000);
        let state = &mut message["content"][1]["state"];
        state["status"] = json!("error");
        state["error"] = json!({"message":"Shell failed","private":"opaque error"});
        state["content"] = json!([{"type":"text","text":"public output"}]);
        let completed = rows(&[message]);
        assert_eq!(completed[1]["activity_key"], progress[1]["activity_key"]);
        assert_eq!(completed[1]["type"], "PostToolUseFailure");
        assert_eq!(completed[1]["at"], 3.0);
        assert_eq!(completed[1]["detail"], "public output\nShell failed");
        assert!(!json!(completed).to_string().contains("opaque"));
    }

    #[test]
    fn only_the_latest_active_shell_gets_a_bounded_public_output_read() {
        let running = |id, shell_id| {
            json!({
                "type":"assistant","id":id,"content":[{
                    "type":"tool","id":"call_1","name":"shell","state":{
                        "status":"running","metadata":{"shellID":shell_id}
                    }
                }]
            })
        };
        let mut current = running("msg_current", "sh_earlier");
        current["content"][0]["id"] = json!("1");
        let mut next = running("msg_current", "sh_current")["content"][0].clone();
        next["id"] = json!("call:second");
        current["content"].as_array_mut().unwrap().push(next);
        let messages = [current];
        let record = json!({"cwd":"/fixture directory"});
        let native = pages(vec![
            json!({"data":{"output":"old prefix","cursor":4096,"size":6000}}),
            json!({"data":{"output":"public output","private":"opaque","cursor":6000,"size":6000}}),
        ]);
        let mut events = json!(rows(&messages));
        decorate_live_shell(&native, &record, &messages, &mut events);
        let path = "/api/shell/sh_current/output?location%5Bdirectory%5D=%2Ffixture%20directory&limit=4096";
        assert_eq!(
            *native.paths.borrow(),
            [path, &format!("{path}&cursor=1904")]
        );
        assert_eq!(events[1]["detail"], "");
        assert_eq!(events[3]["detail"], "…\npublic output");
        assert_ne!(events[1]["activity_key"], events[3]["activity_key"]);
        assert!(!events.to_string().contains("opaque"));

        let native = pages(vec![]);
        let messages = [
            messages[0].clone(),
            json!({"type":"idle","id":"msg_idle"}),
            running("msg_invalid", "sh_wrong?path"),
        ];
        decorate_live_shell(&native, &record, &messages, &mut json!(rows(&messages)));
        assert!(native.paths.borrow().is_empty());
    }

    #[test]
    fn native_shell_history_exposes_its_bounded_output() {
        let rows = rows(&[
            json!({"type":"shell","id":"msg_shell","command":"printf example",
            "status":"exited","exit":0,"output":{"output":"example","cursor":7,"size":7}}),
        ]);
        assert_eq!(rows[0]["detail"], "printf example");
        assert_eq!(rows[1]["type"], "PostToolUse");
        assert_eq!(rows[1]["detail"], "example");
        assert_ne!(rows[0]["activity_key"], rows[1]["activity_key"]);
    }

    #[test]
    fn a_failed_idle_is_a_provider_error() {
        let messages = chronological(load("messages_error"));
        let error = last_error(&messages).unwrap();
        assert_eq!(error["message"], "Fixture provider rejected");
        assert!(rows(&messages).iter().any(|r| r["type"] == "StopFailure"));
    }

    #[test]
    fn inbox_items_are_queued_and_after_stop_not_delivered() {
        let inbox = load("inbox_busy");
        let entry = &inbox["data"][0];
        let conversation = string(entry, "sessionID");
        let queued = queued_rows(&inbox, conversation, false);
        assert_eq!(queued[0]["type"], "UserPromptSubmit");
        assert_eq!(queued[0]["delivery_state"], "queued");
        assert_eq!(queued[0]["detail"], "queued later");
        assert_eq!(queued[0]["queued_at"], seconds(entry));
        let stopped = queued_rows(&inbox, conversation, true);
        assert_eq!(stopped[0]["delivery_state"], "not_delivered");
        assert_eq!(stopped[0]["queued_at"], queued[0]["queued_at"]);
    }

    #[test]
    fn tool_events_have_unique_ids_and_carry_input_and_result() {
        let rows = rows(&chronological(load("messages_question")));
        let tools: Vec<_> = rows
            .iter()
            .filter(|r| r["type"] == "PreToolUse" || r["type"] == "PostToolUse")
            .collect();
        assert_eq!(tools.len(), 2);
        assert_ne!(tools[0]["message_id"], tools[1]["message_id"]);
        assert_eq!(
            tools[0]["message_id"],
            "msg_14bb2df17001mGPCg2h4EGlMmX:call_mgrimcii"
        );
        assert!(tools[0]["detail"]
            .as_str()
            .unwrap()
            .contains("Pick a color"));
        assert_eq!(tools[1]["tool"], "question");
        assert!(tools.iter().all(|r| r["reply_id"].is_null()));
        // Every event ID is unique, so Activity and history never merge rows.
        let mut ids: Vec<_> = rows.iter().map(|r| r["message_id"].to_string()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), rows.len());
    }

    #[test]
    fn an_assistant_message_joins_its_text_where_it_starts() {
        let message = json!({"id":"msg_A","type":"assistant","time":{"created":2000},"content":[
            {"type":"text","text":"Checking"},
            {"type":"tool","id":"call_1","name":"bash","state":{"status":"error","input":{"command":"x"},"error":"denied"}},
            {"type":"reasoning","text":"hidden"},
            {"type":"text","text":"Done"}]});
        let rows = rows(&[message]);
        let types: Vec<_> = rows.iter().map(|r| string(r, "type")).collect();
        assert_eq!(types, ["AgentMessage", "PreToolUse", "PostToolUseFailure"]);
        assert_eq!(rows[0]["detail"], "Checking\nDone");
        assert_eq!(rows[0]["reply_id"], "msg_A");
        assert_eq!(rows[0]["at"], 2.0);
        assert_eq!(rows[2]["detail"], "denied");
    }

    #[test]
    fn rows_of_a_streaming_reply_are_marked_until_it_completes() {
        let mut message = json!({"id":"msg_A","type":"assistant","time":{"created":2000},"content":[
            {"type":"text","text":"Partial"},
            {"type":"tool","id":"call_1","name":"bash","state":{"status":"running","input":{}}}]});
        let streaming = rows(std::slice::from_ref(&message));
        assert_eq!(streaming.len(), 3);
        assert!(streaming.iter().all(|r| r["streaming"] == true));
        message["time"]["completed"] = json!(3500);
        let done = rows(&[message]);
        assert!(done.iter().all(|r| r.get("streaming").is_none()));
        // A finished reply is dated when it completed; its tools when it started.
        assert_eq!(done[0]["at"], 3.5);
        assert_eq!(done[1]["at"], 2.0);
        let user = rows(&[json!({"id":"msg_U","type":"user","text":"x","time":{"created":1}})]);
        assert!(user[0].get("streaming").is_none());
    }

    #[test]
    fn other_idle_outcomes_stop_with_their_outcome_and_clear_errors() {
        let idle = |id: &str, outcome: &str| json!({"id":id,"type":"idle","outcome":outcome,"time":{"created":1}});
        let rows = rows(&[idle("msg_A", "interrupted")]);
        assert_eq!(rows[0]["type"], "Stop");
        assert_eq!(rows[0]["detail"], "interrupted");
        let mut failed = chronological(load("messages_error"));
        // A later successful turn or a new prompt supersedes the failure.
        failed.push(idle("msg_B", "succeeded"));
        assert!(last_error(&failed).is_none());
        let mut prompted = chronological(load("messages_error"));
        prompted.push(json!({"id":"msg_C","type":"user","text":"again","time":{"created":2}}));
        assert!(last_error(&prompted).is_none());
        let error = last_error(&chronological(load("messages_error"))).unwrap();
        assert_eq!(error["id"], "msg_14bb546e7001QR5FwtWF0VburA");
    }

    #[test]
    fn queued_rows_skip_foreign_and_invalid_items() {
        let inbox = json!({"data":[
            {"id":"../x","type":"user","payload":{"text":"bad"},"time":{"created":1}},
            {"id":"msg_A","type":"other","payload":{"text":"skip"},"time":{"created":1}},
            {"id":"msg_B","type":"user","payload":{"text":"keep"},"time":{"created":3000}}]});
        let rows = queued_rows(&inbox, "ses_fixture", false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["message_id"], "msg_B");
        assert_eq!(rows[0]["at"], 3.0);
        assert!(rows[0].get("queued_at").is_none());
        assert!(queued_rows(&json!({}), "ses_fixture", true).is_empty());
    }

    #[test]
    fn timestamps_follow_unique_exact_inbox_entries_even_when_text_matches() {
        let entry = |id, time, session| {
            json!({"id":id,"type":"user","delivery":"queue",
            "sessionID":session,"payload":{"text":"same"},"time":{"created":time}})
        };
        let a = entry("msg_A", 3000, "ses_fixture");
        let b = entry("msg_B", 4000, "ses_fixture");
        let c = entry("msg_C", 5000, "ses_other");
        let project = |data| queued_rows(&json!({"data":data}), "ses_fixture", false);
        let first = project(vec![a.clone(), b.clone(), c]);
        assert_eq!(
            first
                .iter()
                .map(|row| row["queued_at"].as_f64())
                .collect::<Vec<_>>(),
            [Some(3.0), Some(4.0), None]
        );
        let reinserted = project(vec![b, entry("msg_D", 6000, "ses_fixture")]);
        assert_eq!(reinserted[1]["message_id"], "msg_D");
        assert_eq!(reinserted[1]["queued_at"], 6.0);
        let duplicate = project(vec![a, entry("msg_A", 6000, "ses_fixture")]);
        assert!(duplicate.iter().all(|row| row.get("queued_at").is_none()));
    }

    /// Serves scripted `/message` pages and records each requested path.
    struct Pages {
        pages: std::cell::RefCell<Vec<Value>>,
        paths: std::cell::RefCell<Vec<String>>,
    }
    impl Native for Pages {
        fn raw(&self, method: &str, path: &str, _: Option<&Value>) -> Result<Response> {
            assert_eq!(method, "GET");
            self.paths.borrow_mut().push(path.to_owned());
            let body = self.pages.borrow_mut().remove(0);
            Ok(Response { status: 200, body })
        }
    }
    fn pages(pages: Vec<Value>) -> Pages {
        Pages {
            pages: std::cell::RefCell::new(pages),
            paths: Default::default(),
        }
    }
    fn message(id: &str, text: &str) -> Value {
        json!({"id":id,"type":"user","text":text,"time":{"created":1}})
    }
    /// A newest-first page of these IDs (given oldest first) and a cursor.
    fn listed(ids: &[&str], next: Option<&str>) -> Value {
        let data: Vec<_> = ids.iter().rev().map(|id| message(id, "x")).collect();
        json!({"data": data, "cursor": {"previous": null, "next": next}})
    }
    fn ids(page: &Page) -> Vec<&str> {
        page.messages.iter().map(|m| string(m, "id")).collect()
    }
    fn record() -> Value {
        json!({"conversation_id": "ses_A"})
    }
    fn later() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn pages_are_followed_newest_first_and_returned_in_order() {
        let native = pages(vec![
            listed(&["msg_3", "msg_4"], Some("next/+=")),
            listed(&["msg_1", "msg_2"], Some("again")),
            listed(&[], None),
        ]);
        let (page, changed) = scan(&native, &record(), None, Bounds::default(), later()).unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_2", "msg_3", "msg_4"]);
        assert!(!page.truncated && changed);
        assert_eq!(
            *native.paths.borrow(),
            [
                "/api/session/ses_A/message?limit=100&order=desc",
                "/api/session/ses_A/message?limit=100&cursor=next%2F%2B%3D",
                "/api/session/ses_A/message?limit=100&cursor=again",
            ]
        );
    }

    #[test]
    fn the_cache_supplies_older_messages_and_the_first_page_stays_fresh() {
        let cached = Cache {
            messages: vec![
                message("msg_1", "a"),
                message("msg_2", "b"),
                message("msg_3", "old"),
            ],
            truncated: false,
        };
        // The newest message changed in place (streaming) and one was added;
        // the page reaches back into the cache, so no further page is read.
        let mut first = listed(&["msg_3", "msg_4"], Some("more"));
        first["data"][1] = message("msg_3", "new");
        let native = pages(vec![first]);
        let (page, changed) = scan(
            &native,
            &record(),
            Some(&cached),
            Bounds::default(),
            later(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_2", "msg_3", "msg_4"]);
        assert_eq!(page.messages[2]["text"], "new");
        assert!(changed && !page.truncated);
        assert_eq!(native.paths.borrow().len(), 1);
        // Same newest message: reuse without rewriting the cache.
        let native = pages(vec![page_of(&page.messages[2..])]);
        let cached = Cache {
            messages: page.messages.clone(),
            truncated: false,
        };
        let (again, changed) = scan(
            &native,
            &record(),
            Some(&cached),
            Bounds::default(),
            later(),
        )
        .unwrap();
        assert_eq!(ids(&again), ids(&page));
        assert!(!changed);
    }
    fn page_of(messages: &[Value]) -> Value {
        let data: Vec<_> = messages.iter().rev().cloned().collect();
        json!({"data": data, "cursor": {"previous": null, "next": "more"}})
    }

    #[test]
    fn a_cached_message_read_again_is_never_listed_twice() {
        let cached = Cache {
            messages: vec![
                message("msg_1", "x"),
                message("msg_2", "x"),
                message("msg_3", "x"),
            ],
            truncated: false,
        };
        // The service orders msg_2 after msg_3, unlike the cache.
        let native = pages(vec![listed(&["msg_3", "msg_2", "msg_4"], Some("c"))]);
        let (page, _) = scan(
            &native,
            &record(),
            Some(&cached),
            Bounds::default(),
            later(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_3", "msg_2", "msg_4"]);
    }

    #[test]
    fn a_cache_that_does_not_connect_is_ignored() {
        let cached = Cache {
            messages: vec![message("msg_other", "x")],
            truncated: true,
        };
        let native = pages(vec![listed(&["msg_1"], None)]);
        let (page, _) = scan(
            &native,
            &record(),
            Some(&cached),
            Bounds::default(),
            later(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["msg_1"]);
        assert!(!page.truncated);
    }

    #[test]
    fn bounds_keep_the_newest_window_and_report_truncation() {
        let bounds = Bounds {
            messages: 3,
            bytes: usize::MAX,
        };
        let native = pages(vec![
            listed(&["msg_3", "msg_4"], Some("c1")),
            listed(&["msg_1", "msg_2"], Some("c2")),
        ]);
        let (page, _) = scan(&native, &record(), None, bounds, later()).unwrap();
        assert_eq!(ids(&page), ["msg_2", "msg_3", "msg_4"]);
        assert!(page.truncated);
        // The byte budget applies to the spliced list too.
        let size = message("msg_1", "x").to_string().len();
        let bounds = Bounds {
            messages: 100,
            bytes: size * 2,
        };
        let cached = Cache {
            messages: vec![message("msg_1", "x"), message("msg_2", "x")],
            truncated: false,
        };
        let native = pages(vec![listed(&["msg_2", "msg_3"], Some("c"))]);
        let (page, _) = scan(&native, &record(), Some(&cached), bounds, later()).unwrap();
        assert_eq!(ids(&page), ["msg_2", "msg_3"]);
        assert!(page.truncated);
    }

    #[test]
    fn a_passed_deadline_or_a_repeated_page_stops_with_the_newest_window() {
        let native = pages(vec![listed(&["msg_1", "msg_2"], Some("c"))]);
        let (page, _) = scan(&native, &record(), None, Bounds::default(), Instant::now()).unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_2"]);
        assert!(page.truncated);
        let native = pages(vec![
            listed(&["msg_1", "msg_2"], Some("c")),
            listed(&["msg_1", "msg_2"], Some("c")),
        ]);
        let (page, _) = scan(&native, &record(), None, Bounds::default(), later()).unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_2"]);
        assert!(page.truncated);
    }

    #[test]
    fn invalid_pages_and_messages_are_refused_or_skipped() {
        let native = pages(vec![json!({"data": {}})]);
        assert!(scan(&native, &record(), None, Bounds::default(), later()).is_err());
        let mut odd = listed(&["msg_1"], None);
        odd["data"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"bad id","type":"user","text":"x"}));
        let native = pages(vec![odd]);
        let (page, _) = scan(&native, &record(), None, Bounds::default(), later()).unwrap();
        assert_eq!(ids(&page), ["msg_1"]);
    }

    fn aged(path: &Path, seconds: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        let then = std::time::SystemTime::now() - Duration::from_secs(seconds);
        file.set_modified(then).unwrap();
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn pruning_drops_the_least_recently_saved_caches_beyond_the_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let name = |n: u8| format!("{}.json", format!("{n:x}").repeat(64));
        // Saved oldest to newest: 1, 2, 3, 4, 5.
        for n in 1..=5u8 {
            std::fs::write(dir.path().join(name(n)), [b'x'; 10]).unwrap();
            aged(&dir.path().join(name(n)), 100 - u64::from(n));
        }
        // 3 was just saved: it stays although 4 and 5 are newer.
        let keep = dir.path().join(name(3));
        prune(dir.path(), &keep, 3, u64::MAX);
        assert_eq!(names(dir.path()), [name(3), name(4), name(5)]);
        // 3 and 5 fit in 20 bytes; 4 does not.
        prune(dir.path(), &keep, 10, 20);
        assert_eq!(names(dir.path()), [name(3), name(5)]);
        // The kept cache stays even when it alone exceeds the byte bound.
        prune(dir.path(), &keep, 10, 1);
        assert_eq!(names(dir.path()), [name(3)]);
    }

    #[test]
    fn pruning_keeps_other_names_and_fresh_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join(format!("{}.json", "a".repeat(64)));
        std::fs::write(&keep, b"x").unwrap();
        // Upper case is not a cache name; "C" aliases no other file on
        // case-insensitive file systems.
        for name in [
            ".hgs-old",
            ".hgs-fresh",
            "notes.json",
            &format!("{}.json", "C".repeat(64)),
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join(format!("{}.json", "b".repeat(64)))).unwrap();
        aged(&dir.path().join(".hgs-old"), 7200);
        prune(dir.path(), &keep, 1, 1);
        let mut expected = vec![
            ".hgs-fresh".to_owned(),
            format!("{}.json", "C".repeat(64)),
            format!("{}.json", "a".repeat(64)),
            format!("{}.json", "b".repeat(64)),
            "notes.json".to_owned(),
        ];
        expected.sort();
        assert_eq!(names(dir.path()), expected);
    }

    #[test]
    fn saving_a_cache_prunes_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let page = Page {
            messages: vec![message("msg_1", "x")],
            truncated: true,
        };
        for n in 0..CACHE_FILES {
            let path = dir.path().join(format!("{n:064x}.json"));
            std::fs::write(&path, b"{}").unwrap();
            aged(&path, 1000 + n as u64);
        }
        let path = dir.path().join(format!("{}.json", "f".repeat(64)));
        save_at(&path, &record(), &page);
        assert_eq!(names(dir.path()).len(), CACHE_FILES);
        assert!(path.exists());
        // The oldest older cache went first.
        assert!(!dir
            .path()
            .join(format!("{:064x}.json", CACHE_FILES - 1))
            .exists());
    }

    #[test]
    fn a_read_publishes_its_cache_only_for_the_exact_current_binding() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache.json");
        let page = Page {
            messages: vec![message("msg_1", "x")],
            truncated: false,
        };
        let a = json!({"name": "opencode/p/t", "run_id": "run-a", "conversation_id": "ses_A"});
        // Same identity: saved.
        publish_locked(&a, &cache, &page, Some(&a));
        assert!(load_at(&cache, &a).is_some());
        // Forget of A, then the same name re-created as B: A's late read
        // publishes nothing and removes A's cache.
        let b = json!({"name": "opencode/p/t", "run_id": "run-b", "conversation_id": "ses_B"});
        publish_locked(&a, &cache, &page, Some(&b));
        assert!(!cache.exists());
        // Same conversation under a new run is another binding too.
        let rerun = json!({"name": "opencode/p/t", "run_id": "run-c", "conversation_id": "ses_A"});
        publish_locked(&a, &cache, &page, Some(&rerun));
        assert!(!cache.exists());
        // Forgotten (no binding): nothing saved.
        publish_locked(&a, &cache, &page, None);
        assert!(!cache.exists());
    }

    #[test]
    fn a_read_saves_nothing_without_the_lock_and_rechecks_the_binding_under_it() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache.json");
        let page = Page {
            messages: vec![message("msg_1", "x")],
            truncated: false,
        };
        let a = json!({"name": "opencode/p/t", "run_id": "run-a", "conversation_id": "ses_A"});
        let b = json!({"name": "opencode/p/t", "run_id": "run-b", "conversation_id": "ses_B"});
        // Lock busy: nothing is saved, the binding is not even read.
        publish_with(
            || None::<()>,
            || panic!("read without the lock"),
            &a,
            &cache,
            &page,
        );
        assert!(!cache.exists());
        // Under the lock the binding is read then: swapped mid-read to B.
        std::fs::write(&cache, b"{}").unwrap();
        publish_with(|| Some(()), || Some(b.clone()), &a, &cache, &page);
        assert!(!cache.exists());
        // Unchanged binding: saved.
        publish_with(|| Some(()), || Some(a.clone()), &a, &cache, &page);
        assert!(load_at(&cache, &a).is_some());
    }

    #[test]
    fn a_changed_read_is_published_only_under_the_lock_for_its_exact_binding() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache.json");
        let a = json!({"name": "opencode/p/t", "run_id": "run-a", "conversation_id": "ses_A"});
        let b = json!({"name": "opencode/p/t", "run_id": "run-b", "conversation_id": "ses_B"});
        // Lock busy: the read still answers, saves nothing and reads no binding.
        let native = pages(vec![listed(&["msg_1"], None)]);
        let page =
            fetch_published(&native, &a, &cache, || None::<()>, || panic!("unlocked")).unwrap();
        assert_eq!(ids(&page), ["msg_1"]);
        assert!(!cache.exists());
        // Under the lock for the exact binding: saved.
        let native = pages(vec![listed(&["msg_1"], None)]);
        fetch_published(&native, &a, &cache, || Some(()), || Some(a.clone())).unwrap();
        assert!(load_at(&cache, &a).is_some());
        // The name was re-created during a changed read: its cache is removed.
        let native = pages(vec![listed(&["msg_1", "msg_2"], None)]);
        let page = fetch_published(&native, &a, &cache, || Some(()), || Some(b.clone())).unwrap();
        assert_eq!(ids(&page), ["msg_1", "msg_2"]);
        assert!(!cache.exists());
    }

    #[test]
    fn using_a_cache_makes_it_recent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, b"{}").unwrap();
        aged(&path, 1000);
        touch(&path);
        let age = std::fs::metadata(&path)
            .unwrap()
            .modified()
            .unwrap()
            .elapsed()
            .unwrap();
        assert!(age < Duration::from_secs(100), "{age:?}");
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    }

    #[test]
    fn the_cache_file_round_trips_only_for_its_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let page = Page {
            messages: vec![message("msg_1", "x")],
            truncated: true,
        };
        save_at(&path, &record(), &page);
        let cache = load_at(&path, &record()).unwrap();
        assert_eq!(cache.messages, page.messages);
        assert!(cache.truncated);
        assert!(load_at(&path, &json!({"conversation_id": "ses_B"})).is_none());
        assert_ne!(
            cache_path(&record()),
            cache_path(&json!({"conversation_id": "ses_B"}))
        );
    }
}
