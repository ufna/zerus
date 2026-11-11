use super::*;

// Settings admit at most 200 members, plus the separate lead roster row.
pub(super) const MAX_ROSTER: usize = 201;
pub(super) const MEMBER_SESSIONS_SQL: &str =
    "SELECT id,machine,session_id FROM members WHERE team=?1 ORDER BY id LIMIT ?2";

impl Store {
    /// One primary-key range read, including unbound rows in the bound so a
    /// malformed roster cannot turn this into a scan of arbitrary size.
    pub fn member_sessions(&self, team: &str) -> Result<Vec<MemberSession>> {
        self.active()?;
        let Some(db) = &self.db else {
            return Ok(Vec::new());
        };
        let mut statement = db.prepare(MEMBER_SESSIONS_SQL).map_err(storage)?;
        let roster = statement
            .query_map(rusqlite::params![team, MAX_ROSTER + 1], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(storage)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?;
        if roster.len() > MAX_ROSTER {
            return Err(Problem::new(
                code::OVER_CAP,
                "The member roster exceeds its bound.",
            ));
        }
        Ok(roster
            .into_iter()
            .filter_map(|(member, machine, session)| {
                machine
                    .zip(session)
                    .map(|(machine, session_id)| MemberSession {
                        member,
                        machine,
                        session_id,
                    })
            })
            .collect())
    }
}
