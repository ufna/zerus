//! Private, lazy Teams storage. Reading a missing store never creates it.
#![allow(dead_code)] // Consumed by the following CLI and service packages.

#[cfg(test)]
mod tests;

mod pages;
mod records;
pub(crate) mod rows;
pub use records::*;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zerus_team_plan::{model::*, problem::code, Problem};

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
    pub fn member(&self, team: &str, id: &str) -> Result<Option<Member>> {
        self.active()?;
        self.db
            .as_ref()
            .map_or(Ok(None), |db| rows::member(db, team, id))
    }
    pub fn node(&self, team: &str, id: NodeId) -> Result<Option<Node>> {
        self.active()?;
        self.db
            .as_ref()
            .map_or(Ok(None), |db| rows::node(db, team, id))
    }
    pub fn task_observation(
        &self,
        team: &str,
        id: NodeId,
    ) -> Result<Option<super::ops::TaskObservation>> {
        self.active()?;
        self.db.as_ref().map_or(Ok(None),|db| {
            let raw: Option<(String,String,bool,String)> = db.query_row(
                "SELECT task_revision,task_stage,task_cancelled,rank FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT task_revision,task_stage,task_cancelled,rank FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1",
                rusqlite::params![team,id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage)?;
            raw.map(|(revision,stage,cancelled,rank)|Ok(super::ops::TaskObservation{previous_revision:revision.clone(),revision,stage:serde_json::from_value(serde_json::Value::String(stage)).map_err(storage)?,cancelled,rank})).transpose()
        })
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
    let version: u32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(storage)?;
    if version > 1 {
        return Err(storage("store schema is newer than this version of Zerus"));
    }
    if version == 0 {
        if !create {
            return Err(storage("uninitialized store"));
        }
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        // Another first creator may have initialized the database while we waited.
        let current: u32 = tx
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(storage)?;
        if current == 0 {
            tx.execute_batch(include_str!("schema.sql"))
                .map_err(storage)?;
        } else if current != 1 {
            return Err(storage("store schema changed"));
        }
        tx.commit().map_err(storage)?;
    }
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
        .map_err(storage)?;
    Ok(db)
}
