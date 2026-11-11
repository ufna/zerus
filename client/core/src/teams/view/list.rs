//! The team list and what waits for the person, projected from one page of
//! package 5's replies.
use super::*;

/// The teams of the reply's `rows`, at most [`LIST_KEPT`].
pub fn list(reply: &Value) -> TeamList {
    let rows = array(reply, "rows");
    let teams = rows
        .iter()
        .take(LIST_KEPT)
        .map(|row| TeamSummary {
            id: name(row, "id"),
            project: name(row, "project"),
            name: name(row, "name"),
            status: name(row, "status"),
            lead: name(row, "lead"),
            home: name(row, "home"),
            revision: count(row, "revision"),
            lead_session: lead_session(row),
        })
        .collect();
    TeamList {
        teams,
        truncated: rows.len() > LIST_KEPT,
        next: next_cursor(reply),
    }
}

fn waiting_item(row: &Value) -> Option<WaitingItem> {
    let team = name(row, "team");
    let card = row.get("seq").and_then(Value::as_u64)?;
    Some(WaitingItem {
        id: format!("{team}:{card}"),
        team,
        card,
        kind: name(row, "kind"),
        node: row.get("node").and_then(Value::as_u64),
        text: text(row, "text", TEXT_CHARS),
        state: name(row, "state"),
        by: row
            .get("actor")
            .map(|actor| name(actor, "person"))
            .unwrap_or_default(),
        created_ms: row.get("created_ms").and_then(Value::as_i64).unwrap_or(0),
    })
}

/// The reply's open owner cards, at most [`PAGE`]. The read is contracted to
/// open cards only, so nothing is filtered here.
pub fn waiting(reply: &Value) -> WaitingView {
    let rows = array(reply, "rows");
    WaitingView {
        items: rows.iter().take(PAGE).filter_map(waiting_item).collect(),
        next: next_cursor(reply),
    }
}
