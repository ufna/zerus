//! A cancel decision carried out by the service. The service runs the
//! team-cancel child for each pending cancel and records its answer here,
//! against that exact decision of the node's current generation. In a live
//! team a task's end is recorded by its observation alone: a cancelled
//! answer for a node whose task is bound is refused and writes nothing, and
//! the service observes the task instead; that observation ends the node
//! and settles the decision, or takes a person's revived task over. A
//! cancelled answer ends a node without a task, and after a stop or an
//! archive it settles the decision. A released answer says the key is
//! already gone from the task: in every team state it is recorded, and the
//! node's binding goes with it. A refused answer goes back to the lead.
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
    let live = matches!(ctx.team.status, TeamStatus::Running | TeamStatus::Paused);
    if *state == CancelState::Cancelled && live && bound(ctx, node)? {
        return Err(invalid(
            "The task is observed instead: in a live team a task's end is recorded by its observation.",
        ));
    }
    // A cancel acts on one attempt: an answer about another one never ends a
    // node whose task runs again. A refusal is not bound to an attempt.
    // The newest observation's pair, compared by equality, an empty ID
    // included. While that observation is a conflict no cancelled answer is
    // applied, whatever its pair; a release reports that the key is gone,
    // not who executed an attempt, so the conflict does not hold it back.
    let held = super::attempts::stored(ctx, node)?;
    let conflict = super::attempts::in_conflict(ctx, node)?;
    let ends = matches!(state, CancelState::Cancelled | CancelState::Released);
    let gated = *state == CancelState::Cancelled && conflict;
    if ends && (gated || held != (*attempt, attempt_id.clone())) {
        return Err(invalid(
            "The cancel acted on another attempt than the one the store holds.",
        ));
    }
    match state {
        CancelState::Cancelled | CancelState::Released if !why.is_empty() => {
            return Err(invalid("A cancelled task carries no reason."))
        }
        CancelState::Cancelled | CancelState::Released => (),
        CancelState::Refused => {
            reason(why)?;
            text(why, 4096)?;
        }
    }
    ctx.db.execute("INSERT INTO cancel_outcomes(decision,team,node,generation,state,reason,at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![decision,ctx.team.id,node,ctx.generation(node)?,encode(state)?.trim_matches('"'),why,ctx.now]).map_err(storage)?;
    match (state, live) {
        // A node without a task (a bound one was refused above), or a
        // released task: the node ends and its binding goes with the record.
        (CancelState::Cancelled | CancelState::Released, true) => cancelled(ctx, node)?,
        (CancelState::Refused, true) => {
            ctx.settle(node)?;
            let body = format!("The cancel of task {node} was refused: {why}");
            ctx.reply.item = Some(ctx.inbox(node, &format!("cancel_refused:{decision}"), &body)?);
        }
        // After a stop or an archive the decision settles and nothing else
        // of the node changes; its key is cleared next. The stop or the
        // archive already released every binding of the team.
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
/// Whether the node's task is bound: the service observes it.
fn bound(ctx: &Context<'_>, node: u32) -> Result<bool> {
    ctx.db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM live_tasks WHERE team=?1 AND node=?2)",
            params![ctx.team.id, node],
            |r| r.get(0),
        )
        .map_err(storage)
}
/// What a cancelled observation does to the node, without its task
/// revision: first what the end does to the node's pipeline, through the
/// observation path's own function, then the end of the node.
fn cancelled(ctx: &mut Context<'_>, node: u32) -> Result<()> {
    let n = ctx.node(node)?;
    pipeline::task_ended(ctx, &n, true)?;
    let table = ctx.node_table(node)?;
    ctx.db
        .execute(
            &format!("UPDATE {table} SET task_cancelled=1 WHERE team=?1 AND id=?2"),
            params![ctx.team.id, node],
        )
        .map_err(storage)?;
    queue::finish_observation(ctx, node, true)
}
