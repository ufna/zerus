use super::*;

/// A ready accept may have been in flight at Stop or a board override.
/// Preserve that authorization as provenance; it grants no new effect.
pub(super) struct Permit {
    pub decision: u64,
    pub late: bool,
    pub generation: u32,
}
pub(super) fn permit(ctx: &Context<'_>, node: u32, decision: u64) -> Result<Permit> {
    let (generation, state): (u32, DecisionState) = ctx.db
        .query_row(
            "SELECT generation,state FROM decisions d WHERE seq=?1 AND team=?2 AND node=?3 AND kind='accept' AND (state=?4 OR (state=?5 AND stopped_ready=1)) AND NOT EXISTS(SELECT 1 FROM integration_facts f WHERE f.decision=d.seq)",
            params![decision, ctx.team.id, node, DecisionState::Ready, DecisionState::Settled],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| invalid("The named decision has no unrecorded integration authority."))?;
    Ok(Permit {
        decision,
        generation,
        late: state == DecisionState::Settled,
    })
}
pub(super) fn record(
    ctx: &Context<'_>,
    permit: &Permit,
    node: u32,
    commit: Option<&str>,
    conflicts: &[String],
) -> Result<()> {
    ctx.db.execute("INSERT INTO integration_facts(decision,team,node,actor,commit_id,conflicts,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![permit.decision,ctx.team.id,node,encode(ctx.actor)?,commit,encode(&conflicts)?,ctx.now]).map_err(storage)?;
    Ok(())
}
