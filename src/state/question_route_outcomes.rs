//! Read-only route outcomes from this computer's desktop service ledger.
//! One connection per listing, indexed exact-question lookups (not an Activity
//! window), and a bounded Activity window for inspection. No send or retry.
use super::*;
use rusqlite::{params, Connection, OpenFlags};
use zerus_client_core::desktop::question_route::{escalates, Record};

pub(crate) struct Reader {
    db: Connection,
}
impl Reader {
    pub(crate) fn at(state: &Path) -> Option<Self> {
        // v1 routes on Linux only. Never create a database or migrate from a
        // listing. A missing/older/unreadable service remains unavailable.
        if !cfg!(target_os = "linux") {
            return None;
        }
        let path = state.join("desktop/state.sqlite3");
        #[cfg(unix)]
        let identity = crate::sqlite_file::private(&path).ok()?;
        let db = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        db.busy_timeout(std::time::Duration::ZERO).ok()?;
        #[cfg(unix)]
        crate::sqlite_file::bind_private(&db, &path, identity).ok()?;
        Some(Self { db })
    }
    pub(crate) fn load() -> Option<Self> {
        Self::at(&root())
    }

    /// A constant-size watch input, changed only by a failed route. Service
    /// heartbeats and other Store traffic never invalidate the session list.
    pub(crate) fn revision(&self) -> Option<i64> {
        self.db
            .query_row(
                "SELECT revision FROM desktop_question_route_outcome_revision WHERE id=1",
                [],
                |r| r.get(0),
            )
            .ok()
    }

    pub(crate) fn escalated(&self, row: &Value, questions: Option<&[Value]>) -> bool {
        if row.get("parent").is_none() || row["phase"] != "input" {
            return false;
        }
        questions
            .into_iter()
            .flatten()
            .take(64)
            .any(|q| self.question(row, q).unwrap_or(false))
    }
    fn question(&self, row: &Value, q: &Value) -> Option<bool> {
        if !zerus_client_core::desktop::question_route::routable(q)
            || q["run_id"] != row["run_id"]
            || q["conversation_id"] != row["conversation_id"]
        {
            return Some(false);
        }
        let mut query = self.db.prepare_cached(
            "SELECT key,state,record FROM desktop_question_routes INDEXED BY desktop_question_routes_question
             WHERE child_session_id=?1 AND run_id=?2 AND question_id=?3
             AND json_extract(record,'$.identity.conversation_id')=?4
             AND json_extract(record,'$.identity.question_hash')=?5 LIMIT 2"
        ).ok()?;
        let rows = query
            .query_map(
                params![
                    string(row, "session_id"),
                    string(row, "run_id"),
                    string(q, "question_id"),
                    string(row, "conversation_id"),
                    string(q, "question_hash")
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .ok()?
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()?;
        let [(key, state, raw)] = rows.as_slice() else {
            return Some(false);
        };
        let record: Record = serde_json::from_str(raw).ok()?;
        Some(record.displayable() && record.identity.key() == *key && escalates(state))
    }
    pub(crate) fn inspect(&self, row: &mut Value) {
        if !string(row, "agent_id").is_empty() && row["agent_id"] != "main" {
            return;
        }
        let questions = row["pending_questions"].as_array();
        if self.escalated(row, questions.map(Vec::as_slice)) {
            row["human_request"] = json!(true);
        }
        if let Ok(routes) =
            crate::desktop::question_route::activity::routes(&self.db, string(row, "session_id"))
        {
            row["question_route_activity"] = json!(routes);
        }
    }
}
