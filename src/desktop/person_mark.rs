//! Person message marks (zerus-46l1q, S1). A mark says that the person typed
//! one message in the desktop composer and the window admitted it for one
//! exact session, run and conversation. Only the deferred-send frontend
//! admission writes one, in the same transaction as its original row
//! ([`record`]); question routes, `hgs send`, launches, hand-outs, task
//! dispatch, timers, recovery and keep-warm never do.
//!
//! A mark grants nothing by itself. The prompt hook of the asked session
//! binds it to the native turn that accepts exactly that input
//! (`state::person_turn`), and `caller::person_request` reads both again. The
//! CLI reads marks only through [`lookup`], read-only, from this computer's
//! own desktop store at its canonical location.
use super::store::{self, Location};
use crate::config::Config;
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use std::time::Duration;
use zerus_client_core::desktop::deferred_send::{Kind, Verified};

// Merge-order allocation: the next free Store version when this lands. A
// later schema merge renumbers this one constant, never standalone DDL.
pub(super) const SCHEMA: i64 = 43;
/// The most marks the table keeps. A full table writes no new mark: the send
/// goes on, unmarked, and grants nothing.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
const ROWS: i64 = 4096;
/// The longest a person turn lasts (owner decision, 2026-11-18), in
/// milliseconds.
pub(crate) const TURN_MS: i64 = 12 * 3600 * 1000;
/// Marks are kept a little longer than a turn can last, then removed by the
/// next admission.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
const KEEP_MS: i64 = TURN_MS + 3600 * 1000;
/// The only origin S1 writes; the phone (S1b) adds its own.
pub(crate) const DESKTOP: &str = "desktop";

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE desktop_person_marks(
        request_id TEXT PRIMARY KEY CHECK(length(CAST(request_id AS BLOB))=36),
        session TEXT NOT NULL CHECK(length(CAST(session AS BLOB)) BETWEEN 1 AND 512),
        run_id TEXT NOT NULL CHECK(length(CAST(run_id AS BLOB)) BETWEEN 1 AND 128),
        conversation_id TEXT NOT NULL CHECK(length(CAST(conversation_id AS BLOB)) BETWEEN 1 AND 128),
        origin TEXT NOT NULL CHECK(origin IN('desktop')),
        input_sha256 TEXT NOT NULL CHECK(length(CAST(input_sha256 AS BLOB))=64),
        at INTEGER NOT NULL CHECK(at>0));
        CREATE INDEX desktop_person_marks_at ON desktop_person_marks(at,request_id);
        CREATE TRIGGER desktop_person_mark_original BEFORE UPDATE ON desktop_person_marks BEGIN
        SELECT RAISE(ABORT,'immutable person mark'); END;",
    )?;
    Ok(())
}

/// Test rewind of this leaf for older-version upgrade fixtures.
#[cfg(test)]
pub(crate) fn remove_fixture(db: &Connection) {
    db.execute_batch("DROP TABLE desktop_person_marks;")
        .unwrap();
}

/// Writes the mark of the person's own deferred send `original`, inside the
/// frontend admission's writer transaction and only for its first admission.
/// Expired marks go first; a full table, an empty conversation (a first
/// message has none yet) or a request already marked writes nothing, and the
/// send itself is never refused for it. Returns whether a mark was written.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
pub(super) fn record(tx: &Transaction<'_>, original: &Verified, at: i64) -> Result<bool> {
    let binding = &original.binding;
    ensure!(
        binding.kind == Kind::DeferredSend,
        "only the person's deferred send is marked"
    );
    ensure!(at > 0, "invalid person mark time");
    if binding.conversation_id.is_empty() {
        return Ok(false);
    }
    tx.execute(
        "DELETE FROM desktop_person_marks WHERE at<?1",
        [at - KEEP_MS],
    )?;
    let count: i64 = tx.query_row("SELECT count(*) FROM desktop_person_marks", [], |r| {
        r.get(0)
    })?;
    if count >= ROWS {
        return Ok(false);
    }
    let written = tx.execute(
        "INSERT OR IGNORE INTO desktop_person_marks VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            binding.request_id,
            binding.session,
            binding.run_id,
            binding.conversation_id,
            DESKTOP,
            binding.input_sha256,
            at
        ],
    )?;
    Ok(written == 1)
}

/// One mark, as [`lookup`] read it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mark {
    pub session: String,
    pub run_id: String,
    pub conversation_id: String,
    pub origin: String,
    /// The SHA-256 of the exact `hgs send` stdin, which the CLI's receipt of
    /// this request records as its digest.
    pub input_sha256: String,
    /// When the window admitted it, in Unix milliseconds.
    pub at: i64,
}

/// The mark of `request_id` in this computer's desktop store, read-only, or
/// `None`: no store, an older schema, another configuration's store (a copy
/// or a redirected state folder), no such mark, or a row outside its bounds.
/// It never creates, migrates or locks anything.
pub(crate) fn lookup(config: &Config, request_id: &str) -> Option<Mark> {
    lookup_at(&Location::new(config).ok()?, request_id)
        .ok()
        .flatten()
}

fn lookup_at(location: &Location, request_id: &str) -> Result<Option<Mark>> {
    if !canonical_uuid(request_id) {
        return Ok(None);
    }
    location.check_existing()?;
    let path = location.root.join("state.sqlite3");
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    store::database_files(location)?;
    let db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    db.busy_timeout(Duration::from_millis(250))?;
    read(&db, &location.configuration, request_id)
}

fn read(db: &Connection, configuration: &str, request_id: &str) -> Result<Option<Mark>> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < SCHEMA {
        return Ok(None);
    }
    // The store of this exact state and configuration folder: a copied or
    // redirected store names the folders it was made for.
    let own: String = db.query_row(
        "SELECT substr(configuration,1,65) FROM service WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(own == configuration, "desktop configuration mismatch");
    let row = db
        .query_row(
            "SELECT session,run_id,conversation_id,origin,input_sha256,at FROM desktop_person_marks WHERE request_id=?1",
            [request_id],
            |r| {
                Ok(Mark {
                    session: r.get(0)?,
                    run_id: r.get(1)?,
                    conversation_id: r.get(2)?,
                    origin: r.get(3)?,
                    input_sha256: r.get(4)?,
                    at: r.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(row.filter(|mark| {
        !mark.session.is_empty()
            && !mark.run_id.is_empty()
            && !mark.conversation_id.is_empty()
            && mark.origin == DESKTOP
            && mark.input_sha256.len() == 64
            && mark.input_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            && mark.at > 0
    }))
}

fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}

/// The rows of the table, for the tests of every writer: a mark's presence
/// is the producer's word, never a fixture's.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) fn count(db: &Connection) -> i64 {
    db.query_row("SELECT count(*) FROM desktop_person_marks", [], |r| {
        r.get(0)
    })
    .unwrap()
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) fn read_for_test(
    db: &Connection,
    configuration: &str,
    request_id: &str,
) -> Option<Mark> {
    read(db, configuration, request_id).unwrap()
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(crate) fn lookup_in(location: &Location, request_id: &str) -> Option<Mark> {
    lookup_at(location, request_id).ok().flatten()
}
