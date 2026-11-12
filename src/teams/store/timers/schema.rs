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
pub(crate) const EXTENSION: u32 = 8;

#[cfg(test)]
type Hook = Box<dyn FnOnce() -> Result<()>>;
#[cfg(test)]
thread_local! {
    /// Runs inside the transition, after the backfills, before the marker.
    pub(crate) static BEFORE_MARK: std::cell::RefCell<Option<Hook>> =
        const { std::cell::RefCell::new(None) };
    /// Transitions completed on this thread.
    pub(crate) static TRANSITIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Runs on the open path right after the first, unlocked fence.
    pub(crate) static AFTER_FENCE: std::cell::RefCell<Option<Hook>> =
        const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(in crate::teams::store) fn after_fence() -> Result<()> {
    match AFTER_FENCE.with(|slot| slot.borrow_mut().take()) {
        Some(hook) => hook(),
        None => Ok(()),
    }
}

/// Read first and read only, before any statement of the store's schema
/// extensions that can write: a store marked by a newer program is refused
/// and nothing is written to it. Every write of the open path reads the mark
/// again under the lock it writes with: the common batches, the read index
/// conversion and the transition.
pub(in crate::teams::store) fn fence(db: &Connection) -> Result<()> {
    marked(db).map(|_| ())
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
        let result = marked(db).and_then(|done| if done { Ok(()) } else { transition(db) });
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
/// Whether the completion marker holds this layout's version. A marker newer
/// than this program's is a store written by a newer program: the open is
/// refused as for a newer schema, and nothing is written.
fn marked(db: &Connection) -> Result<bool> {
    match version(db)? {
        // The same refusal as a newer core schema's.
        Some(v) if v > EXTENSION => Err(Problem::new(
            code::INVALID_STATE,
            "store schema is newer than this version of Zerus",
        )),
        v => Ok(v == Some(EXTENSION)),
    }
}
/// The layout version the completion marker holds, if any.
fn version(db: &Connection) -> Result<Option<u32>> {
    let table: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='timers_extension')",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !table {
        return Ok(None);
    }
    db.query_row("SELECT version FROM timers_extension WHERE id=1", [], |r| {
        r.get(0)
    })
    .optional()
    .map_err(storage)
}
fn transition(db: &Connection) -> Result<()> {
    let from = version(db)?;
    // Triggers exist before any backfill reads, so a write that commits
    // after this transaction is seen by them, and none can commit inside it.
    db.execute_batch(include_str!("../timers.sql"))
        .map_err(storage)?;
    released_outcomes(db)?;
    observed_attempts(db)?;
    remembered_attempts(db, from)?;
    migrate_outbox(db)?;
    backfill_teams(db)?;
    backfill_released(db)?;
    backfill_stopped(db)?;
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
/// A task observation keeps the attempt it shows beside its data: columns of
/// package 6's table, in its definition for a new store and added here to a
/// table an older store already has.
fn observed_attempts(db: &Connection) -> Result<()> {
    for (column, kind) in [
        ("attempt", "INTEGER NOT NULL DEFAULT 0"),
        ("attempt_id", "TEXT NOT NULL DEFAULT ''"),
        ("last_attempt_id", "TEXT NOT NULL DEFAULT ''"),
        ("attempt_conflict", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        let present: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('task_observations') WHERE name=?1)",
                [column],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if !present {
            let sql = format!("ALTER TABLE task_observations ADD COLUMN {column} {kind}");
            db.execute_batch(&sql).map_err(storage)?;
        }
    }
    Ok(())
}
/// The attempt a node runs under is remembered only when the node took it
/// in: its own effect, or a person's execution the store took over, which a
/// stored observation of a running task shows under the rules that wrote
/// it. Layout 4 kept no memory and is seeded so; layouts 5 and 6 remembered
/// every observed attempt and are corrected the same way, so that an
/// attempt merely observed while the task did not run is judged when it
/// starts. A store from before the attempt columns has nothing to seed from,
/// and a later layout keeps what its rules wrote. One statement, once.
fn remembered_attempts(db: &Connection, from: Option<u32>) -> Result<()> {
    if matches!(from, Some(4..=6)) {
        db.execute(TAKEN_IN, []).map_err(storage)?;
    }
    Ok(())
}
const TAKEN_IN: &str = "UPDATE task_observations SET last_attempt_id=CASE WHEN (json_extract(data,'$.stage')='in_progress' AND NOT json_extract(data,'$.cancelled')) OR EXISTS(SELECT 1 FROM effects e WHERE e.request_id=task_observations.attempt_id AND e.team=task_observations.team AND e.node=task_observations.node) THEN attempt_id ELSE '' END WHERE attempt_id<>''";
/// Before layout 8 a cancel outcome was cancelled or refused; the table is
/// rebuilt once with its current definition, which also takes released.
/// Its rows move as they are; no trigger or view reads it.
fn released_outcomes(db: &Connection) -> Result<()> {
    let sql: Option<String> = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='cancel_outcomes'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if sql.is_none_or(|sql| sql.contains("'released'")) {
        return Ok(());
    }
    db.execute_batch(
        "DROP INDEX IF EXISTS cancel_refusals;
         ALTER TABLE cancel_outcomes RENAME TO cancel_outcomes_v1;",
    )
    .map_err(storage)?;
    db.execute_batch(include_str!("../timers.sql"))
        .map_err(storage)?;
    db.execute_batch(
        "INSERT INTO cancel_outcomes(decision,team,node,generation,state,reason,at_ms)
          SELECT decision,team,node,generation,state,reason,at_ms FROM cancel_outcomes_v1;
         DROP TABLE cancel_outcomes_v1;",
    )
    .map_err(storage)
}
/// Version 1 kept released stages in an outbox; they move to the frontier.
fn migrate_outbox(db: &Connection) -> Result<()> {
    let old: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='key_clear_outbox')",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if old {
        db.execute_batch(
            "INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT team,node,'stage' FROM key_clear_outbox;
             DROP TABLE key_clear_outbox;",
        )
        .map_err(storage)?;
    }
    Ok(())
}
/// Walk the archive once, in pages, for stages released before the frontier.
fn backfill_released(db: &Connection) -> Result<()> {
    let page =
        "SELECT team,id FROM node_archive WHERE (team,id)>(?1,?2) ORDER BY team,id LIMIT 200";
    let insert = format!("INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT a.team,a.id,'stage' FROM node_archive a WHERE a.team=?1 AND a.id=?2 AND {RELEASED}");
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
/// A stopped or archived team's open nodes whose cancel settled before the
/// frontier existed; one statement per pending team, over its live nodes.
fn backfill_stopped(db: &Connection) -> Result<()> {
    let mut insert = db.prepare(STOPPED).map_err(storage)?;
    let mut after = String::new();
    loop {
        let teams = column::<String>(
            db,
            "SELECT team FROM key_clear_teams WHERE pending=1 AND team>?1 ORDER BY team LIMIT 200",
            params![after],
        )?;
        for team in &teams {
            insert.execute([team]).map_err(storage)?;
        }
        match teams.last() {
            Some(last) if teams.len() == 200 => after = last.clone(),
            _ => return Ok(()),
        }
    }
}
const STOPPED: &str = "INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT n.team,n.id,'stop' FROM nodes n JOIN node_identity i ON i.team=n.team AND i.id=n.id LEFT JOIN stage_tasks s ON s.team=n.team AND s.node=n.id WHERE n.team=?1 AND (i.task_uuid<>'' OR s.request_id IS NOT NULL) AND NOT EXISTS(SELECT 1 FROM decisions d WHERE d.team=n.team AND d.node=n.id AND d.state<>'settled' AND d.kind='cancel') AND NOT EXISTS(SELECT 1 FROM task_observations o WHERE o.team=n.team AND o.node=n.id AND json_extract(o.data,'$.key') IS NOT NULL) AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=n.team AND k.node=n.id)";
