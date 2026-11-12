//! This package's additions to a store. The extension knows two stores:
//! one from before it (no mark), which it extends to the current layout in
//! one immediate transaction (the tables, indexes and triggers first, then
//! the backfills, then the mark), and one at a layout that reached the
//! integration branch, which it steps forward to the current one. A mark
//! that only an unmerged development build could have written is refused
//! with a sentence that says so, and nothing is written; a newer mark is
//! refused as a newer schema is. Opening a store at the current layout
//! reads one row and takes no write lock.
//!
//! The one-time cost: the backfills walk the teams once and every archived
//! node once, in pages of 200 keys, under the write lock; memory stays one
//! page. Measured here at about 33 SQLite VM steps and 6 microseconds per
//! archived node (10,000 in 60 ms), so a million archived nodes hold the
//! lock for several seconds, once. Another process opening the store
//! meanwhile waits for it, up to a minute, instead of the usual two seconds.
use super::*;
use std::time::Duration;

/// The layout the whole transition builds before any step: the first that
/// reached the integration branch, with the timers. Every lower value was
/// written only by unmerged development builds of this package.
pub(crate) const BASE: u32 = 8;
/// The current layout: the base and the steps after it.
pub(crate) const EXTENSION: u32 = 9;
/// Each layout that reached the integration branch, with the step that
/// brings a store marked with it to the next; the steps run in order up to
/// the current layout, after the base for a store from before the extension.
const STEPS: &[(u32, Step)] = &[(8, answers)];
/// One step from a merged layout to the next, inside the transition.
type Step = fn(&Connection) -> Result<()>;
/// Layout 8 to 9: the lead's answers reach their members.
fn answers(db: &Connection) -> Result<()> {
    db.execute_batch(include_str!("../answers.sql"))
        .map_err(storage)
}

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
/// extensions that can write: a store marked by a newer program or by an
/// unmerged development build is refused and nothing is written to it.
/// Every write of the open path reads the mark again under the lock it
/// writes with: the common batches, the read index conversion and the
/// transition.
pub(in crate::teams::store) fn fence(db: &Connection) -> Result<()> {
    marked(db).map(|_| ())
}
pub(in crate::teams::store) fn extend(db: &Connection) -> Result<()> {
    if marked(db)? == Mark::Current {
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
        let result = marked(db).and_then(|mark| match mark {
            Mark::Current => Ok(()),
            mark => transition(db, mark),
        });
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
/// What the completion marker says about a store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// A store from before the extension.
    Absent,
    /// An earlier layout that reached the integration branch.
    Merged(u32),
    Current,
}
/// The store's mark. A newer one is a store written by a newer program and
/// is refused as a newer schema is; any other value was written only by an
/// unmerged development build and is refused too. Nothing is written.
fn marked(db: &Connection) -> Result<Mark> {
    match version(db)? {
        None => Ok(Mark::Absent),
        Some(EXTENSION) => Ok(Mark::Current),
        // The same refusal as a newer core schema's.
        Some(v) if v > EXTENSION => Err(Problem::new(
            code::INVALID_STATE,
            "store schema is newer than this version of Zerus",
        )),
        Some(v) if STEPS.iter().any(|(from, _)| *from == v) => Ok(Mark::Merged(v)),
        Some(_) => Err(Problem::new(
            code::INVALID_STATE,
            "store schema was written by an unmerged development build of Zerus, and this version does not open it",
        )),
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
fn transition(db: &Connection, mark: Mark) -> Result<()> {
    let from = match mark {
        Mark::Absent => {
            // Triggers exist before any backfill reads, so a write that
            // commits after this transaction is seen by them, and none can
            // commit inside it.
            db.execute_batch(include_str!("../timers.sql"))
                .map_err(storage)?;
            observed_attempts(db)?;
            backfill_teams(db)?;
            backfill_released(db)?;
            backfill_stopped(db)?;
            BASE
        }
        Mark::Merged(from) => from,
        Mark::Current => return Ok(()),
    };
    for (_, step) in STEPS.iter().filter(|(layout, _)| *layout >= from) {
        step(db)?;
    }
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
/// table a store from before the extension already has. Such a store holds
/// no attempt of any task, so its first observation of a pair is judged
/// against no attempt, as for a task that never recorded one.
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
