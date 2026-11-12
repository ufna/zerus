use super::*;

pub(super) const HOLDER_SQL: &str = "SELECT e.member,e.generation,e.state FROM nodes n JOIN effects e ON e.team=n.team AND e.node=n.id AND e.generation=n.generation WHERE n.team=?1 AND n.id=?2 ORDER BY e.seq DESC LIMIT 1";
/// The states of a newest effect that hold nothing: only reserved, or
/// refused, or superseded. The page's row facts use the same list.
pub(crate) const NOT_HOLDING: &str = "('reserved','refused','superseded')";
/// The states of a newest effect whose member received nothing: a known
/// refusal, or a supersession. They differ from [`NOT_HOLDING`] in exactly
/// one state, reserved: a member's own report or question proves a reserved
/// delivery before its outcome is recorded, so it admits its member, who
/// does not yet hold the attempt. A known outcome is never such a proof.
pub(crate) const NOT_RECEIVED: &str = "('refused','superseded')";

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

/// Hands a Return that has nobody to go to back to the lead, as a refused
/// reopen is handed back: the decision settles, the node is held, and the
/// lead gets one inbox item of its own kind, `return_without_holder:<seq>`.
/// Used where a Return loses its holder: the step to layout 10 for Returns
/// an earlier layout admitted, and a refused outcome. Returns the item.
pub(crate) fn hand_back_return(
    db: &Connection,
    team: &str,
    node: u32,
    seq: u64,
    actor: &str,
    now: i64,
) -> Result<u64> {
    rows::settle_decisions(db, team, Some(node), actor, false)?;
    db.execute(
        "UPDATE nodes SET hold=1 WHERE team=?1 AND id=?2",
        rusqlite::params![team, node],
    )
    .map_err(storage)?;
    let text = format!(
        "The return of task {node} had nobody to go to: no member holds its attempt (its hand-out was only reserved, refused or superseded). The task is held; decide again."
    );
    db.execute(
        "INSERT INTO inbox(team,kind,node,generation,actor,text,created_ms) SELECT ?1,?2,?3,generation,?4,?5,?6 FROM nodes WHERE team=?1 AND id=?3",
        rusqlite::params![team, format!("return_without_holder:{seq}"), node, actor, text, now],
    )
    .map_err(storage)?;
    Ok(db.last_insert_rowid() as u64)
}

impl Store {
    /// The attempt holder of a live node: [`holder_of`].
    pub fn attempt_holder(&self, team: &str, node: u32) -> Result<Option<AttemptHolder>> {
        self.active()?;
        let Some(db) = &self.db else { return Ok(None) };
        holder_of(db, team, node)
    }
}
