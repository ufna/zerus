use super::*;

pub(super) const HOLDER_SQL: &str = "SELECT e.member,e.generation,e.state FROM nodes n JOIN effects e ON e.team=n.team AND e.node=n.id AND e.generation=n.generation WHERE n.team=?1 AND n.id=?2 ORDER BY e.seq DESC LIMIT 1";

impl Store {
    /// Latest effect of this live node's current generation, with no history
    /// walk. Choosing the latest row before filtering prevents a refusal from
    /// falling back to an earlier holder.
    pub fn attempt_holder(&self, team: &str, node: u32) -> Result<Option<AttemptHolder>> {
        self.active()?;
        let Some(db) = &self.db else { return Ok(None) };
        let row: Option<(String, u32, String)> = db
            .query_row(HOLDER_SQL, rusqlite::params![team, node], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
            .map_err(storage)?;
        Ok(row.and_then(|(member, generation, state)| {
            if matches!(state.as_str(), "reserved" | "refused" | "superseded") {
                None
            } else {
                Some(AttemptHolder { member, generation })
            }
        }))
    }
}
