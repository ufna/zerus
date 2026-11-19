//! Authoritative native archives. Older CLIs depend on the terminal-only
//! archive directory and its catalog, including for live terminal actions.
use super::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::time::Duration;

const FILE: &str = "dsh/archives-v1.sqlite3";
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS archive_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS archive_rows (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, run TEXT NOT NULL, conversation TEXT NOT NULL,
        at REAL NOT NULL, search TEXT NOT NULL, body TEXT NOT NULL, UNIQUE(name, run, conversation)
    );
    CREATE INDEX IF NOT EXISTS native_archive_by_date ON archive_rows(at DESC, id DESC);
";

fn sql<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|error| format!("DeepSeek archive: {error}"))
}

fn existing_path() -> Result<Option<PathBuf>> {
    let path = root().join(FILE);
    match fs::symlink_metadata(&path) {
        Ok(info) if info.is_file() && !info.file_type().is_symlink() => Ok(Some(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
        Ok(_) => Err("DeepSeek archive is not a regular file".into()),
    }
}

fn open(path: &Path, writable: bool) -> Result<Connection> {
    let mode = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let db = sql(Connection::open_with_flags(
        path,
        mode | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    ))?;
    sql(db.busy_timeout(Duration::from_secs(2)))?;
    Ok(db)
}

// The state writer serializes publication. Do not publish an uninitialized
// database even if startup is interrupted after the SQLite file is created.
fn writer() -> Result<Connection> {
    if let Some(path) = existing_path()? {
        return open(&path, true);
    }
    let path = root().join(FILE);
    let parent = path.parent().unwrap();
    private_dir(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    {
        let db = open(temp.path(), true)?;
        let tx = sql(db.unchecked_transaction())?;
        sql(tx.execute_batch(SCHEMA))?;
        sql(tx.execute(
            "INSERT INTO archive_meta VALUES ('id', ?1)",
            [uuid::Uuid::new_v4().to_string()],
        ))?;
        sql(tx.commit())?;
    }
    temp.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temp.persist_noclobber(&path)
        .map_err(|error| error.to_string())?;
    crate::platform::sync_dir(parent).map_err(|error| error.to_string())?;
    open(&path, true)
}

pub(in crate::state) fn revision() -> Result<Option<String>> {
    existing_path()?
        .map(|path| archive_catalog::stamp(&path))
        .transpose()
}

/// Add the native metadata to the page's pinned read connection. All disk
/// tables keep their original names and schemas for older executables.
pub(in crate::state) fn attach(db: &Connection) -> Result<Option<String>> {
    let Some(path) = existing_path()? else {
        return Ok(None);
    };
    sql(db.execute(
        "ATTACH DATABASE ?1 AS dsh_archive",
        [path.to_str().ok_or("DeepSeek archive path is not UTF-8")?],
    ))?;
    let id = sql(db.query_row(
        "SELECT value FROM dsh_archive.archive_meta WHERE key='id'",
        [],
        |row| row.get(0),
    ))?;
    sql(db.execute_batch(
        "CREATE TEMP VIEW archive_rows AS
        SELECT id, name, at, search FROM main.archive_rows
        UNION ALL SELECT id, name, at, search FROM dsh_archive.archive_rows",
    ))?;
    Ok(Some(id))
}

pub(super) fn contains(id: &str) -> Result<bool> {
    if !session_id::canonical(id) {
        return Err("invalid archive ID".into());
    }
    let Some(path) = existing_path()? else {
        return Ok(false);
    };
    let db = open(&path, false)?;
    sql(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM archive_rows WHERE id=?1)",
        [id],
        |row| row.get(0),
    ))
}

pub(in crate::state) fn by_id(id: &str) -> Result<Option<Value>> {
    if !session_id::canonical(id) {
        return Err("invalid archive ID".into());
    }
    let Some(path) = existing_path()? else {
        return Ok(None);
    };
    let db = open(&path, false)?;
    let raw: Option<Vec<u8>> = sql(db
        .query_row(
            "SELECT substr(CAST(body AS BLOB), 1, ?2) FROM archive_rows WHERE id=?1",
            params![id, storage::ID_RECORD_BYTES + 1],
            |row| row.get(0),
        )
        .optional())?;
    raw.map(|raw| {
        if raw.len() as u64 > storage::ID_RECORD_BYTES {
            return Err("DeepSeek archive is too large".into());
        }
        let mut record: Value = serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
        rename::normalize_archive(&mut record)?;
        if record["version"] != 1
            || record["archive_id"] != id
            || record["agent"] != "dsh"
            || record["backend"] != "dsh"
            || !string(&record, "name").starts_with("dsh/")
        {
            return Err("invalid DeepSeek archive record".into());
        }
        Ok(record)
    })
    .transpose()
}

pub(super) fn publish(record: &Value) -> Result<String> {
    let db = writer()?;
    let name = string(record, "name");
    let run = string(record, "run_id");
    let conversation = string(record, "conversation_id");
    let existing: Option<String> = sql(db
        .query_row(
            "SELECT id FROM archive_rows WHERE name=?1 AND run=?2 AND conversation=?3",
            params![name, run, conversation],
            |row| row.get(0),
        )
        .optional())?;
    if let Some(id) = existing {
        let prior = by_id(&id)?.ok_or("DeepSeek archive changed before publication")?;
        if !archive::equivalent(record, &[prior]) {
            return Err("DeepSeek archive identity changed".into());
        }
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut row = record.clone();
    row["archive_id"] = json!(id);
    row["archived_at"] = json!(now());
    row["state"] = json!("archived");
    for key in ["layout", "pausing", "terminal_name"] {
        row.as_object_mut().unwrap().remove(key);
    }
    let raw = row.to_string();
    if raw.len() as u64 > storage::ID_RECORD_BYTES {
        return Err("DeepSeek archive is too large".into());
    }
    sql(db.execute(
        "INSERT INTO archive_rows VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            name,
            run,
            conversation,
            row["archived_at"].as_f64(),
            archive_catalog::search_text(&row),
            raw
        ],
    ))?;
    Ok(id)
}

pub(in crate::state) fn update_name(record: &Value) -> Result<()> {
    let db = writer()?;
    let raw = record.to_string();
    if raw.len() as u64 > storage::ID_RECORD_BYTES {
        return Err("DeepSeek archive is too large".into());
    }
    let changed = sql(db.execute("UPDATE archive_rows SET name=?2, search=?3, body=?4 WHERE id=?1 AND run=?5 AND conversation=?6",
        params![string(record, "archive_id"), string(record, "name"), archive_catalog::search_text(record), raw,
            string(record, "run_id"), string(record, "conversation_id")]))?;
    if changed != 1 {
        return Err("DeepSeek archive changed before rename".into());
    }
    Ok(())
}

pub(in crate::state) fn remove(record: &Value) -> Result<()> {
    let db = writer()?;
    let changed = sql(db.execute(
        "DELETE FROM archive_rows WHERE id=?1 AND name=?2 AND run=?3 AND conversation=?4",
        params![
            string(record, "archive_id"),
            string(record, "name"),
            string(record, "run_id"),
            string(record, "conversation_id")
        ],
    ))?;
    if changed != 1 {
        return Err("DeepSeek archive changed before removal".into());
    }
    Ok(())
}
