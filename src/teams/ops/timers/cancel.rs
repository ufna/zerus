//! A cancel decision carried out by the service. The service runs the
//! team-cancel child for each pending cancel and records its answer here,
//! against that exact decision of the node's current generation. In a live
//! team a cancelled answer does what the observation of a cancelled task
//! does, so that whichever arrives first, the second finds nothing to do.
use super::*;

pub(in crate::teams::ops) fn record_cancel(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::RecordCancel {
        node,
        decision,
        state,
        reason: why,
        attempt,
        attempt_id,
    } = ctx.op
    else {
        unreachable!()
    };
    let (node, decision) = (*node, *decision);
    text(attempt_id, 256)?;
    pending(ctx, node, decision)?;
    // A cancel acts on one attempt: an answer about another one never ends a
    // node whose task runs again. A refusal is not bound to an attempt.
    // The newest observation's pair; a conflict (a number with no current
    // attempt) matches no answer about an attempt.
    let held = super::attempts::stored(ctx, node)?;
    let conflict = held.0 > 0 && held.1.is_empty(); // see attempts::conflict
    if *state == CancelState::Cancelled && (conflict || held != (*attempt, attempt_id.clone())) {
        return Err(invalid(
            "The cancel acted on another attempt than the one the store holds.",
        ));
    }
    match state {
        CancelState::Cancelled if !why.is_empty() => {
            return Err(invalid("A cancelled task carries no reason."))
        }
        CancelState::Cancelled => (),
        CancelState::Refused => {
            reason(why)?;
            text(why, 4096)?;
        }
    }
    ctx.db.execute("INSERT INTO cancel_outcomes(decision,team,node,generation,state,reason,at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![decision,ctx.team.id,node,ctx.generation(node)?,encode(state)?.trim_matches('"'),why,ctx.now]).map_err(storage)?;
    let live = matches!(ctx.team.status, TeamStatus::Running | TeamStatus::Paused);
    match (state, live) {
        (CancelState::Cancelled, true) => cancelled(ctx, node)?,
        (CancelState::Refused, true) => {
            ctx.settle(node)?;
            let body = format!("The cancel of task {node} was refused: {why}");
            ctx.reply.item = Some(ctx.inbox(node, &format!("cancel_refused:{decision}"), &body)?);
        }
        // After a stop or an archive the decision settles and nothing else
        // of the node changes; its key is cleared next.
        (_, false) => ctx.settle(node)?,
    }
    store::timers::settle(ctx.db, &ctx.team.id)?;
    ctx.reply.node = Some(node);
    Ok(())
}
/// The exact cancel decision, still Pending for the node's current attempt.
fn pending(ctx: &Context<'_>, node: u32, decision: u64) -> Result<()> {
    let row: Option<(u32, String, String, u32)> = ctx
        .db
        .query_row(
            "SELECT node,kind,state,generation FROM decisions WHERE seq=?1 AND team=?2",
            params![decision, ctx.team.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(storage)?;
    let current = ctx.generation(node)?;
    match row {
        Some((n, kind, state, generation))
            if n == node
                && kind == "cancel"
                && state == DecisionState::Pending.as_str()
                && generation == current =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "The cancel decision is no longer pending for the current attempt.",
        )),
    }
}
/// What a cancelled observation does to the node, without its task revision.
fn cancelled(ctx: &Context<'_>, node: u32) -> Result<()> {
    let table = ctx.node_table(node)?;
    ctx.db
        .execute(
            &format!("UPDATE {table} SET task_cancelled=1 WHERE team=?1 AND id=?2"),
            params![ctx.team.id, node],
        )
        .map_err(storage)?;
    queue::finish_observation(ctx, node, true)
}
