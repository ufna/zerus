//! Indexed reads for the person's questions and notes on the existing rows.
use super::*;

pub(crate) const NOTE: &str = "person_note_to_lead";
pub(crate) const REPLY_NOTICE: &str = "lead_reply_to_person";
pub(crate) const ACK_NOTICE: &str = "lead_note_ack";
pub(crate) fn passive(kind: &str) -> bool {
    matches!(kind, NOTE | REPLY_NOTICE | ACK_NOTICE)
}

macro_rules! contact {
    ($prefix:literal, $suffix:literal) => {
        concat!($prefix, "node=0 AND generation=0 AND (kind GLOB 'person_question:*' OR kind GLOB 'person_note_link:*')", $suffix)
    };
}
const RECENT: &str = contact!(
    "SELECT EXISTS(SELECT 1 FROM inbox INDEXED BY person_contact_time WHERE team=?1 AND ",
    " AND created_ms>=?2)"
);
const COUNT_OPEN: &str = contact!(
    "SELECT COUNT(*) FROM (SELECT 1 FROM inbox INDEXED BY person_contact_open WHERE team=?1 AND state='open' AND ",
    " AND kind GLOB ?2 LIMIT ?3)"
);
const NEWEST: &str = contact!(
    "SELECT seq FROM inbox INDEXED BY person_contact_open WHERE team=?1 AND state='open' AND ",
    " ORDER BY seq DESC LIMIT 1"
);
const SCHEMA: &str = contact!(
    "CREATE INDEX IF NOT EXISTS person_contact_time ON inbox(team,created_ms) WHERE ",
    "; CREATE INDEX IF NOT EXISTS person_contact_open ON inbox(team,seq) WHERE state='open' AND "
);
pub(super) const NOTICE_INDEX: &str = "CREATE INDEX IF NOT EXISTS person_lead_notices ON cards(team,seq) WHERE state='open' AND kind IN ('lead_reply_to_person','lead_note_ack');";
const WAITING_INDEX: &str = "CREATE INDEX IF NOT EXISTS person_owner_waiting ON cards(seq) WHERE state='open' AND kind NOT IN ('person_note_to_lead','lead_note_ack');";

pub(super) fn extend(db: &Connection) -> Result<()> {
    let complete: bool = db.query_row(
        "SELECT count(*)=4 FROM sqlite_schema WHERE type='index' AND name IN ('person_contact_time','person_contact_open','person_lead_notices','person_owner_waiting')",
        [], |r| r.get(0),
    ).map_err(storage)?;
    if complete {
        return Ok(());
    }
    db.execute_batch("SAVEPOINT person_contact_schema")
        .map_err(begin_error)?;
    let result = timers::fence(db).and_then(|()| {
        db.execute_batch(&format!(
            "{SCHEMA}{};{NOTICE_INDEX}{WAITING_INDEX}",
            contact!("", "")
        ))
        .map_err(storage)
    });
    if result.is_err() {
        db.execute_batch("ROLLBACK TO person_contact_schema")
            .map_err(storage)?;
    }
    db.execute_batch("RELEASE person_contact_schema")
        .map_err(storage)?;
    result
}

pub(in crate::teams) fn sent_since(db: &Connection, team: &str, since: i64) -> Result<bool> {
    db.query_row(RECENT, params![team, since], |r| r.get(0))
        .map_err(storage)
}

pub(in crate::teams) fn open_count(
    db: &Connection,
    team: &str,
    pattern: &str,
    cap: u32,
) -> Result<u32> {
    db.query_row(COUNT_OPEN, params![team, pattern, cap], |r| r.get(0))
        .map_err(storage)
}

pub(super) fn newest(db: &Connection, team: &str) -> Result<u64> {
    Ok(db
        .query_row(NEWEST, [team], |r| r.get(0))
        .optional()
        .map_err(storage)?
        .unwrap_or(0))
}

pub(in crate::teams) fn unread(db: &Connection, team: &str) -> Result<u32> {
    db.query_row(
        "SELECT count(*) FROM (SELECT 1 FROM cards INDEXED BY person_lead_notices WHERE team=?1
         AND state='open' AND kind IN ('lead_reply_to_person','lead_note_ack') LIMIT 201)",
        [team],
        |r| r.get(0),
    )
    .map_err(storage)
}
