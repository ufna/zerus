//! This package's additions to a store, as one transition: the tables,
//! indexes and triggers first, then the backfills, then a completion marker,
//! all inside one immediate transaction. A store without the marker,
//! including one that an earlier, partial extension left, runs the whole
//! transition again; every step is idempotent. Opening a marked store reads
//! one row and takes no write lock.
//!
//! The one-time cost: the backfills walk the teams once and every archived
//! node once, in pages of 200 keys, under the write lock; memory stays one
//! page. Measured here at about 33 SQLite VM steps and 6 microseconds per
//! archived node (10,000 in 60 ms), so a million archived nodes hold the
//! lock for several seconds, once. Another process opening the store
//! meanwhile waits for it, up to a minute, instead of the usual two seconds.
use super::*;
use std::time::Duration;

/// Raised whenever this package's layout changes.
pub(crate) const EXTENSION: u32 = 1;

#[cfg(test)]
type Hook = Box<dyn FnOnce() -> Result<()>>;
#[cfg(test)]
thread_local! {
    /// Runs inside the transition, after the backfills, before the marker.
    pub(crate) static BEFORE_MARK: std::cell::RefCell<Option<Hook>> =
        const { std::cell::RefCell::new(None) };
    /// Transitions completed on this thread.
    pub(crate) static TRANSITIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub(in crate::teams::store) fn extend(db: &Connection) -> Result<()> {
    if marked(db)? {
        return Ok(());
    }
    let own = db.is_autocommit();
    let prior: u64 = db
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .map_err(storage)?;
    if own {
        db.busy_timeout(Duration::from_secs(60)).map_err(storage)?;
    }
    let begun = db.execute_batch(if own {
        "BEGIN IMMEDIATE;"
    } else {
        "SAVEPOINT timers;"
    });
    let result = begun.map_err(storage).and_then(|()| {
        let result = if marked(db)? { Ok(()) } else { transition(db) };
        let end = match (&result, own) {
            (Ok(()), true) => "COMMIT;",
            (Ok(()), false) => "RELEASE timers;",
            (Err(_), true) => "ROLLBACK;",
            (Err(_), false) => "ROLLBACK TO timers; RELEASE timers;",
        };
        db.execute_batch(end).map_err(storage)?;
        result
    });
    if own {
        db.busy_timeout(Duration::from_millis(prior))
            .map_err(storage)?;
    }
    result
}
/// The completion marker holds this layout's version.
fn marked(db: &Connection) -> Result<bool> {
    let table: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='timers_extension')",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !table {
        return Ok(false);
    }
    let version: Option<u32> = db
        .query_row("SELECT version FROM timers_extension WHERE id=1", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(storage)?;
    Ok(version == Some(EXTENSION))
}
fn transition(db: &Connection) -> Result<()> {
    // Triggers exist before any backfill reads, so a write that commits
    // after this transaction is seen by them, and none can commit inside it.
    db.execute_batch(include_str!("../timers.sql"))
        .map_err(storage)?;
    backfill_teams(db)?;
    backfill_outbox(db)?;
    #[cfg(test)]
    if let Some(hook) = BEFORE_MARK.with(|slot| slot.borrow_mut().take()) {
        hook()?;
    }
    db.execute(
        "INSERT INTO timers_extension(id,version) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET version=excluded.version",
        [EXTENSION],
    )
    .map_err(storage)?;
    #[cfg(test)]
    TRANSITIONS.with(|n| n.set(n.get() + 1));
    Ok(())
}
fn backfill_teams(db: &Connection) -> Result<()> {
    let sql =
        format!("SELECT id FROM teams WHERE id>?1 AND status IN {TERMINAL} ORDER BY id LIMIT 200");
    let mut insert = db
        .prepare("INSERT OR IGNORE INTO key_clear_teams(team,pending) VALUES(?1,1)")
        .map_err(storage)?;
    let mut after = String::new();
    loop {
        let ids = column::<String>(db, &sql, params![after])?;
        for id in &ids {
            insert.execute([id]).map_err(storage)?;
        }
        match ids.last() {
            Some(last) if ids.len() == 200 => after = last.clone(),
            _ => return Ok(()),
        }
    }
}
/// Walk the archive once, in pages, for stages released before the outbox.
fn backfill_outbox(db: &Connection) -> Result<()> {
    let page =
        "SELECT team,id FROM node_archive WHERE (team,id)>(?1,?2) ORDER BY team,id LIMIT 200";
    let insert = format!("INSERT OR IGNORE INTO key_clear_outbox(team,node) SELECT a.team,a.id FROM node_archive a WHERE a.team=?1 AND a.id=?2 AND {RELEASED}");
    let mut insert = db.prepare(&insert).map_err(storage)?;
    let mut after = (String::new(), 0u32);
    loop {
        let keys = rows(db, page, params![after.0, after.1], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
        })?;
        for (team, id) in &keys {
            insert.execute(params![team, id]).map_err(storage)?;
        }
        match keys.last() {
            Some(last) if keys.len() == 200 => after = last.clone(),
            _ => return Ok(()),
        }
    }
}
