use crate::{platform, teams::store::storage};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};
use zerus_team_plan::Problem;
type Result<T> = std::result::Result<T, Problem>;
const PENDING_NODE: &str = "SELECT EXISTS(SELECT 1 FROM actor_rows INDEXED BY actor_delivery_node WHERE team=?1 AND json_extract(data,'$.node')=?2 AND key>='delivery:' AND key<'delivery;')";
const PENDING_MEMBER: &str = "SELECT EXISTS(SELECT 1 FROM actor_rows INDEXED BY actor_delivery_member WHERE team=?1 AND json_extract(data,'$.member')=?2 AND key>='delivery:' AND key<'delivery;')";
pub(super) const CAPACITY: &str = "team_actor_capacity";
fn write_error(error: rusqlite::Error) -> Problem {
    if matches!(&error,rusqlite::Error::SqliteFailure(code,_) if code.extended_code==rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER)
    {
        Problem::new(CAPACITY, "Team actor ledger is full")
    } else {
        storage(error)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Row {
    pub team: String,
    pub key: String,
    pub version: u64,
    pub data: Value,
}
pub(super) struct Ledger(Connection);
impl Ledger {
    pub fn open(state: &Path) -> Result<Self> {
        let dir = state.join("teams");
        platform::private_dir(&dir).map_err(storage)?;
        if !platform::verify_private(&dir, platform::Private::Directory).map_err(storage)? {
            return Err(storage("Actor ledger directory is not private"));
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            match platform::verify_private(
                &dir.join(format!("actor-v1.sqlite3{suffix}")),
                platform::Private::File,
            ) {
                Ok(true) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err(storage("Actor ledger is not private")),
            }
        }
        let path = dir.join("actor-v1.sqlite3");
        let db = connection(&path)?;
        db.busy_timeout(Duration::from_secs(2)).map_err(storage)?;
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(storage)?;
        if version > 1 {
            return Err(storage("Actor ledger schema is newer than this build"));
        }
        db.execute_batch(include_str!("ledger.sql"))
            .map_err(storage)?;
        Ok(Self(db))
    }
    pub fn get(&self, team: &str, key: &str) -> Result<Option<Row>> {
        self.0
            .query_row(
                "SELECT version,data FROM actor_rows WHERE team=?1 AND key=?2",
                params![team, key],
                |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage)?
            .map(|(version, data)| {
                Ok(Row {
                    team: team.into(),
                    key: key.into(),
                    version,
                    data: serde_json::from_str(&data).map_err(storage)?,
                })
            })
            .transpose()
    }
    pub fn reserve(&mut self, team: &str, key: &str, data: Value) -> Result<Row> {
        let row = Row {
            team: team.into(),
            key: key.into(),
            version: 0,
            data,
        };
        let changed = self
            .0
            .execute(
                "INSERT OR IGNORE INTO actor_rows(team,key,version,data) VALUES(?1,?2,0,?3)",
                params![team, key, Self::encode(&row)?],
            )
            .map_err(write_error)?;
        if changed != 1 {
            return Err(storage("Actor row already reserved"));
        }
        Ok(row)
    }
    pub fn save(&mut self, row: &mut Row) -> Result<()> {
        let changed = self.0.execute("UPDATE actor_rows SET version=version+1,data=?4 WHERE team=?1 AND key=?2 AND version=?3",params![row.team,row.key,row.version,Self::encode(row)?]).map_err(write_error)?;
        if changed != 1 {
            return Err(storage("Actor row changed"));
        }
        row.version += 1;
        Ok(())
    }
    fn encode(row: &Row) -> Result<String> {
        let raw = serde_json::to_string(&row.data).map_err(storage)?;
        if raw.len() > 16_384 || row.team.len() > 128 || row.key.len() > 256 {
            return Err(storage("Actor row exceeds its bound"));
        }
        Ok(raw)
    }
    pub fn page(&self, team: &str, after: &str) -> Result<Vec<Row>> {
        let mut s=self.0.prepare("SELECT key,version,data FROM actor_rows WHERE team=?1 AND key>?2 ORDER BY key LIMIT 200").map_err(storage)?;
        let rows = s
            .query_map(params![team, after], |r| {
                Ok((r.get::<_, String>(0)?, r.get(1)?, r.get::<_, String>(2)?))
            })
            .map_err(storage)?
            .map(|r| {
                let (key, version, data) = r.map_err(storage)?;
                Ok(Row {
                    team: team.into(),
                    key,
                    version,
                    data: serde_json::from_str(&data).map_err(storage)?,
                })
            })
            .collect();
        rows
    }
    pub fn prefix_page(&self, team: &str, prefix: &str, after: &str) -> Result<Vec<Row>> {
        let mut s=self.0.prepare("SELECT key,version,data FROM actor_rows WHERE team=?1 AND key>=?2 AND key<?3 AND key>?4 ORDER BY key LIMIT 200").map_err(storage)?;
        let rows = s
            .query_map(
                params![team, prefix, format!("{prefix}\u{10ffff}"), after],
                |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get::<_, String>(2)?)),
            )
            .map_err(storage)?
            .map(|r| {
                let (key, version, data) = r.map_err(storage)?;
                Ok(Row {
                    team: team.into(),
                    key,
                    version,
                    data: serde_json::from_str(&data).map_err(storage)?,
                })
            })
            .collect();
        rows
    }
    pub fn merge_pending(&self, team: &str) -> Result<bool> {
        self.0.query_row("SELECT EXISTS(SELECT 1 FROM actor_rows WHERE team=?1 AND key>='merge:' AND key<'merge;')",[team],|r|r.get(0)).map_err(storage)
    }
    pub fn pending_node(&self, team: &str, node: u32) -> Result<bool> {
        self.0
            .query_row(PENDING_NODE, params![team, node], |r| r.get(0))
            .map_err(storage)
    }
    pub fn pending_member(&self, team: &str, member: &str) -> Result<bool> {
        self.0
            .query_row(PENDING_MEMBER, params![team, member], |r| r.get(0))
            .map_err(storage)
    }
    pub fn remove(&mut self, row: &Row) -> Result<()> {
        self.0
            .execute(
                "DELETE FROM actor_rows WHERE team=?1 AND key=?2 AND version=?3",
                params![row.team, row.key, row.version],
            )
            .map_err(storage)?;
        Ok(())
    }
}

fn connection(path: &Path) -> Result<Connection> {
    #[cfg(not(unix))]
    {
        let file = platform::no_follow(platform::private_open(
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true),
        ))
        .open(path)
        .map_err(storage)?;
        if !platform::verify_private_file(&file).map_err(storage)? {
            return Err(storage("Actor ledger is not private"));
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let identity = {
        crate::sqlite_file::create(path).map_err(storage)?;
        crate::sqlite_file::private(path).map_err(storage)?
    };
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(storage)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    crate::sqlite_file::bind_private(&db, path, identity).map_err(storage)?;
    Ok(db)
}

#[cfg(test)]
mod index_tests {
    use super::*;
    #[test]
    fn delivery_point_reads_use_their_partial_indexes() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(temp.path()).unwrap();
        for (sql, index) in [
            (PENDING_NODE, "actor_delivery_node"),
            (PENDING_MEMBER, "actor_delivery_member"),
        ] {
            let mut q = ledger
                .0
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap();
            let plan = q
                .query_map(params!["team", "value"], |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert!(
                plan.iter()
                    .any(|p| p.contains(index) && p.contains("<expr>=?")),
                "{plan:?}"
            );
            assert!(
                !plan.iter().any(|p| p.contains("SCAN actor_rows")),
                "{plan:?}"
            );
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TerminalRetry {
    pub failures: u32,
    pub next_ms: i64,
    pub reason: String,
}
impl Ledger {
    pub fn terminal_retry(&self) -> Result<Option<TerminalRetry>> {
        terminal_retry(&self.0)
    }
    pub fn terminal_failed(&mut self, now: i64, reason: &str) -> Result<()> {
        let tx = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let failures = terminal_retry(&tx)?
            .unwrap_or_default()
            .failures
            .saturating_add(1);
        let delay = super::retry::delay(failures);
        tx.execute("INSERT INTO actor_terminal_retry VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET failures=excluded.failures,next_ms=excluded.next_ms,reason=excluded.reason",params![failures,now.saturating_add(delay),super::cut(reason,2048)]).map_err(storage)?;
        tx.commit().map_err(storage)
    }
    pub fn terminal_succeeded(&mut self) -> Result<()> {
        self.0
            .execute("DELETE FROM actor_terminal_retry WHERE id=1", [])
            .map_err(storage)?;
        Ok(())
    }
}
fn terminal_retry(db: &Connection) -> Result<Option<TerminalRetry>> {
    db.query_row(
        "SELECT failures,next_ms,reason FROM actor_terminal_retry WHERE id=1",
        [],
        |r| {
            Ok(TerminalRetry {
                failures: r.get(0)?,
                next_ms: r.get(1)?,
                reason: r.get(2)?,
            })
        },
    )
    .optional()
    .map_err(storage)
}
