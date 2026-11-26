//! Versioned native-provider archive stores, separate from the terminal catalog.
use super::*;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::{
    fs,
    time::{Duration, Instant},
};

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS archive_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS archive_rows (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, run TEXT NOT NULL, conversation TEXT NOT NULL,
        at REAL NOT NULL, search TEXT NOT NULL, body TEXT NOT NULL, UNIQUE(name, run, conversation)
    );
    CREATE INDEX IF NOT EXISTS native_archive_by_date ON archive_rows(at DESC, id DESC);
";

fn sql<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|error| format!("Native archive: {error}"))
}

pub(super) struct Store {
    file: &'static str,
    valid: fn(&Value) -> bool,
    provider: &'static str,
}

impl Store {
    pub(super) const fn new(
        file: &'static str,
        provider: &'static str,
        valid: fn(&Value) -> bool,
    ) -> Self {
        Self {
            file,
            provider,
            valid,
        }
    }

    fn existing_path(&self) -> Result<Option<PathBuf>> {
        let path = root().join(self.file);
        match fs::symlink_metadata(&path) {
            Ok(info) if info.is_file() && !info.file_type().is_symlink() => Ok(Some(path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
            Ok(_) => Err("Native archive is not a regular file".into()),
        }
    }

    // The state writer serializes publication. Do not publish an uninitialized
    // database even if startup is interrupted after the SQLite file is created.
    fn writer(&self) -> Result<Connection> {
        if let Some(path) = self.existing_path()? {
            return open(&path, true);
        }
        let path = root().join(self.file);
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

    pub(super) fn revision(&self) -> Result<Option<String>> {
        self.existing_path()?
            .map(|path| archive_catalog::stamp(&path))
            .transpose()
    }

    /// Attach only a fixed internal alias, keeping the store's on-disk schema.
    pub(super) fn attach(&self, db: &Connection, alias: &'static str) -> Result<Option<String>> {
        let Some(path) = self.existing_path()? else {
            return Ok(None);
        };
        sql(db.execute(
            &format!("ATTACH DATABASE ?1 AS {alias}"),
            [path.to_str().ok_or("Native archive path is not UTF-8")?],
        ))?;
        sql(db.query_row(
            &format!("SELECT value FROM {alias}.archive_meta WHERE key='id'"),
            [],
            |row| row.get(0),
        ))
        .map(Some)
    }

    pub(super) fn contains(&self, id: &str) -> Result<bool> {
        if !session_id::canonical(id) {
            return Err("invalid archive ID".into());
        }
        let Some(path) = self.existing_path()? else {
            return Ok(false);
        };
        let db = open(&path, false)?;
        sql(db.query_row(
            "SELECT EXISTS(SELECT 1 FROM archive_rows WHERE id=?1)",
            [id],
            |row| row.get(0),
        ))
    }

    pub(super) fn by_id(&self, id: &str) -> Result<Option<Value>> {
        if !session_id::canonical(id) {
            return Err("invalid archive ID".into());
        }
        let Some(path) = self.existing_path()? else {
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
        raw.map(|raw| self.decode(id, &raw)).transpose()
    }

    fn decode(&self, id: &str, raw: &[u8]) -> Result<Value> {
        if raw.len() as u64 > storage::ID_RECORD_BYTES {
            return Err("Native archive is too large".into());
        }
        let mut record: Value = serde_json::from_slice(raw).map_err(|error| error.to_string())?;
        rename::normalize_archive(&mut record)?;
        if !session_id::canonical(id)
            || record["version"] != 1
            || record["archive_id"] != id
            || !(self.valid)(&record)
        {
            return Err("invalid Native archive record".into());
        }
        Ok(record)
    }

    /// A complete, bounded read of native archive references. Archive bodies are
    /// saved binding metadata. Native history stays in the provider's own store.
    pub(super) fn visit_references(
        &self,
        until: Instant,
        max_rows: usize,
        max_bytes: usize,
        mut visit: impl FnMut(Value) -> Result<()>,
    ) -> Result<(usize, usize)> {
        let coverage = || {
            format!(
                "{} archive references exceeded their review bound",
                self.provider
            )
        };
        let Some(path) = self.existing_path()? else {
            return Ok((0, 0));
        };
        let db = open(&path, false)?;
        sql(db.busy_timeout(until.saturating_duration_since(Instant::now())))?;
        db.progress_handler(1000, Some(move || Instant::now() >= until));
        let mut query = sql(db.prepare(
            "SELECT substr(CAST(id AS BLOB),1,37), substr(CAST(body AS BLOB),1,?1),
         CASE WHEN length(CAST(body AS BLOB)) < ?1 AND json_valid(body)
           THEN coalesce(json_extract(body,'$.name')=name
                     AND json_extract(body,'$.run_id')=run
                     AND json_extract(body,'$.conversation_id')=conversation, 0) ELSE 0 END
         FROM archive_rows ORDER BY id LIMIT ?2",
        ))?;
        let mut rows = sql(query.query(params![storage::ID_RECORD_BYTES + 1, max_rows + 1]))?;
        let mut count = 0;
        let mut bytes = 0;
        while let Some(row) = sql(rows.next())? {
            count += 1;
            if count > max_rows || Instant::now() >= until {
                return Err(coverage());
            }
            let id =
                std::str::from_utf8(sql(row.get_ref(0))?.as_blob().map_err(|e| e.to_string())?)
                    .map_err(|error| error.to_string())?;
            let raw = sql(row.get_ref(1))?.as_blob().map_err(|e| e.to_string())?;
            bytes += raw.len();
            if bytes > max_bytes {
                return Err(coverage());
            }
            if !sql(row.get::<_, bool>(2))? {
                return Err(format!(
                    "{} archive reference identity changed",
                    self.provider
                ));
            }
            visit(self.decode(id, raw)?)?;
        }
        if Instant::now() >= until {
            return Err(coverage());
        }
        Ok((count, bytes))
    }

    /// Called under the state writer after a complete removal census.
    pub(super) fn mark_worktree_removal(&self, record: &Value, cleanup: &Value) -> Result<()> {
        let id = string(record, "archive_id");
        let path = self
            .existing_path()?
            .ok_or("Native archive disappeared before removal preparation")?;
        let db = open(&path, true)?;
        let prior: Vec<u8> = sql(db.query_row(
            "SELECT substr(CAST(body AS BLOB), 1, ?2) FROM archive_rows WHERE id=?1",
            params![id, storage::ID_RECORD_BYTES + 1],
            |row| row.get(0),
        ))?;
        if self.decode(id, &prior)? != *record {
            return Err("Native archive changed before removal preparation".into());
        }
        let mut updated = record.clone();
        updated["worktree_cleanup"] = cleanup.clone();
        let body = updated.to_string();
        if body.len() as u64 > storage::ID_RECORD_BYTES {
            return Err("Native archive is too large".into());
        }
        let changed = sql(db.execute(
            "UPDATE archive_rows SET body=?1
         WHERE id=?2 AND CAST(body AS BLOB)=?3 AND name=?4 AND run=?5 AND conversation=?6",
            params![
                body,
                id,
                prior,
                record["name"].as_str(),
                record["run_id"].as_str(),
                record["conversation_id"].as_str()
            ],
        ))?;
        if changed != 1 {
            return Err("Native archive changed before removal preparation".into());
        }
        Ok(())
    }

    pub(super) fn publish(&self, record: &Value) -> Result<String> {
        let db = self.writer()?;
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
            let prior = self
                .by_id(&id)?
                .ok_or("Native archive changed before publication")?;
            if !archive::equivalent(record, &[prior]) {
                return Err("Native archive identity changed".into());
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
            return Err("Native archive is too large".into());
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

    pub(super) fn update_name(&self, record: &Value) -> Result<()> {
        let db = self.writer()?;
        let raw = record.to_string();
        if raw.len() as u64 > storage::ID_RECORD_BYTES {
            return Err("Native archive is too large".into());
        }
        let changed = sql(db.execute("UPDATE archive_rows SET name=?2, search=?3, body=?4 WHERE id=?1 AND run=?5 AND conversation=?6",
        params![string(record, "archive_id"), string(record, "name"), archive_catalog::search_text(record), raw,
            string(record, "run_id"), string(record, "conversation_id")]))?;
        if changed != 1 {
            return Err("Native archive changed before rename".into());
        }
        Ok(())
    }

    pub(super) fn remove(&self, record: &Value) -> Result<()> {
        let db = self.writer()?;
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
            return Err("Native archive changed before removal".into());
        }
        Ok(())
    }
}

pub(super) fn open(path: &Path, writable: bool) -> Result<Connection> {
    let mode = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let db = sql(Connection::open_with_flags(
        path,
        mode | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    ))?;
    let wait = storage::listing_scope::deadline().map_or(Duration::from_secs(2), |deadline| {
        deadline.saturating_duration_since(Instant::now())
    });
    sql(db.busy_timeout(wait))?;
    Ok(db)
}
