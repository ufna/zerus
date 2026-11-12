use super::*;

pub(super) const HOLDER_SQL: &str = "SELECT e.member,e.generation,e.state FROM nodes n JOIN effects e ON e.team=n.team AND e.node=n.id AND e.generation=n.generation WHERE n.team=?1 AND n.id=?2 ORDER BY e.seq DESC LIMIT 1";
/// The states of a newest effect that hold nothing: only reserved, or
/// refused, or superseded. The page's row facts use the same list.
pub(crate) const NOT_HOLDING: &str = "('reserved','refused','superseded')";

/// The one rule for who holds a live node's attempt: the newest effect of
/// its current generation, unless that effect holds nothing. Choosing the
/// latest row before filtering prevents a refusal from falling back to an
/// earlier holder. No history walk.
pub(crate) fn holder_of(db: &Connection, team: &str, node: u32) -> Result<Option<AttemptHolder>> {
    let row: Option<(String, u32, String)> = db
        .query_row(HOLDER_SQL, rusqlite::params![team, node], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()
        .map_err(storage)?;
    Ok(row.and_then(|(member, generation, state)| {
        if NOT_HOLDING.contains(&format!("'{state}'")) {
            None
        } else {
            Some(AttemptHolder { member, generation })
        }
    }))
}

impl Store {
    /// The attempt holder of a live node: [`holder_of`].
    pub fn attempt_holder(&self, team: &str, node: u32) -> Result<Option<AttemptHolder>> {
        self.active()?;
        let Some(db) = &self.db else { return Ok(None) };
        holder_of(db, team, node)
    }
}
