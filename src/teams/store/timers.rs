//! Service reads for the lead's doorbell, the timers and the cleanup after a
//! stop. Each read is one bounded page over live rows through an index; none
//! reads settled history or a whole team.
use super::*;
use rusqlite::params;
use zerus_team_plan::model::{MemberState, SessionKey, TaskRef};

const TERMINAL: &str = "('\"stopped\"','\"archived\"')";

/// One open inbox item, as a timer reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenItem {
    pub seq: u64,
    pub node: u32,
    pub generation: u32,
    pub created_ms: i64,
}
/// The lead's inbox as the doorbell counts it; `open` stops at 201.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxView {
    pub open: u32,
    pub newest: u64,
    pub oldest: Option<OpenItem>,
}
/// A delivered attempt of a live node, with the outcome time when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub node: u32,
    pub generation: u32,
    pub member: String,
    pub outcome_ms: i64,
}
/// A report item that still waits for the lead, and which reminders fired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub item: OpenItem,
    pub lead_reminded: bool,
    pub person_reminded: bool,
}
/// The task of a terminal team's node whose key has no final record yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyedTask {
    pub node: u32,
    pub task: TaskRef,
}
/// The exact binding a doorbell was reserved for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bell {
    pub team: String,
    pub member: String,
    pub session: SessionKey,
    /// The mark of the previous doorbell of this binding.
    pub after: u64,
}

/// Add the timers' companion tables on open. The terminal-team index is
/// filled from existing teams once, when it is created, in pages.
pub(super) fn extend(db: &Connection) -> Result<()> {
    let own = db.is_autocommit();
    db.execute_batch(if own {
        "BEGIN IMMEDIATE;"
    } else {
        "SAVEPOINT timers;"
    })
    .map_err(storage)?;
    let result = (|| {
        let fresh: bool = db.query_row("SELECT NOT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='key_clear_teams')",[],|r|r.get(0)).map_err(storage)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS key_clear_teams (team TEXT PRIMARY KEY, pending INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS key_clear_pending ON key_clear_teams(team) WHERE pending=1;")
            .map_err(storage)?;
        if fresh {
            backfill(db)?;
        }
        db.execute_batch(include_str!("timers.sql"))
            .map_err(storage)
    })();
    let end = match (&result, own) {
        (Ok(()), true) => "COMMIT;",
        (Ok(()), false) => "RELEASE timers;",
        (Err(_), true) => "ROLLBACK;",
        (Err(_), false) => "ROLLBACK TO timers; RELEASE timers;",
    };
    db.execute_batch(end).map_err(storage)?;
    result
}
fn backfill(db: &Connection) -> Result<()> {
    let sql =
        format!("SELECT id FROM teams WHERE id>?1 AND status IN {TERMINAL} ORDER BY id LIMIT 200");
    let mut after = String::new();
    loop {
        let ids = column::<String>(db, &sql, params![after])?;
        for id in &ids {
            db.execute(
                "INSERT OR IGNORE INTO key_clear_teams(team,pending) VALUES(?1,1)",
                [id],
            )
            .map_err(storage)?;
        }
        match ids.last() {
            Some(last) if ids.len() == 200 => after = last.clone(),
            _ => return Ok(()),
        }
    }
}
fn column<T: rusqlite::types::FromSql>(
    db: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<T>> {
    let mut s = db.prepare(sql).map_err(storage)?;
    let rows = s.query_map(args, |r| r.get(0)).map_err(storage)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)
}
fn rows<T>(
    db: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut s = db.prepare(sql).map_err(storage)?;
    let rows = s.query_map(args, map).map_err(storage)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)
}
fn item(r: &rusqlite::Row<'_>) -> rusqlite::Result<OpenItem> {
    Ok(OpenItem {
        seq: r.get(0)?,
        node: r.get(1)?,
        generation: r.get(2)?,
        created_ms: r.get(3)?,
    })
}
fn limit(value: u32) -> Result<u32> {
    if (1..=MAX_PAGE).contains(&value) {
        Ok(value)
    } else {
        Err(Problem::new(
            code::OVER_CAP,
            "Page limit must be between 1 and 200.",
        ))
    }
}

/// The reads, named so that their query plans are checked by tests.
pub(crate) mod sql {
    pub const NEWEST: &str =
        "SELECT seq FROM inbox WHERE team=?1 AND state='open' ORDER BY seq DESC LIMIT 1";
    pub const OPEN: &str =
        "SELECT count(*) FROM (SELECT 1 FROM inbox WHERE team=?1 AND state='open' LIMIT 201)";
    pub const OLDEST: &str = "SELECT seq,node,generation,created_ms FROM inbox WHERE team=?1 AND state='open' ORDER BY seq LIMIT 1";
    pub const UNREPORTED: &str = "SELECT n.id,n.generation,e.member,COALESCE(o.at_ms,e.created_ms) FROM nodes n JOIN effects e ON e.team=n.team AND e.node=n.id AND e.generation=n.generation AND e.state='submitted' LEFT JOIN effect_outcomes o ON o.team=e.team AND o.request_id=e.request_id WHERE n.team=?1 AND n.node_state='in_progress' AND n.id>?2 AND NOT EXISTS(SELECT 1 FROM timer_firings f WHERE f.team=n.team AND f.kind='no_report' AND f.node=n.id AND f.generation=n.generation AND f.item=0) ORDER BY n.id LIMIT ?3";
    pub const UNSTARTED: &str = "SELECT n.id,n.generation,e.member,o.at_ms FROM nodes n JOIN effects e ON e.team=n.team AND e.node=n.id AND e.generation=n.generation AND e.state='submitted' JOIN effect_outcomes o ON o.team=e.team AND o.request_id=e.request_id WHERE n.team=?1 AND n.node_state='pending' AND n.id>?2 AND NOT EXISTS(SELECT 1 FROM timer_firings f WHERE f.team=n.team AND f.kind='not_started' AND f.node=n.id AND f.generation=n.generation AND f.item=0) ORDER BY n.id LIMIT ?3";
    pub const UNDECIDED: &str = "SELECT i.seq,i.node,i.generation,i.created_ms,EXISTS(SELECT 1 FROM timer_firings f WHERE f.team=i.team AND f.kind='remind_lead' AND f.node=i.node AND f.generation=i.generation AND f.item=i.seq),EXISTS(SELECT 1 FROM timer_firings f WHERE f.team=i.team AND f.kind='remind_person' AND f.node=i.node AND f.generation=i.generation AND f.item=i.seq) FROM inbox i WHERE i.team=?1 AND i.state='open' AND i.seq>?2 AND i.kind='report' AND NOT EXISTS(SELECT 1 FROM decisions d WHERE d.team=i.team AND d.node=i.node AND d.generation=i.generation) ORDER BY i.seq LIMIT ?3";
    pub const FIRED: &str = "SELECT EXISTS(SELECT 1 FROM timer_firings WHERE team=?1 AND kind=?2 AND node=?3 AND generation=?4 AND item=?5)";
    pub const KEYED_TEAMS: &str =
        "SELECT team FROM key_clear_teams WHERE pending=1 AND team>?1 ORDER BY team LIMIT ?2";
    pub const KEYED: &str = "SELECT i.id,i.task_project,i.task_uuid FROM node_identity i WHERE i.team=?1 AND NOT EXISTS(SELECT 1 FROM task_observations o WHERE o.team=i.team AND o.node=i.id AND o.released) AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=i.team AND k.node=i.id)";
    pub const KEYED_PAGE: &str = "SELECT i.id,i.task_project,i.task_uuid FROM node_identity i WHERE i.team=?1 AND NOT EXISTS(SELECT 1 FROM task_observations o WHERE o.team=i.team AND o.node=i.id AND o.released) AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=i.team AND k.node=i.id) AND i.id>?2 ORDER BY i.id LIMIT ?3";
    pub const SETTLE: &str = "UPDATE key_clear_teams SET pending=0 WHERE team=?1 AND pending=1 AND NOT EXISTS(SELECT 1 FROM node_identity i WHERE i.team=?1 AND NOT EXISTS(SELECT 1 FROM task_observations o WHERE o.team=i.team AND o.node=i.id AND o.released) AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=i.team AND k.node=i.id) LIMIT 1)";
    pub const LATE: &str =
        "SELECT card FROM late_sessions WHERE team=?1 AND member=?2 AND launch=?3";
    pub const WAITING: &str =
        "SELECT EXISTS(SELECT 1 FROM inbox WHERE team=?1 AND state='open' AND seq>?2)";
}
pub(crate) use sql::KEYED;

impl Store {
    fn read<T: Default>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.active()?;
        self.db.as_ref().map_or(Ok(T::default()), f)
    }
    /// The open lead inbox: newest and oldest item and a count bounded at 201.
    pub fn inbox_view(&self, team: &str) -> Result<Option<InboxView>> {
        self.read(|db| {
            let newest = db.query_row(sql::NEWEST, [team], |r| r.get(0));
            let Some(newest) = newest.optional().map_err(storage)? else {
                return Ok(None);
            };
            let open = db
                .query_row(sql::OPEN, [team], |r| r.get(0))
                .map_err(storage)?;
            let oldest = db
                .query_row(sql::OLDEST, [team], item)
                .optional()
                .map_err(storage)?;
            Ok(Some(InboxView {
                open,
                newest,
                oldest,
            }))
        })
    }
    /// Delivered attempts of live in-progress nodes without a report item
    /// yet, in node order after `after`, with the delivery's time.
    pub fn unreported(&self, team: &str, after: u32, page: u32) -> Result<Vec<Attempt>> {
        let page = limit(page)?;
        self.read(|db| rows(db, sql::UNREPORTED, params![team, after, page], attempt))
    }
    /// Submitted hand-outs of live nodes whose task is still To do, with the
    /// stored outcome time; an outcome without a stored time is never listed.
    pub fn unstarted(&self, team: &str, after: u32, page: u32) -> Result<Vec<Attempt>> {
        let page = limit(page)?;
        self.read(|db| rows(db, sql::UNSTARTED, params![team, after, page], attempt))
    }
    /// Open report items with no decision in their generation, after `after`.
    pub fn undecided(&self, team: &str, after: u64, page: u32) -> Result<Vec<Waiting>> {
        let page = limit(page)?;
        self.read(|db| {
            rows(db, sql::UNDECIDED, params![team, after, page], |r| {
                Ok(Waiting {
                    item: item(r)?,
                    lead_reminded: r.get(4)?,
                    person_reminded: r.get(5)?,
                })
            })
        })
    }
    /// Whether one timer key already fired.
    pub fn timer_fired(
        &self,
        team: &str,
        kind: &str,
        node: u32,
        generation: u32,
        item: u64,
    ) -> Result<bool> {
        self.read(|db| {
            db.query_row(
                sql::FIRED,
                params![team, kind, node, generation, item],
                |r| r.get(0),
            )
            .map_err(storage)
        })
    }
    /// Terminal teams with a task key that has no final record, by ID.
    pub fn keyed_teams(&self, after: &str, page: u32) -> Result<Vec<String>> {
        let page = limit(page)?;
        self.read(|db| column(db, sql::KEYED_TEAMS, params![after, page]))
    }
    /// A terminal team's nodes whose task key is neither released nor finally
    /// recorded, in node order after `after`.
    pub fn keyed_tasks(&self, team: &str, after: u32, page: u32) -> Result<Vec<KeyedTask>> {
        let page = limit(page)?;
        self.read(|db| {
            rows(db, sql::KEYED_PAGE, params![team, after, page], |r| {
                let task = TaskRef {
                    project: r.get(1)?,
                    uuid: r.get(2)?,
                };
                Ok(KeyedTask {
                    node: r.get(0)?,
                    task,
                })
            })
        })
    }
    /// Leave the pending set once no node of the team lacks a final record.
    pub fn settle_keyed(&self, team: &str) -> Result<bool> {
        self.read(|db| settle(db, team))
    }
    /// The card a late launch's provenance opened, if it was recorded.
    pub fn late_session(&self, team: &str, member: &str, launch: &str) -> Result<Option<u64>> {
        self.read(|db| {
            db.query_row(sql::LATE, params![team, member, launch], |r| r.get(0))
                .optional()
                .map_err(storage)
        })
    }
}
/// A team leaves the pending set when no node lacks a final record.
pub(crate) fn settle(db: &Connection, team: &str) -> Result<bool> {
    Ok(db.execute(sql::SETTLE, [team]).map_err(storage)? == 1)
}
fn attempt(r: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
    Ok(Attempt {
        node: r.get(0)?,
        generation: r.get(1)?,
        member: r.get(2)?,
        outcome_ms: r.get(3)?,
    })
}

/// The doorbell's admission, read inside the immediate transaction held
/// across its spawn: the team still running, the lead's binding still the one
/// reserved, and an item beyond the previous mark still open.
pub(crate) fn admit_bell(db: &Connection, bell: &Bell) -> Result<()> {
    let team = super::rows::team(db, &bell.team)?.ok_or_else(|| refuse("The team is gone."))?;
    if team.status != zerus_team_plan::model::TeamStatus::Running {
        return Err(refuse("The team is not running."));
    }
    let lead = super::rows::member(db, &team.id, &team.lead)?;
    if team.lead != bell.member
        || !lead.is_some_and(|m| {
            m.state == MemberState::Active && m.session.as_ref() == Some(&bell.session)
        })
    {
        return Err(refuse("The lead's binding changed."));
    }
    let waiting: bool = db
        .query_row(sql::WAITING, params![team.id, bell.after], |r| r.get(0))
        .map_err(storage)?;
    if !waiting {
        return Err(refuse("No item beyond the last doorbell is open."));
    }
    Ok(())
}
fn refuse(text: &str) -> Problem {
    Problem::new(code::INVALID_STATE, text)
}

#[cfg(test)]
mod tests;
