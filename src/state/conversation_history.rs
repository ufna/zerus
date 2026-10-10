//! Bounded public history indexing. Source files remain native-authoritative;
//! derived files contain only public message projections, never raw transcripts.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

const PUBLIC: &[&str] = &[
    "UserPromptSubmit",
    "UserPromptQueued",
    "UserMessage",
    "TurnStarted",
    "QuestionAnswered",
    "AgentMessage",
    "Stop",
];
const BUDGET: usize = 2 * 1024 * 1024;
const ROW_BYTES: usize = 32 * 1024;
const RECONCILE_VERSION: &str = "2";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    request_id: String,
    expected_run_id: String,
    expected_conversation_id: String,
    #[serde(default)]
    archive_id: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    around: Option<String>,
    #[serde(default)]
    around_incoming_seq: Option<i64>,
    #[serde(default)]
    history_epoch: Option<String>,
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn validate(r: &Request) -> Result<()> {
    let id = uuid::Uuid::parse_str(&r.request_id).map_err(|_| "Invalid history request UUID")?;
    if id.is_nil()
        || id.to_string() != r.request_id
        || r.expected_run_id.is_empty()
        || r.expected_run_id.len() > 128
        || r.expected_conversation_id.is_empty()
        || r.expected_conversation_id.len() > 256
        || [&r.expected_run_id, &r.expected_conversation_id]
            .iter()
            .any(|v| v.chars().any(char::is_control))
    {
        return Err("Invalid exact history identity".into());
    }
    if !matches!(r.limit.unwrap_or(100), 1..=100)
        || [
            r.before.is_some(),
            r.after.is_some(),
            r.around.is_some(),
            r.around_incoming_seq.is_some(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
            > 1
    {
        return Err("Choose one bounded history selector".into());
    }
    for v in [&r.before, &r.after, &r.around].into_iter().flatten() {
        if v.is_empty() || v.len() > 2048 {
            return Err("Invalid opaque history cursor".into());
        }
    }
    if r.around_incoming_seq.is_some_and(|v| v < 0)
        || r.around_incoming_seq.is_some() != r.history_epoch.is_some()
    {
        return Err("Incoming sequence requires its exact history epoch".into());
    }
    if let Some(v) = &r.agent_id {
        if v.is_empty()
            || v.len() > 160
            || v.contains(['/', '\\'])
            || v.chars().any(char::is_control)
        {
            return Err("Invalid child history identity".into());
        }
    }
    if let Some(v) = &r.archive_id {
        if uuid::Uuid::parse_str(v)
            .map(|id| id.to_string() != *v || id.is_nil())
            .unwrap_or(true)
        {
            return Err("Invalid archive identity".into());
        }
    }
    Ok(())
}
fn record(name: &str, r: &Request) -> Result<Value> {
    let value = if dsh::exists(name) {
        if r.archive_id.is_some() {
            return Err("DeepSeek has no archive history".into());
        }
        dsh::binding(name)?
    } else {
        let _guard = lock(None)?;
        if let Some(id) = &r.archive_id {
            archive::read_archive(name, id)?
        } else {
            read(name)?
        }
    };
    if string(&value, "name") != name
        || string(&value, "run_id") != r.expected_run_id
        || string(&value, "conversation_id") != r.expected_conversation_id
        || (r.archive_id.is_none() && !string(&value, "archive_id").is_empty())
    {
        return Err("History target changed; refresh its exact identity".into());
    }
    if let Some(id) = &r.agent_id {
        if !value["subagents"].get(id).is_some_and(Value::is_object)
            && string(&value, "agent") != "dsh"
        {
            return Err("Child is not in this exact parent conversation".into());
        }
    }
    Ok(value)
}
fn native_record(parent: &Value, r: &Request) -> Value {
    let mut result = parent.clone();
    if let Some(id) = &r.agent_id {
        let child = &parent["subagents"][id];
        if child["external"] == true {
            for (to, from) in [
                ("agent", "provider"),
                ("conversation_id", "conversation_id"),
                ("transcript", "transcript"),
                ("cwd", "cwd"),
                ("agent_home", "agent_home"),
            ] {
                result[to] = child[from].clone();
            }
        }
    }
    result
}
fn key(parent: &Value, r: &Request) -> String {
    hash(
        &json!([
            parent["agent"],
            parent["agent_home"],
            parent["account_id"],
            parent["conversation_id"],
            r.agent_id
        ])
        .to_string(),
    )
}
fn index(parent: &Value, r: &Request) -> Result<(Connection, Guard)> {
    let directory = absolute_root()?.join("public_history");
    private_dir(&directory)?;
    let info = directory.symlink_metadata().map_err(|e| e.to_string())?;
    if !info.is_dir()
        || info.permissions().mode() & 0o077 != 0
        || info.uid() != unsafe { libc::geteuid() }
    {
        return Err("Public history index directory must be private".into());
    }
    let key = key(parent, r);
    let guard = lock(Some(&directory.join(format!("{key}.lock"))))?;
    let path = directory.join(format!("{key}.sqlite3"));
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o077 != 0
        || meta.nlink() != 1
    {
        return Err("Public history index file must be private".into());
    }
    drop(file);
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;
 CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY,at REAL NOT NULL,role TEXT NOT NULL,stream TEXT NOT NULL,kind TEXT NOT NULL,ordinal TEXT NOT NULL,body TEXT NOT NULL,incoming INTEGER);
 CREATE INDEX IF NOT EXISTS message_order ON messages(at,ordinal,id); CREATE INDEX IF NOT EXISTS incoming_order ON messages(incoming) WHERE incoming IS NOT NULL;
 CREATE INDEX IF NOT EXISTS message_candidates ON messages(role,at);
 CREATE TABLE IF NOT EXISTS aliases(original TEXT PRIMARY KEY,id TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS receipt_seen(name TEXT PRIMARY KEY,stamp TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS reconcile_work(id TEXT PRIMARY KEY);").map_err(|e|e.to_string())?;
    if get(&db, "schema")?.as_deref() != Some("2") {
        let has = db
            .prepare("PRAGMA table_info(messages)")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .filter_map(std::result::Result::ok)
            .any(|v| v == "ordinal");
        if !has {
            db.execute_batch("ALTER TABLE messages ADD COLUMN ordinal TEXT NOT NULL DEFAULT ''; ")
                .map_err(|e| e.to_string())?;
        }
        db.execute_batch("DELETE FROM messages;DELETE FROM aliases;DELETE FROM receipt_seen;DELETE FROM reconcile_work;DELETE FROM meta;").map_err(|e|e.to_string())?;
        set(&db, "schema", "2")?;
    }
    if get(&db, "epoch")?.is_none() {
        // A new index has no old projections to migrate.
        set(&db, "reconcile_version", RECONCILE_VERSION)?;
        set(&db, "epoch", &uuid::Uuid::new_v4().to_string())?;
        set(
            &db,
            "secret",
            &(uuid::Uuid::new_v4().simple().to_string()
                + &uuid::Uuid::new_v4().simple().to_string()),
        )?;
    }
    Ok((db, guard))
}
fn get(db: &Connection, key: &str) -> Result<Option<String>> {
    db.query_row("SELECT value FROM meta WHERE key=?", [key], |row| {
        row.get(0)
    })
    .optional()
    .map_err(|e| e.to_string())
}
fn set(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(|e|e.to_string())?;
    Ok(())
}
fn original(event: &Value, stream: &str) -> Option<String> {
    let kind = string(event, "type");
    let role = if kind == "QuestionAnswered" {
        "You (answer)"
    } else if [
        "UserPromptSubmit",
        "UserPromptQueued",
        "UserMessage",
        "TurnStarted",
    ]
    .contains(&kind)
    {
        "You"
    } else {
        "AgentMessage"
    };
    let source = if string(event, "source").is_empty() {
        stream
    } else {
        string(event, "source")
    };
    let id = event
        .get("message_id")
        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        .or_else(|| event.get("seq"))
        .or_else(|| event.get("source_offset"))?;
    let text = if let Some(v) = id.as_str() {
        v.to_owned()
    } else {
        id.to_string()
    };
    Some(format!("{stream}:{source}:{role}:native:{text}"))
}
fn add(db: &Connection, canonical: &str, mut row: Value, stream: &str) -> Result<()> {
    if !PUBLIC.contains(&string(&row, "type"))
        || (string(&row, "detail").trim().is_empty()
            && row["attachments"].as_array().is_none_or(Vec::is_empty))
    {
        return Ok(());
    }
    let detail = string(&row, "detail").to_owned();
    if detail.len() > ROW_BYTES {
        let mut end = ROW_BYTES;
        while !detail.is_char_boundary(end) {
            end -= 1;
        }
        row["detail"] = json!(&detail[..end]);
        row["detail_truncated"] = json!(true);
    }
    let mut public = json!({});
    for field in [
        "type",
        "detail",
        "detail_truncated",
        "source",
        "at",
        "seq",
        "message_id",
        "agent_id",
        "reply_id",
        "message_phase",
        "request_id",
        "question_id",
        "question_hash",
        "answers",
        "attachments",
        "submitted_text",
        "source_offset",
        "native_match_hash",
    ] {
        if let Some(v) = row.get(field) {
            public[field] = v.clone();
        }
    }
    row = public;
    if string(&row, "message_id").len() > 256 {
        row.as_object_mut().unwrap().remove("message_id");
    }
    let incoming = ["AgentMessage", "Stop"].contains(&string(&row, "type"));
    let role = if incoming { "incoming" } else { "outgoing" };
    let origin = original(&row, stream).ok_or("Public row lacks stable provenance")?;
    let existing: Option<String> = db
        .query_row("SELECT id FROM aliases WHERE original=?", [&origin], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        if stream == "provider" || stream == "attachment" {
            let old: usize = db
                .query_row(
                    "SELECT length(CAST(body AS BLOB)) FROM messages WHERE id=?",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let encoded = row.to_string();
            let used = get(db, "bytes")?
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            let updated = used.saturating_sub(old) + encoded.len();
            if updated > 512 * 1024 * 1024 {
                set(db, "gap", "index_budget_exhausted")?;
                return Ok(());
            }
            set(db, "bytes", &updated.to_string())?;
            db.execute(
                "UPDATE messages SET body=?,kind=? WHERE id=?",
                params![row.to_string(), string(&row, "type"), id],
            )
            .map_err(|e| e.to_string())?;
            queue(db, &id, at_of(&row))?;
        }
        return Ok(());
    }
    let id = hash(&json!([canonical, origin]).to_string());
    let at = row["at"].as_f64().filter(|v| v.is_finite()).unwrap_or(0.);
    let ordinal = if stream == "journal" {
        format!("0:{:020}", row["seq"].as_i64().unwrap_or(0))
    } else if stream == "provider" {
        let offset = string(&row, "source_offset");
        let (file, byte) = offset.split_once(':').unwrap_or(("0", "0"));
        format!(
            "1:{:020}:{:020}",
            file.parse::<u64>().unwrap_or(0),
            byte.parse::<u64>().unwrap_or(0)
        )
    } else {
        format!("2:{}", string(&row, "message_id"))
    };
    let count: i64 = db
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let used = get(db, "bytes")?
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let cost = row.to_string().len() + origin.len() + 256;
    if count >= 100000 || used + cost > 512 * 1024 * 1024 {
        set(db, "gap", "index_budget_exhausted")?;
        return Ok(());
    }
    db.execute(
        "INSERT OR IGNORE INTO messages(id,at,role,stream,kind,ordinal,body) VALUES(?,?,?,?,?,?,?)",
        params![
            id,
            at,
            role,
            stream,
            string(&row, "type"),
            ordinal,
            row.to_string()
        ],
    )
    .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR IGNORE INTO aliases(original,id) VALUES(?,?)",
        params![origin, id],
    )
    .map_err(|e| e.to_string())?;
    set(db, "bytes", &(used + cost).to_string())?;
    queue(db, &id, at)?;
    Ok(())
}
fn at_of(value: &Value) -> f64 {
    value["at"].as_f64().filter(|v| v.is_finite()).unwrap_or(0.)
}
fn queue(db: &Connection, id: &str, at: f64) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO reconcile_work(id) VALUES(?)", [id])
        .map_err(|e| e.to_string())?;
    db.execute("INSERT OR IGNORE INTO reconcile_work(id) SELECT id FROM messages WHERE stream IN ('provider','attachment') AND at BETWEEN ? AND ?",params![at-10.,at+10.]).map_err(|e|e.to_string())?;
    Ok(())
}
fn paths(record: &Value) -> Result<Vec<PathBuf>> {
    if string(record, "agent") == "kimi" {
        return questions::wire_paths(record);
    }
    let id = string(record, "conversation_id");
    let path = PathBuf::from(string(record, "transcript"));
    if !["codex", "claude"].contains(&string(record, "agent"))
        || uuid::Uuid::parse_str(id).is_err()
        || !path.is_absolute()
        || !path
            .file_name()
            .is_some_and(|v| v.to_string_lossy().contains(id))
    {
        return Ok(vec![]);
    }
    Ok(vec![path])
}
fn sources(db: &Connection, parent: &Value, r: &Request, native: &Value) -> Result<(bool, Value)> {
    let paths = paths(native)?;
    if paths.is_empty() {
        return Ok((false, json!({"provider":"unavailable"})));
    }
    let mut descriptors = Vec::new();
    let mut files = Vec::new();
    for path in paths {
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(_) => return Ok((false, json!({"provider":"unavailable"}))),
        };
        let meta = file.metadata().map_err(|e| e.to_string())?;
        let mut first = Vec::new();
        BufReader::new((&mut file).take(256 * 1024))
            .read_until(b'\n', &mut first)
            .map_err(|e| e.to_string())?;
        if string(native, "agent") == "codex" {
            let first: Value =
                serde_json::from_slice(&first).map_err(|_| "Provider header cannot be verified")?;
            if first["type"] != "session_meta"
                || first["payload"]["id"] != native["conversation_id"]
            {
                return Err("Provider transcript identity mismatch".into());
            }
        }
        descriptors.push(json!([
            path,
            meta.dev(),
            meta.ino(),
            hash(&String::from_utf8_lossy(&first))
        ]));
        files.push((
            file,
            meta.len(),
            format!("{}:{}", meta.mtime(), meta.mtime_nsec()),
        ));
    }
    let descriptor = json!(descriptors).to_string();
    if get(db, "sources")?
        .as_deref()
        .is_some_and(|old| old != descriptor)
    {
        db.execute_batch("DELETE FROM messages;DELETE FROM aliases;DELETE FROM receipt_seen;DELETE FROM reconcile_work;DELETE FROM meta WHERE key NOT IN ('secret','schema');").map_err(|e|e.to_string())?;
        set(db, "epoch", &uuid::Uuid::new_v4().to_string())?;
    }
    set(db, "sources", &descriptor)?;
    let canonical = key(parent, r);
    let mut budget = BUDGET;
    let mut complete = true;
    let mut partial_final = false;
    for (number, (mut file, len, modified)) in files.into_iter().enumerate() {
        let offset_key = format!("offset:{number}");
        let mut offset = get(db, &offset_key)?
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let prior_len = get(db, &format!("length:{number}"))?.and_then(|v| v.parse::<u64>().ok());
        let changed_same_length = prior_len == Some(len)
            && get(db, &format!("modified:{number}"))?.is_some_and(|v| v != modified);
        let mut checkpoint = Vec::new();
        file.seek(SeekFrom::Start(offset.saturating_sub(256)))
            .map_err(|e| e.to_string())?;
        (&mut file)
            .take(offset.min(256))
            .read_to_end(&mut checkpoint)
            .map_err(|e| e.to_string())?;
        let boundary = format!("{:x}", Sha256::digest(&checkpoint));
        let changed_boundary =
            get(db, &format!("boundary:{number}"))?.is_some_and(|old| old != boundary);
        if len < offset || changed_same_length || changed_boundary {
            db.execute_batch("DELETE FROM messages;DELETE FROM aliases;DELETE FROM receipt_seen;DELETE FROM reconcile_work;DELETE FROM meta WHERE key NOT IN ('secret','schema');").map_err(|e|e.to_string())?;
            set(db, "epoch", &uuid::Uuid::new_v4().to_string())?;
            return Ok((false, json!({"provider":"source_reset"})));
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(budget as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        budget -= bytes.len();
        let mut consumed = 0;
        let mut skipping = get(db, &format!("skip:{number}"))?.as_deref() == Some("1");
        for part in bytes.split_inclusive(|byte| *byte == b'\n').take(512) {
            if !part.ends_with(b"\n") {
                if skipping || (consumed == 0 && part.len() == BUDGET) {
                    skipping = true;
                    consumed += part.len();
                    set(db, "gap", "oversized_source_line")?;
                } else if offset + bytes.len() as u64 == len {
                    partial_final = true;
                    // A final unterminated public row is useful, but cannot establish a
                    // complete source. Keep its offset for a later finalized revision.
                    if let Ok(value) = serde_json::from_slice::<Value>(part) {
                        if string(native, "agent") != "claude"
                            || string(&value, "sessionId").is_empty()
                            || value["sessionId"] == native["conversation_id"]
                        {
                            if let Some((mut row, _)) =
                                search::provider_event(string(native, "agent"), &value)
                            {
                                row["source_offset"] =
                                    json!(format!("{number}:{}", offset + consumed as u64));
                                add(db, &canonical, row, "provider")?;
                            }
                        }
                    }
                }
                break;
            }
            let position = offset + consumed as u64;
            consumed += part.len();
            if skipping {
                skipping = false;
                continue;
            }
            let value: Value = match serde_json::from_slice(part) {
                Ok(value) => value,
                Err(_) => {
                    set(db, "gap", "malformed_source_line")?;
                    continue;
                }
            };
            if string(native, "agent") == "claude"
                && !string(&value, "sessionId").is_empty()
                && value["sessionId"] != native["conversation_id"]
            {
                set(db, "gap", "source_identity_gap")?;
                continue;
            }
            if let Some((mut row, _)) = search::provider_event(string(native, "agent"), &value) {
                row["source_offset"] = json!(format!("{number}:{position}"));
                add(db, &canonical, row, "provider")?;
            }
        }
        offset += consumed as u64;
        set(db, &offset_key, &offset.to_string())?;
        let mut checkpoint = Vec::new();
        let mut file = File::open(
            &descriptors[number][0]
                .as_str()
                .ok_or("Missing source identity")?,
        )
        .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(offset.saturating_sub(256)))
            .map_err(|e| e.to_string())?;
        file.take(offset.min(256))
            .read_to_end(&mut checkpoint)
            .map_err(|e| e.to_string())?;
        set(
            db,
            &format!("boundary:{number}"),
            &format!("{:x}", Sha256::digest(&checkpoint)),
        )?;
        set(db, &format!("length:{number}"), &len.to_string())?;
        set(db, &format!("modified:{number}"), &modified)?;
        set(
            db,
            &format!("skip:{number}"),
            if skipping { "1" } else { "0" },
        )?;
        complete &= offset == len && !skipping;
    }
    Ok((
        complete,
        json!({"provider":if complete{"complete"}else if partial_final{"partial_final_line"}else{"indexing"}}),
    ))
}
fn journal(db: &Connection, parent: &Value, r: &Request) -> Result<bool> {
    let path = root().join("events.sqlite3");
    if !path.exists() {
        return Ok(true);
    }
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let offset = get(db, "journal_seq")?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let mut query=source.prepare("SELECT seq,payload FROM events WHERE name=? AND conversation IS ? AND seq>? ORDER BY seq LIMIT 1000").map_err(|e|e.to_string())?;
    let values = query
        .query_map(
            params![
                journal::journal_name(parent),
                parent["conversation_id"].as_str(),
                offset
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let canonical = key(parent, r);
    for (seq, text) in &values {
        let mut row: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(_) => {
                set(db, "gap", "malformed_journal_row")?;
                continue;
            }
        };
        let agent = string(&row, "agent_id");
        if if let Some(id) = &r.agent_id {
            agent != id
        } else {
            !["", "main"].contains(&agent)
        } {
            continue;
        }
        row["seq"] = json!(seq);
        if r.agent_id.is_some() {
            row["agent_id"] = json!("");
            if row["type"] == "SubagentStop" {
                row["type"] = json!("Stop");
            }
        }
        add(db, &canonical, row, "journal")?;
    }
    if let Some((seq, _)) = values.last() {
        set(db, "journal_seq", &seq.to_string())?;
    }
    Ok(values.len() < 1000)
}
fn receipts(db: &Connection, parent: &Value, r: &Request) -> Result<bool> {
    let directory = attachments::history_directory(parent);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(true);
    };
    let mut seen = std::collections::HashMap::<String, String>::new();
    let mut query = db
        .prepare("SELECT name,stamp FROM receipt_seen")
        .map_err(|e| e.to_string())?;
    for row in query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (name, stamp) = row.map_err(|e| e.to_string())?;
        seen.insert(name, stamp);
    }
    let mut paths = Vec::new();
    let mut count = 0;
    for entry in entries.flatten().take(100001) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        count += 1;
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let stamp = format!(
            "{}:{}:{}",
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        );
        if seen.get(&name) != Some(&stamp) {
            paths.push((name, stamp, entry.path()));
        }
    }
    if count >= 100000 {
        set(db, "gap", "receipt_catalog_budget_exhausted")?;
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    let complete = paths.len() <= 64;
    for (name, stamp, path) in paths.into_iter().take(64) {
        match attachments::history_receipt(parent, r.agent_id.as_deref().unwrap_or(""), &path) {
            Ok(Some((mut row, submitted))) => {
                row["native_match_hash"] = json!(hash(&submitted));
                add(db, &key(parent, r), row, "attachment")?;
            }
            Ok(None) => {}
            Err(_) => {
                set(db, "gap", "receipt_source_gap")?;
            }
        }
        db.execute("INSERT INTO receipt_seen(name,stamp) VALUES(?,?) ON CONFLICT(name) DO UPDATE SET stamp=excluded.stamp",params![name,stamp]).map_err(|e|e.to_string())?;
    }
    Ok(complete)
}
fn mac(secret: &str, text: &str) -> String {
    let mut key = [0u8; 64];
    let bytes = secret.as_bytes();
    key[..bytes.len()].copy_from_slice(bytes);
    let inner: Vec<u8> = key.iter().map(|v| v ^ 0x36).chain(text.bytes()).collect();
    let outer: Vec<u8> = key
        .iter()
        .map(|v| v ^ 0x5c)
        .chain(Sha256::digest(inner))
        .collect();
    format!("{:x}", Sha256::digest(outer))
}
fn cursor(secret: &str, epoch: &str, at: f64, ordinal: &str, id: &str) -> String {
    let text = json!([epoch, at, ordinal, id]).to_string();
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(text.as_bytes()),
        mac(secret, &text)
    )
}
fn decode(value: &str, secret: &str, epoch: &str) -> Result<(f64, String, String)> {
    let (body, signature) = value.split_once('.').ok_or("Invalid history cursor")?;
    let bytes = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| "Invalid history cursor")?;
    let text = String::from_utf8(bytes).map_err(|_| "Invalid history cursor")?;
    let expected = mac(secret, &text);
    if signature.len() != expected.len()
        || !signature
            .bytes()
            .zip(expected.bytes())
            .fold(true, |ok, (a, b)| ok & (a == b))
    {
        return Err("History cursor authentication failed".into());
    }
    let value: Value = serde_json::from_str(&text).map_err(|_| "Invalid history cursor")?;
    if value[0] != epoch {
        return Err("History epoch changed; reopen its verified head".into());
    }
    Ok((
        value[1]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("Invalid history position")?,
        value[2]
            .as_str()
            .ok_or("Invalid history position")?
            .to_owned(),
        value[3]
            .as_str()
            .ok_or("Invalid history position")?
            .to_owned(),
    ))
}
fn incoming(db: &Connection, complete: bool) -> Result<Option<i64>> {
    if !complete {
        return Ok(None);
    }
    let historical:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM messages m WHERE m.role='incoming' AND m.incoming IS NULL AND EXISTS(SELECT 1 FROM messages old WHERE old.incoming IS NOT NULL AND (m.at,m.ordinal,m.id)<(old.at,old.ordinal,old.id)))",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    if historical {
        db.execute("UPDATE messages SET incoming=NULL", [])
            .map_err(|e| e.to_string())?;
        set(db, "epoch", &uuid::Uuid::new_v4().to_string())?;
        set(db, "order_reset", "historical_backfill")?;
    }
    let mut next: i64 = db
        .query_row(
            "SELECT COALESCE(max(incoming),0) FROM messages",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut query=db.prepare("SELECT id FROM messages WHERE role='incoming' AND incoming IS NULL ORDER BY at,ordinal,id LIMIT 1000").map_err(|e|e.to_string())?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let more = ids.len() == 1000;
    for id in ids {
        next += 1;
        db.execute(
            "UPDATE messages SET incoming=? WHERE id=?",
            params![next, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(if more { None } else { Some(next) })
}
fn merge_row(db: &Connection, first: &str, second: &str, body: &str, kind: &str) -> Result<()> {
    let a: Option<i64> = db
        .query_row("SELECT incoming FROM messages WHERE id=?", [first], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let b: Option<i64> = db
        .query_row("SELECT incoming FROM messages WHERE id=?", [second], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let (keep, remove) = if b.is_some() && a.is_none() {
        (second, first)
    } else {
        (first, second)
    };
    if a.is_some() && b.is_some() && a != b {
        db.execute("UPDATE messages SET incoming=NULL", [])
            .map_err(|e| e.to_string())?;
        set(db, "epoch", &uuid::Uuid::new_v4().to_string())?;
        set(db, "order_reset", "canonical_merge")?;
    }
    db.execute(
        "UPDATE messages SET body=?,kind=? WHERE id=?",
        params![body, kind, keep],
    )
    .map_err(|e| e.to_string())?;
    db.execute("UPDATE aliases SET id=? WHERE id=?", params![keep, remove])
        .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO aliases(original,id) VALUES(?,?)",
        params![format!("history:{remove}"), keep],
    )
    .map_err(|e| e.to_string())?;
    db.execute("DELETE FROM messages WHERE id=?", [remove])
        .map_err(|e| e.to_string())?;
    db.execute("DELETE FROM reconcile_work WHERE id=?", [remove])
        .map_err(|e| e.to_string())?;
    Ok(())
}
/// Hook detail() uses journal::clipped(...,journal::REPLY_LIMIT), and 1200 in
/// older journals: Unicode scalars, plus exactly one synthetic U+2026 when more
/// text exists. Other ellipses are literal.
fn stop_excerpt_matches(provider: &str, hook: &str) -> bool {
    let trimmed = hook.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '…') {
        return false;
    }
    let excerpt = hook
        .strip_suffix('…')
        .filter(|prefix| [1200, journal::REPLY_LIMIT].contains(&prefix.chars().take(journal::REPLY_LIMIT + 1).count()))
        .unwrap_or(hook);
    !excerpt.trim().is_empty() && provider.contains(excerpt)
}

/// Retry old cached pairs without rebuilding sources or changing stable IDs.
/// Primary-key scans survive merges/deletes; a version is committed only once
/// both the scan and the ordinary bounded work queue have finished.
fn advance_reconcile_upgrade(db: &Connection) -> Result<bool> {
    if get(db, "reconcile_version")?.as_deref() == Some(RECONCILE_VERSION) {
        return Ok(true);
    }
    let offset = get(db, "reconcile_upgrade_after")?.unwrap_or_default();
    let mut query = db
        .prepare("SELECT id,role,stream FROM messages WHERE id>? ORDER BY id LIMIT 256")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([offset], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, role, stream) in &rows {
        if role == "incoming" && stream == "provider" {
            db.execute("INSERT OR IGNORE INTO reconcile_work(id) VALUES(?)", [id])
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some((id, _, _)) = rows.last() {
        set(db, "reconcile_upgrade_after", id)?;
    }
    Ok(rows.len() < 256)
}
fn finish_reconcile_upgrade(db: &Connection, scan_done: bool) -> Result<bool> {
    if !scan_done {
        return Ok(false);
    }
    let pending: bool = db
        .query_row("SELECT EXISTS(SELECT 1 FROM reconcile_work)", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if !pending {
        set(db, "reconcile_version", RECONCILE_VERSION)?;
        db.execute("DELETE FROM meta WHERE key='reconcile_upgrade_after'", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(!pending)
}
fn reconcile(db: &Connection) -> Result<()> {
    // A work queue handles either arrival order, body revisions and deleted-ID
    // reuse. Candidate uniqueness is checked across the full canonical index.
    let mut query=db.prepare("SELECT w.id,m.at,m.role,m.stream,m.body FROM reconcile_work w LEFT JOIN messages m ON m.id=w.id ORDER BY w.id LIMIT 64").map_err(|e|e.to_string())?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, at, role, stream, body) in &rows {
        if let (Some(at), Some(role), Some(stream), Some(body)) = (at, role, stream, body) {
            if ["provider", "attachment"].contains(&stream.as_str()) {
                let value: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
                let radius = if stream == "attachment" { 10. } else { 3. };
                let count:i64=db.query_row("SELECT count(*) FROM (SELECT 1 FROM messages WHERE role=? AND at BETWEEN ? AND ? LIMIT 33)",params![role,at-radius,at+radius],|r|r.get(0)).map_err(|e|e.to_string())?;
                if count <= 32 {
                    let mut near=db.prepare("SELECT id,stream,kind,body FROM messages WHERE role=? AND at BETWEEN ? AND ?").map_err(|e|e.to_string())?;
                    let candidates = near
                        .query_map(params![role, at - radius, at + radius], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, String>(3)?,
                            ))
                        })
                        .map_err(|e| e.to_string())?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?;
                    let matches = |text: &str| {
                        if stream == "attachment" {
                            hash(text) == string(&value, "native_match_hash")
                        } else if role == "incoming" {
                            stop_excerpt_matches(string(&value, "detail"), text)
                        } else {
                            text == string(&value, "detail")
                        }
                    };
                    let hooks: Vec<_> = candidates
                        .iter()
                        .filter(|(other, source, kind, text)| {
                            other != id
                                && if stream == "attachment" {
                                    source != "attachment"
                                        && [
                                            "UserPromptSubmit",
                                            "UserPromptQueued",
                                            "UserMessage",
                                            "TurnStarted",
                                        ]
                                        .contains(&kind.as_str())
                                } else {
                                    source == "journal"
                                        && if role == "incoming" {
                                            kind == "Stop"
                                        } else {
                                            [
                                                "UserPromptSubmit",
                                                "UserPromptQueued",
                                                "UserMessage",
                                                "TurnStarted",
                                            ]
                                            .contains(&kind.as_str())
                                        }
                                }
                                && serde_json::from_str::<Value>(text)
                                    .ok()
                                    .is_some_and(|row| matches(string(&row, "detail")))
                        })
                        .collect();
                    if let [hook] = hooks.as_slice() {
                        let hook_value: Value =
                            serde_json::from_str(&hook.3).map_err(|e| e.to_string())?;
                        let competing = candidates
                            .iter()
                            .filter(|(_, source, _, text)| {
                                source == stream
                                    && serde_json::from_str::<Value>(text).ok().is_some_and(|row| {
                                        if stream == "attachment" {
                                            string(&row, "native_match_hash")
                                                == string(&value, "native_match_hash")
                                        } else if role == "incoming" {
                                            stop_excerpt_matches(
                                                string(&row, "detail"),
                                                string(&hook_value, "detail"),
                                            )
                                        } else {
                                            string(&row, "detail") == string(&hook_value, "detail")
                                        }
                                    })
                            })
                            .count();
                        if competing == 1 {
                            merge_row(db, &hook.0, id, body, string(&value, "type"))?;
                        }
                    }
                }
            }
        }
        db.execute("DELETE FROM reconcile_work WHERE id=?", [id])
            .map_err(|e| e.to_string())?;
    }
    let pending: bool = db
        .query_row("SELECT EXISTS(SELECT 1 FROM reconcile_work)", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    set(db, "reconciled", if pending { "0" } else { "1" })?;
    Ok(())
}
fn ascii_wire_size(value: &Value) -> usize {
    value
        .to_string()
        .chars()
        .map(|c| if c.is_ascii() { 1 } else { 6 * c.len_utf16() })
        .sum()
}
fn page(
    db: &Connection,
    r: &Request,
    complete: bool,
    total: Option<i64>,
    status: Value,
) -> Result<Value> {
    let epoch = get(db, "epoch")?.ok_or("History epoch unavailable")?;
    let secret = get(db, "secret")?.ok_or("History cursor key unavailable")?;
    let anchor = r
        .before
        .as_ref()
        .or(r.after.as_ref())
        .or(r.around.as_ref())
        .map(|v| decode(v, &secret, &epoch))
        .transpose()?;
    let mut around = if r.around.is_some() {
        anchor.clone()
    } else {
        None
    };
    if let Some(seq) = r.around_incoming_seq {
        if !complete || r.history_epoch.as_deref() != Some(&epoch) {
            return Err("Exact complete history epoch required for incoming anchor".into());
        }
        around = db
            .query_row(
                "SELECT at,ordinal,id FROM messages WHERE incoming>? ORDER BY incoming LIMIT 1",
                [seq],
                |row| {
                    Ok((
                        row.get::<_, f64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
    }
    let limit = r.limit.unwrap_or(100) as i64;
    let (sql, position) = if let Some(anchor) = around {
        ("SELECT id,at,ordinal,body,incoming FROM messages WHERE (at,ordinal,id)>=(?,?,?) ORDER BY at,ordinal,id LIMIT ?",anchor)
    } else if r.after.is_some() {
        ("SELECT id,at,ordinal,body,incoming FROM messages WHERE (at,ordinal,id)>(?,?,?) ORDER BY at,ordinal,id LIMIT ?",anchor.unwrap())
    } else {
        ("SELECT id,at,ordinal,body,incoming FROM messages WHERE (at,ordinal,id)<(?,?,?) ORDER BY at DESC,ordinal DESC,id DESC LIMIT ?",anchor.unwrap_or((f64::MAX,"~".into(),"~".into())))
    };
    let mut query = db.prepare(sql).map_err(|e| e.to_string())?;
    let mut rows = query
        .query_map(params![position.0, position.1, position.2, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    rows.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    let mut events = Vec::new();
    let mut kept = Vec::new();
    let mut bytes = 0;
    let mut detail_truncated = false;
    // Consume in query direction so a byte-limited older page remains adjacent
    // to its anchor; reverse only the rows actually returned.
    if sql.contains(" DESC") {
        rows.reverse();
    }
    for (id, at, ordinal, body, incoming) in &rows {
        let mut value: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
        let mut aliases = db
            .prepare("SELECT original FROM aliases WHERE id=? ORDER BY original LIMIT 8")
            .map_err(|e| e.to_string())?;
        let originals = aliases
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        value["original_ids"] = json!(originals);
        value["history_id"] = json!(id);
        value["history_cursor"] = json!(cursor(&secret, &epoch, *at, ordinal, id));
        value["history_stream"] = json!(if string(&value, "source") == "hgs_delivery" {
            "attachment"
        } else if string(&value, "source").is_empty() {
            "journal"
        } else {
            "provider"
        });
        value.as_object_mut().unwrap().remove("native_match_hash");
        value["incoming_seq"] = if complete {
            json!(incoming)
        } else {
            Value::Null
        };
        let mut cost = ascii_wire_size(&value) + 1;
        if bytes + cost > 880 * 1024 {
            if !events.is_empty() {
                break;
            }
            value["detail"] = json!(journal::clipped(&value["detail"], 500));
            value["detail_truncated"] = json!(true);
            detail_truncated = true;
            cost = ascii_wire_size(&value) + 1;
            if cost > 880 * 1024 {
                return Err("Public message metadata exceeds the bounded history page".into());
            }
        }
        bytes += cost;
        detail_truncated |= value["detail_truncated"] == true;
        events.push(value);
        kept.push((id.clone(), *at, ordinal.clone(), body.clone(), *incoming));
    }
    rows = kept;
    if sql.contains(" DESC") {
        rows.reverse();
        events.reverse();
    }
    let has_before = if let Some((id, at, ordinal, _, _)) = rows.first() {
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE (at,ordinal,id)<(?,?,?))",
            params![at, ordinal, id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())?
    } else {
        false
    };
    let has_after = if let Some((id, at, ordinal, _, _)) = rows.last() {
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE (at,ordinal,id)>(?,?,?))",
            params![at, ordinal, id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())?
    } else {
        false
    };
    Ok(
        json!({"history_epoch":epoch,"events":events,"next_before":if has_before{rows.first().map(|(id,at,ordinal,_,_)|cursor(&secret,&epoch,*at,ordinal,id))}else{None},"next_after":if has_after{rows.last().map(|(id,at,ordinal,_,_)|cursor(&secret,&epoch,*at,ordinal,id))}else{None},"has_more_before":has_before,"has_more_after":has_after,
  "head":{"incoming_seq":total,"total_incoming":total,"complete":complete},"indexing":!complete&&get(db,"gap")?.is_none()&&(status["provider"]=="indexing"||status["provider"]=="source_reset"||status["journal"]=="indexing"||status["attachments"]=="indexing"||status["canonical"]=="indexing"),"truncated":detail_truncated||!complete,"source_status":status}),
    )
}
pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    if args.len() != 2 || args[1] != "--json" {
        return Err("usage: hgs history <session> --json".into());
    }
    let mut bytes = Vec::new();
    io::stdin()
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("History request exceeds bounds".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate(&request)?;
    let name = &args[0];
    let parent = record(name, &request)?;
    let native = native_record(&parent, &request);
    let (db, _guard) = index(&parent, &request)?;
    db.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let (provider_complete, mut status) = if string(&parent, "agent") == "dsh" {
        let detail = dsh::inspect(name, request.agent_id.as_deref())?;
        if detail["run_id"] != parent["run_id"]
            || detail
                .get("parent_conversation_id")
                .unwrap_or(&detail["conversation_id"])
                != &parent["conversation_id"]
        {
            return Err("DeepSeek history identity changed".into());
        }
        for row in detail["events"].as_array().into_iter().flatten() {
            add(&db, &key(&parent, &request), row.clone(), "journal")?;
        }
        let done = detail["history_truncated"] == false;
        (
            done,
            json!({"provider":if done{"complete"}else{"partial_resident_adapter"}}),
        )
    } else {
        sources(&db, &parent, &request, &native)?
    };
    let journal_complete = if string(&parent, "agent") == "dsh" {
        true
    } else {
        journal(&db, &parent, &request)?
    };
    let receipts_complete = receipts(&db, &parent, &request)?;
    let source_complete = provider_complete && journal_complete && receipts_complete;
    let upgrade_scan_done = advance_reconcile_upgrade(&db)?;
    if source_complete {
        reconcile(&db)?;
    }
    let upgrade_done = finish_reconcile_upgrade(&db, upgrade_scan_done)?;
    if let Some(reset) = get(&db, "order_reset")? {
        status["canonical_order_reset"] = json!(reset);
    }
    let gap = get(&db, "gap")?;
    if let Some(gap) = &gap {
        status["gap"] = json!(gap);
    }
    status["journal"] = json!(if journal_complete {
        "complete"
    } else {
        "indexing"
    });
    status["attachments"] = json!(if receipts_complete {
        "complete"
    } else {
        "indexing"
    });
    let mut source_bytes = 0u64;
    let mut known_bytes = 0u64;
    let mut progress = db
        .prepare("SELECT key,value FROM meta WHERE key LIKE 'offset:%' OR key LIKE 'length:%'")
        .map_err(|e| e.to_string())?;
    for entry in progress
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
    {
        let (name, value) = entry.map_err(|e| e.to_string())?;
        let value = value.parse::<u64>().unwrap_or(0);
        if name.starts_with("offset:") {
            source_bytes += value;
        } else {
            known_bytes += value;
        }
    }
    status["progress"] = json!({"source_bytes":source_bytes,"known_source_bytes":known_bytes,"journal_seq":get(&db,"journal_seq")?,"receipt_files":db.query_row("SELECT count(*) FROM receipt_seen",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?,"canonical_pending":db.query_row("SELECT count(*) FROM reconcile_work",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?,"assigned_incoming":db.query_row("SELECT count(*) FROM messages WHERE incoming IS NOT NULL",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?,"reconcile_upgrade_after":get(&db,"reconcile_upgrade_after")?,"reconcile_version":get(&db,"reconcile_version")?});
    let mut complete = source_complete
        && upgrade_done
        && get(&db, "reconciled")?.as_deref() == Some("1")
        && gap.is_none();
    status["canonical"] = json!(if !upgrade_scan_done
        || (source_complete && (!upgrade_done || get(&db, "reconciled")?.as_deref() != Some("1")))
    {
        "indexing"
    } else {
        "complete"
    });
    let total = incoming(&db, complete)?;
    if complete && total.is_none() {
        complete = false;
        status["canonical"] = json!("indexing");
    }
    db.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    let mut result = page(&db, &request, complete, total, status)?;
    // A fresh read result cannot quietly switch to a replacement run during I/O.
    let _current = record(name, &request)?;
    result["request_id"] = json!(request.request_id);
    result["name"] = json!(name);
    result["run_id"] = json!(request.expected_run_id);
    result["conversation_id"] = json!(if let Some(id) = &request.agent_id {
        format!("{}/{}", request.expected_conversation_id, id)
    } else {
        request.expected_conversation_id.clone()
    });
    if let Some(id) = &request.agent_id {
        result["agent_id"] = json!(id);
        result["parent_conversation_id"] = json!(request.expected_conversation_id);
    }
    if let Some(id) = &request.archive_id {
        result["archive_id"] = json!(id);
    }
    if ascii_wire_size(&result) > 1024 * 1024 - 4096 {
        return Err("History response exceeds its transport envelope".into());
    }
    println!("{result}");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE messages(id TEXT PRIMARY KEY,at REAL NOT NULL,role TEXT NOT NULL,stream TEXT NOT NULL,kind TEXT NOT NULL,ordinal TEXT NOT NULL,body TEXT NOT NULL,incoming INTEGER); CREATE TABLE aliases(original TEXT PRIMARY KEY,id TEXT NOT NULL); CREATE TABLE reconcile_work(id TEXT PRIMARY KEY); CREATE TABLE receipt_seen(name TEXT PRIMARY KEY,stamp TEXT NOT NULL);").unwrap();
        set(&db, "epoch", "test-epoch").unwrap();
        set(&db, "secret", &"a".repeat(64)).unwrap();
        db
    }
    fn request() -> Request {
        serde_json::from_value(json!({"request_id":uuid::Uuid::new_v4().to_string(),"expected_run_id":"run","expected_conversation_id":"conversation"})).unwrap()
    }
    fn row(kind: &str, id: &str, text: &str, at: f64) -> Value {
        json!({"type":kind,"message_id":id,"detail":text,"at":at,"source":if kind=="Stop"{""}else{"codex_transcript"},"source_offset":"0:12"})
    }
    #[test]
    fn revisions_and_equal_time_provenance_are_stable() {
        let db = database();
        add(
            &db,
            "scope",
            row("AgentMessage", "one", "Partial", 1.),
            "provider",
        )
        .unwrap();
        let id: String = db
            .query_row("SELECT id FROM messages", [], |r| r.get(0))
            .unwrap();
        add(
            &db,
            "scope",
            row("AgentMessage", "one", "Complete final", 1.),
            "provider",
        )
        .unwrap();
        let value = page(&db, &request(), false, None, json!({})).unwrap();
        assert_eq!(value["events"].as_array().unwrap().len(), 1);
        assert_eq!(value["events"][0]["history_id"], id);
        assert_eq!(value["events"][0]["detail"], "Complete final");
    }
    #[test]
    fn either_arrival_order_reconciles_and_future_work_is_not_skipped() {
        for provider_first in [false, true] {
            let db = database();
            let hook = row("Stop", "hook", "answer", 10.);
            let full = row("AgentMessage", "provider", "Full answer", 10.);
            if provider_first {
                add(&db, "scope", full.clone(), "provider").unwrap();
                reconcile(&db).unwrap();
            }
            add(&db, "scope", hook, "journal").unwrap();
            if !provider_first {
                add(&db, "scope", full, "provider").unwrap();
            }
            reconcile(&db).unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            add(
                &db,
                "scope",
                row("Stop", "next-hook", "next", 20.),
                "journal",
            )
            .unwrap();
            add(
                &db,
                "scope",
                row("AgentMessage", "next-provider", "Full next", 20.),
                "provider",
            )
            .unwrap();
            reconcile(&db).unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                2
            );
        }
    }
    #[test]
    fn ambiguous_repeated_messages_remain_distinct() {
        let db = database();
        for id in ["hook-one", "hook-two"] {
            add(&db, "scope", row("Stop", id, "answer", 10.), "journal").unwrap();
        }
        add(
            &db,
            "scope",
            row("AgentMessage", "provider", "Full answer", 10.),
            "provider",
        )
        .unwrap();
        reconcile(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn historical_backfill_resets_epoch_instead_of_inflating_old_watermark() {
        let db = database();
        add(
            &db,
            "scope",
            row("AgentMessage", "later", "Later", 20.),
            "provider",
        )
        .unwrap();
        assert_eq!(incoming(&db, true).unwrap(), Some(1));
        add(
            &db,
            "scope",
            row("AgentMessage", "older", "Older", 10.),
            "provider",
        )
        .unwrap();
        assert_eq!(incoming(&db, true).unwrap(), Some(2));
        assert_ne!(get(&db, "epoch").unwrap().unwrap(), "test-epoch");
    }
    #[test]
    fn unicode_pages_fit_and_cursors_do_not_skip_rows() {
        let db = database();
        for n in 0..100 {
            add(
                &db,
                "scope",
                row(
                    "AgentMessage",
                    &format!("id-{n}"),
                    &"😀".repeat(8000),
                    n as f64,
                ),
                "provider",
            )
            .unwrap();
        }
        let mut r = request();
        let first = page(&db, &r, false, None, json!({})).unwrap();
        assert!(ascii_wire_size(&first) < 1024 * 1024);
        let first_rows = first["events"].as_array().unwrap();
        assert!(first_rows.len() < 100);
        r.before = first["next_before"].as_str().map(str::to_owned);
        let previous = page(&db, &r, false, None, json!({})).unwrap();
        let last = previous["events"].as_array().unwrap().last().unwrap();
        assert_eq!(
            last["at"].as_f64().unwrap() + 1.,
            first_rows[0]["at"].as_f64().unwrap()
        );
    }
    #[test]
    fn hook_clipping_recognizes_unicode_scalar_limit_only() {
        for prefix in ["x".repeat(1200), "🧭Ж🙂α".repeat(300), "Ж".repeat(journal::REPLY_LIMIT)] {
            let full = format!("{prefix} continued public reply");
            let hook = format!("{prefix}…");
            assert!(stop_excerpt_matches(&full, &hook));
            assert!(!stop_excerpt_matches(
                &format!("different{}", &prefix.chars().skip(1).collect::<String>()),
                &hook
            ));
        }
        assert!(!stop_excerpt_matches(
            "Short ending continues",
            "Short ending…"
        ));
        assert!(stop_excerpt_matches(
            "Short ending… literally",
            "Short ending…"
        ));
        assert!(stop_excerpt_matches("Literal ... retained", "..."));
        for blank in ["", " ", "\n", "…", "……"] {
            assert!(!stop_excerpt_matches("public text … ...", blank));
        }
    }
    #[test]
    fn clipped_hooks_merge_in_either_arrival_order_without_read_inflation() {
        for text in ["A".repeat(1200), "🧭Ж🙂α".repeat(300)] {
            for provider_first in [false, true] {
                let db = database();
                let full = format!("{text} final continuation");
                let hook = format!("{text}…");
                if provider_first {
                    add(
                        &db,
                        "scope",
                        row("AgentMessage", "provider", &full, 10.),
                        "provider",
                    )
                    .unwrap();
                    reconcile(&db).unwrap();
                }
                add(&db, "scope", row("Stop", "hook", &hook, 11.), "journal").unwrap();
                if !provider_first {
                    add(
                        &db,
                        "scope",
                        row("AgentMessage", "provider", &full, 10.),
                        "provider",
                    )
                    .unwrap();
                }
                reconcile(&db).unwrap();
                assert_eq!(incoming(&db, true).unwrap(), Some(1));
                let page = page(&db, &request(), true, Some(1), json!({})).unwrap();
                assert_eq!(page["events"].as_array().unwrap().len(), 1);
                assert_eq!(page["events"][0]["detail"], full);
            }
        }
    }
    #[test]
    fn clipped_pairs_respect_radius_and_mutual_uniqueness() {
        let prefix = "Ж".repeat(1200);
        let hook = format!("{prefix}…");
        let full = format!("{prefix} continuation");
        for case in ["wrong_text", "outside_radius", "two_hooks", "two_providers"] {
            let db = database();
            add(&db, "scope", row("Stop", "hook", &hook, 10.), "journal").unwrap();
            add(
                &db,
                "scope",
                row(
                    "AgentMessage",
                    "provider",
                    if case == "wrong_text" {
                        "unrelated"
                    } else {
                        &full
                    },
                    if case == "outside_radius" { 14. } else { 10. },
                ),
                "provider",
            )
            .unwrap();
            if case == "two_hooks" {
                add(
                    &db,
                    "scope",
                    row("Stop", "other_hook", &hook, 10.),
                    "journal",
                )
                .unwrap();
            }
            if case == "two_providers" {
                add(
                    &db,
                    "scope",
                    row("AgentMessage", "other_provider", &full, 10.),
                    "provider",
                )
                .unwrap();
            }
            reconcile(&db).unwrap();
            let expected = if case.starts_with("two_") { 3 } else { 2 };
            assert_eq!(
                db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                expected,
                "{case}"
            );
        }
    }
    #[test]
    fn cached_numbered_duplicates_upgrade_with_aliases_and_explicit_epoch_reset() {
        let db = database();
        let prefix = "🙂".repeat(1200);
        let full = format!("{prefix} full reply");
        add(
            &db,
            "scope",
            row("Stop", "hook", &format!("{prefix}…"), 10.),
            "journal",
        )
        .unwrap();
        add(
            &db,
            "scope",
            row("AgentMessage", "provider", &full, 11.),
            "provider",
        )
        .unwrap();
        // Model an index finalized by the old literal-ellipsis matcher.
        db.execute("DELETE FROM reconcile_work", []).unwrap();
        set(&db, "reconciled", "1").unwrap();
        assert_eq!(incoming(&db, true).unwrap(), Some(2));
        let before = page(&db, &request(), true, Some(2), json!({})).unwrap();
        let old_cursor = before["events"][0]["history_cursor"]
            .as_str()
            .unwrap()
            .to_owned();
        let old_epoch = get(&db, "epoch").unwrap().unwrap();
        let scan = advance_reconcile_upgrade(&db).unwrap();
        reconcile(&db).unwrap();
        assert!(finish_reconcile_upgrade(&db, scan).unwrap());
        assert_eq!(incoming(&db, true).unwrap(), Some(1));
        let epoch = get(&db, "epoch").unwrap().unwrap();
        assert_ne!(epoch, old_epoch);
        assert!(decode(&old_cursor, &get(&db, "secret").unwrap().unwrap(), &epoch).is_err());
        let after = page(&db, &request(), true, Some(1), json!({})).unwrap();
        let row = &after["events"][0];
        assert_eq!(after["events"].as_array().unwrap().len(), 1);
        assert_eq!(row["detail"], full);
        assert!(row["original_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().starts_with("history:")));
        assert_eq!(
            get(&db, "reconcile_version").unwrap().as_deref(),
            Some(RECONCILE_VERSION)
        );
    }
    #[test]
    fn upgrade_scan_is_bounded_resumable_and_does_not_reset_clean_epoch() {
        let db = database();
        for n in 0..300 {
            add(
                &db,
                "scope",
                row(
                    "AgentMessage",
                    &format!("provider-{n}"),
                    "Unique",
                    n as f64 * 10.,
                ),
                "provider",
            )
            .unwrap();
        }
        db.execute("DELETE FROM reconcile_work", []).unwrap();
        assert_eq!(incoming(&db, true).unwrap(), Some(300));
        let old_epoch = get(&db, "epoch").unwrap();
        let scan = advance_reconcile_upgrade(&db).unwrap();
        assert!(!scan);
        reconcile(&db).unwrap();
        assert!(!finish_reconcile_upgrade(&db, scan).unwrap());
        let cursor = get(&db, "reconcile_upgrade_after").unwrap().unwrap();
        assert!(get(&db, "reconcile_version").unwrap().is_none());
        for _ in 0..8 {
            let scan = advance_reconcile_upgrade(&db).unwrap();
            reconcile(&db).unwrap();
            if finish_reconcile_upgrade(&db, scan).unwrap() {
                break;
            }
        }
        assert_eq!(
            get(&db, "reconcile_version").unwrap().as_deref(),
            Some(RECONCILE_VERSION)
        );
        assert_eq!(get(&db, "epoch").unwrap(), old_epoch);
        assert!(!cursor.is_empty());
        assert!(get(&db, "reconcile_upgrade_after").unwrap().is_none());
        assert_eq!(incoming(&db, true).unwrap(), Some(300));
    }
    #[test]
    fn partial_and_oversized_sources_make_bounded_progress() {
        let temp = tempfile::tempdir().unwrap();
        let conversation = uuid::Uuid::new_v4().to_string();
        let path = temp.path().join(format!("rollout-{conversation}.jsonl"));
        let header =
            json!({"type":"session_meta","payload":{"id":conversation}}).to_string() + "\n";
        let public=json!({"type":"event_msg","timestamp":"2026-10-09T00:00:00Z","payload":{"type":"agent_message","message":"Final without newline"}}).to_string();
        std::fs::write(&path, header.clone() + &public).unwrap();
        let native = json!({"agent":"codex","conversation_id":conversation,"transcript":path});
        let db = database();
        let r = request();
        let (_, status) = sources(&db, &native, &r, &native).unwrap();
        assert_eq!(status["provider"], "partial_final_line");
        assert_eq!(
            db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        std::fs::write(
            &path,
            header.clone() + &"x".repeat(BUDGET * 2 + 123) + "\n" + &public + "\n",
        )
        .unwrap();
        for _ in 0..8 {
            let _ = sources(&db, &native, &r, &native).unwrap();
        }
        assert_eq!(
            get(&db, "offset:0")
                .unwrap()
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            std::fs::metadata(&path).unwrap().len()
        );
    }
}
