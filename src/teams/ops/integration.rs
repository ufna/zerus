use super::*;

/// A ready accept may already have been in flight when Stop settled it.
/// Preserve that authorization as provenance; it grants no new effect.
pub(super) struct Permit {
    pub decision: u64,
    pub late: bool,
}
pub(super) fn permit(ctx: &Context<'_>, node: u32) -> Result<Permit> {
    if let Some((decision, kind, state)) = ctx.pending(node)? {
        if kind == "accept" && state == DecisionState::Ready {
            return unused(
                ctx,
                Permit {
                    decision,
                    late: false,
                },
            );
        }
    }
    if !matches!(ctx.team.status, TeamStatus::Stopped | TeamStatus::Archived) {
        return Err(invalid("Acceptance has not authorized integration."));
    }
    let decision=ctx.db.query_row("SELECT seq FROM decisions WHERE team=?1 AND node=?2 AND generation=?3 AND kind='accept' AND state=?4 AND stopped_ready=1 ORDER BY seq DESC LIMIT 1",params![ctx.team.id,node,ctx.generation(node)?,DecisionState::Settled],|r|r.get(0)).optional().map_err(storage)?.ok_or_else(||invalid("No ready acceptance was settled by Stop."))?;
    unused(
        ctx,
        Permit {
            decision,
            late: true,
        },
    )
}
fn unused(ctx: &Context<'_>, permit: Permit) -> Result<Permit> {
    let recorded: bool = ctx
        .db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM integration_facts WHERE decision=?1)",
            [permit.decision],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if recorded {
        return Err(invalid("Integration outcome is already recorded."));
    }
    Ok(permit)
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
