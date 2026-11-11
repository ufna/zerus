//! Private, lazy Teams storage. Reading a missing store never creates it.
#![allow(dead_code)] // Consumed by the following CLI and service packages.

#[cfg(test)]
mod tests;

mod cross;
mod decision_state;
mod holder;
mod member_sessions;
pub use decision_state::DecisionState;
mod pages;
mod records;
pub(crate) mod rows;
mod stage;
pub use records::*;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zerus_team_plan::{model::*, problem::code, Problem};

pub(crate) const OPEN_EFFECT_STATES: &str = "('reserved','submitted','uncertain','cutting_over')";
pub(crate) const SCHEMA_VERSION: u32 = 2;

pub type Result<T> = std::result::Result<T, Problem>;

#[derive(Debug)]
pub struct Store {
    state: PathBuf,
    pub(crate) db: Option<Connection>,
}

pub(crate) fn storage(error: impl std::fmt::Display) -> Problem {
    Problem::new(
        code::INVALID_STATE,
        format!("Teams storage is unavailable: {error}"),
    )
}
pub(crate) fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(storage)
}
pub(crate) fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(storage)
}
pub(crate) fn sql_json<T: DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    col: usize,
) -> rusqlite::Result<T> {
    let value: String = row.get(col)?;
    serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl Store {
    pub fn open(state: &Path) -> Result<Self> {
        if !super::enabled(state) {
            return Err(Problem::new(code::TEAMS_INACTIVE, "Teams preview is off."));
        }
        let path = state.join("teams/teams-v1.sqlite3");
        let db = match std::fs::symlink_metadata(&path) {
            Ok(_) => Some(connect(&path, false)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(storage(e)),
        };
        Ok(Self {
            state: state.to_owned(),
            db,
        })
    }

    /// Retry discovery after another process creates the first team. Absence
    /// is never persisted as an inventory fact and this never creates a file.
    pub fn refresh(&mut self) -> Result<()> {
        self.active()?;
        if self.db.is_none() {
            let path = self.state.join("teams/teams-v1.sqlite3");
            match std::fs::symlink_metadata(&path) {
                Ok(_) => self.db = Some(connect(&path, false)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(storage(e)),
            }
        }
        Ok(())
    }
    pub(crate) fn ensure(&mut self) -> Result<()> {
        self.active()?;
        if self.db.is_none() {
            let dir = self.state.join("teams");
            crate::platform::private_dir(&dir).map_err(storage)?;
            self.db = Some(connect(&dir.join("teams-v1.sqlite3"), true)?);
        }
        Ok(())
    }
    pub(crate) fn active(&self) -> Result<()> {
        if super::enabled(&self.state) {
            Ok(())
        } else {
            Err(Problem::new(code::TEAMS_INACTIVE, "Teams preview is off."))
        }
    }
    pub fn team(&self, id: &str) -> Result<Option<Team>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None), |db| rows::team(db, id))
    }
    pub fn suggestion(&self, team: &str, node: u32) -> Result<Option<RoutingSuggestion>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None), |db| {
            db.query_row(
                "SELECT role,pipeline FROM suggestions WHERE team=?1 AND node=?2",
                rusqlite::params![team, node],
                |r| {
                    Ok(RoutingSuggestion {
                        role: r.get(0)?,
                        pipeline: r.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(storage)
        })
    }
    pub fn card(&self, team: &str, seq: u64) -> Result<Option<Card>> {
        self.active()?;
        self.db
            .as_ref()
            .map_or(Ok(None), |db| rows::card(db, team, seq))
    }
    pub fn member(&self, team: &str, id: &str) -> Result<Option<Member>> {
        self.active()?;
        self.db
            .as_ref()
            .map_or(Ok(None), |db| rows::member(db, team, id))
    }
    pub fn node(&self, team: &str, id: NodeId) -> Result<Option<Node>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None), |db| {
            let tx = db.unchecked_transaction().map_err(storage)?;
            let node = rows::node(&tx, team, id)?;
            tx.commit().map_err(storage)?;
            Ok(node)
        })
    }
    pub fn task_observation(
        &self,
        team: &str,
        id: NodeId,
    ) -> Result<Option<super::ops::TaskObservation>> {
        self.active()?;
        self.db
            .as_ref()
            .map_or(Ok(None), |db| rows::observation(db, team, id))
    }
    pub fn integration_fact(&self, team: &str, node: NodeId) -> Result<Option<IntegrationFact>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None), |db| db.query_row(
            "SELECT decision,node,actor,commit_id,conflicts,created_ms FROM integration_facts WHERE team=?1 AND node=?2 ORDER BY decision DESC LIMIT 1",
            rusqlite::params![team,node], |r|Ok(IntegrationFact{decision:r.get(0)?,node:r.get(1)?,actor:sql_json(r,2)?,commit:r.get(3)?,conflicts:sql_json(r,4)?,created_ms:r.get(5)?}))
            .optional().map_err(storage))
    }
    pub fn generation(&self, team: &str, id: NodeId) -> Result<Option<u32>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None), |db| db.query_row(
            "SELECT generation FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT generation FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1",
            rusqlite::params![team,id], |r| r.get(0)).optional().map_err(storage))
    }
}

fn connect(path: &Path, create: bool) -> Result<Connection> {
    let dir = path
        .parent()
        .ok_or_else(|| storage("missing store directory"))?;
    if !crate::platform::verify_private(dir, crate::platform::Private::Directory)
        .map_err(storage)?
    {
        return Err(storage("store directory is not private"));
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let leaf = dir.join(format!("teams-v1.sqlite3{suffix}"));
        match crate::platform::verify_private(&leaf, crate::platform::Private::File) {
            Ok(true) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err(storage("store file is not private")),
        }
    }
    #[cfg(unix)]
    let identity = {
        if create {
            crate::sqlite_file::create(path).map_err(storage)?;
        }
        crate::sqlite_file::private(path).map_err(storage)?
    };
    #[cfg(not(unix))]
    if create {
        let file = crate::platform::no_follow(crate::platform::private_open(
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true),
        ))
        .open(path)
        .map_err(storage)?;
        if !crate::platform::verify_private_file(&file).map_err(storage)? {
            return Err(storage("store file is not private"));
        }
    }
    let mut db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(storage)?;
    #[cfg(unix)]
    crate::sqlite_file::bind_private(&db, path, identity).map_err(storage)?;
    db.busy_timeout(Duration::from_secs(2)).map_err(storage)?;
    initialize(&mut db)?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
        .map_err(storage)?;
    Ok(db)
}

fn initialize(db: &mut Connection) -> Result<()> {
    let version: u32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(storage)?;
    if version == 1 {
        return Err(storage(
            "Version-1 Teams store belongs to an unsupported pre-release build.",
        ));
    }
    if version > SCHEMA_VERSION {
        return Err(storage("store schema is newer than this version of Zerus"));
    }
    if version == SCHEMA_VERSION {
        return schema_extensions(db);
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(storage)?;
    let current: u32 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(storage)?;
    let sql = match current {
        0 => Some(include_str!("schema.sql")),
        1 => {
            return Err(storage(
                "Version-1 Teams store belongs to an unsupported pre-release build.",
            ))
        }
        SCHEMA_VERSION => None,
        _ => return Err(storage("store schema changed")),
    };
    if let Some(sql) = sql {
        tx.execute_batch(
            &sql.replace("__OPEN_EFFECT_STATES__", OPEN_EFFECT_STATES)
                .replace("__DECISION_PENDING__", DecisionState::Pending.as_str())
                .replace("__DECISION_SETTLED__", DecisionState::Settled.as_str()),
        )
        .map_err(storage)?;
    }
    schema_extensions(&tx)?;
    tx.commit().map_err(storage)
}

// Add empty companion storage and missing read indexes on first creation
// and when a version-2 store from before these seams is opened.
fn schema_extensions(db: &Connection) -> Result<()> {
    db.execute_batch(include_str!("pipeline-schema.sql"))
        .map_err(storage)?;
    db.execute_batch(include_str!("observation-schema.sql"))
        .map_err(storage)?;
    db.execute_batch(include_str!("read-indexes.sql"))
        .map_err(storage)
}
