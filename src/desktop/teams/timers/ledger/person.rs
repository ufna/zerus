//! Person inbox reminders retain a mark for the logical lead conversation.
//! The ordinary doorbell ledger still owns the exact run and send receipt.
use super::{Binding, Result, Rung, storage};
use rusqlite::{Connection, params};

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS timer_person_doorbells (
  team TEXT NOT NULL, member TEXT NOT NULL, machine TEXT NOT NULL,
  session TEXT NOT NULL, conversation TEXT NOT NULL, mark INTEGER NOT NULL,
  id TEXT NOT NULL UNIQUE, state TEXT NOT NULL CHECK(state IN ('reserved','possible','released')),
  PRIMARY KEY(team,member,machine,session,conversation,mark));
CREATE INDEX IF NOT EXISTS timer_person_doorbell_covered
  ON timer_person_doorbells(team,member,machine,session,conversation,mark)
  WHERE state IN ('reserved','possible');";

pub(in crate::desktop::teams::timers) const COVERED: &str = "
SELECT COALESCE(MAX(mark),0) FROM timer_person_doorbells
WHERE team=?1 AND member=?2 AND machine=?3 AND session=?4 AND conversation=?5
AND state IN ('reserved','possible')";
pub(in crate::desktop::teams::timers) const TRIED: &str = "
SELECT COALESCE(MAX(mark),0) FROM timer_person_doorbells
WHERE team=?1 AND member=?2 AND machine=?3 AND session=?4 AND conversation=?5";

pub(super) fn marks(db: &Connection, b: &Binding<'_>) -> Result<(u64, u64)> {
    let key = params![
        b.team,
        b.member,
        b.session.machine,
        b.session.session_id,
        b.conversation
    ];
    let covered = db
        .query_row(COVERED, key, |row| row.get(0))
        .map_err(storage)?;
    let tried = db
        .query_row(TRIED, key, |row| row.get(0))
        .map_err(storage)?;
    Ok((covered, tried))
}

pub(super) fn reserve(db: &Connection, b: &Binding<'_>, id: &str, mark: u64) -> Result<()> {
    db.execute(
        "INSERT INTO timer_person_doorbells(team,member,machine,session,conversation,mark,id,state)
         VALUES(?1,?2,?3,?4,?5,?6,?7,'reserved')
         ON CONFLICT(team,member,machine,session,conversation,mark)
         DO UPDATE SET id=excluded.id,state='reserved'",
        params![
            b.team,
            b.member,
            b.session.machine,
            b.session.session_id,
            b.conversation,
            mark,
            id
        ],
    )
    .map_err(storage)?;
    Ok(())
}

pub(super) fn settle(db: &Connection, id: &str, rung: &Rung) -> Result<()> {
    // A reservation covers its mark until there is evidence that it did
    // not reach the lead. Missing outcomes keep the already-durable mark.
    let possible = matches!(rung, Rung::Submitted | Rung::Uncertain(_));
    let state = if possible { "possible" } else { "released" };
    db.execute(
        "UPDATE timer_person_doorbells SET state=?2 WHERE id=?1 AND state='reserved'",
        params![id, state],
    )
    .map_err(storage)?;
    // A settled mark can replace smaller marks for the same binding. A
    // definite non-start replaces only the older attempts' cooldown evidence.
    db.execute(
        "DELETE FROM timer_person_doorbells WHERE rowid IN (
           SELECT rowid FROM (
             SELECT prior.rowid,prior.state FROM timer_person_doorbells settled
             JOIN timer_person_doorbells prior USING(team,member,machine,session,conversation)
             WHERE settled.id=?1 AND prior.mark<settled.mark ORDER BY prior.mark DESC LIMIT 64
           ) WHERE ?2 OR state='released')",
        params![id, possible],
    )
    .map_err(storage)?;
    Ok(())
}
