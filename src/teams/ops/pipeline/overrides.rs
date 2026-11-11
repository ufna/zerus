use super::*;

pub(in crate::teams::ops) fn released_stage(ctx: &Context<'_>, n: &Node) -> Result<bool> {
    if n.kind != NodeKind::Stage {
        return Ok(false);
    }
    ctx.db.query_row("SELECT EXISTS(SELECT 1 FROM stage_tasks WHERE team=?1 AND node=?2 AND state IN ('superseded','refused','uncertain'))",params![ctx.team.id,n.id],|r|r.get(0)).map_err(storage)
}

pub(in crate::teams::ops) fn override_task(ctx: &Context<'_>, target: u32) -> Result<()> {
    let stage: Option<u32> = ctx
        .db
        .query_row(
            "SELECT stage FROM pipeline_runs WHERE team=?1 AND node=?2",
            params![ctx.team.id, target],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?
        .flatten();
    if let Some(stage) = stage {
        ctx.db.execute("UPDATE stage_tasks SET state='superseded' WHERE team=?1 AND node=?2 AND state IN ('wanted','reserved','created')",params![ctx.team.id,stage]).map_err(storage)?;
        retire_stage(ctx, stage)?;
    }
    state(ctx, target, "overridden", None)?;
    ctx.db.execute("UPDATE cards SET state='closed',decision='stale',decided_actor=?3 WHERE team=?1 AND node=?2 AND kind='pipeline_acceptance' AND state='open'",params![ctx.team.id,target,encode(ctx.actor)?]).map_err(storage)?;
    Ok(())
}
pub(in crate::teams::ops) fn observed_reopen(ctx: &Context<'_>, node: u32) -> Result<bool> {
    ctx.db.query_row("SELECT EXISTS(SELECT 1 FROM pipeline_runs WHERE team=?1 AND node=?2 AND generation=?3 AND state IN ('active','person','lead','accepted'))",params![ctx.team.id,node,ctx.generation(node)?],|r|r.get(0)).map_err(storage)
}
pub(in crate::teams::ops) fn observe_override(ctx: &mut Context<'_>, n: &Node) -> Result<()> {
    if n.kind == NodeKind::Stage {
        if let Some(target) = n.stage_of {
            if current_stage(ctx, n.id, target)? {
                ctx.db
                    .execute(
                        "UPDATE stage_tasks SET state='superseded' WHERE team=?1 AND node=?2",
                        params![ctx.team.id, n.id],
                    )
                    .map_err(storage)?;
                retire_stage(ctx, n.id)?;
                let target = ctx.node(target)?;
                to_lead(
                    ctx,
                    &target,
                    "The person overrode the pipeline's stage task.",
                )?;
            }
        }
        Ok(())
    } else {
        override_task(ctx, n.id)
    }
}
pub(in crate::teams::ops) fn before_decision(ctx: &Context<'_>, node: u32) -> Result<()> {
    let busy:bool=ctx.db.query_row("SELECT EXISTS(SELECT 1 FROM pipeline_runs WHERE team=?1 AND node=?2 AND state IN ('active','person'))",params![ctx.team.id,node],|r|r.get(0)).map_err(storage)?;
    if busy && !is_person(ctx.actor) {
        return Err(invalid(
            "The pipeline's stages or the person's decision are still pending.",
        ));
    }
    if busy {
        override_task(ctx, node)?;
    }
    Ok(())
}
pub(in crate::teams::ops) fn decide_card(
    ctx: &mut Context<'_>,
    node: u32,
    answer: &str,
) -> Result<bool> {
    let current:bool=ctx.db.query_row("SELECT EXISTS(SELECT 1 FROM pipeline_runs p JOIN nodes n ON n.team=p.team AND n.id=p.node WHERE p.team=?1 AND p.node=?2 AND p.state='person' AND p.generation=n.generation AND n.node_state='reported')",params![ctx.team.id,node],|r|r.get(0)).map_err(storage)?;
    if !current || ctx.pending(node)?.is_some() {
        return Ok(true);
    }
    ctx.active()?;
    let n = ctx.live_node(node)?;
    match answer {
        "approve" => {
            state(ctx, node, "accepted", None)?;
            decisions::accept_result(ctx, &n, Decider::Person, true)?;
        }
        "reject" => to_lead(ctx, &n, "The person declined the pipeline's acceptance.")?,
        _ => return Err(invalid("Pipeline acceptance must be approve or reject.")),
    }
    Ok(false)
}

pub(in crate::teams::ops) fn before_observation(
    ctx: &mut Context<'_>,
    n: &mut Node,
    o: &TaskObservation,
    live: bool,
) -> Result<()> {
    let reopened = live
        && matches!(ctx.team.status, TeamStatus::Running | TeamStatus::Paused)
        && n.kind != NodeKind::Stage
        && n.state == NodeState::Reported
        && !o.cancelled
        && matches!(
            o.stage,
            TaskStage::Todo | TaskStage::InProgress | TaskStage::Backlog
        )
        && observed_reopen(ctx, n.id)?
        && !ctx.reopening(n.id)?;
    if o.cancelled || o.stage == TaskStage::Done || reopened {
        observe_override(ctx, n)?;
    }
    if reopened {
        ctx.settle_override(n.id)?;
        ctx.advance(n.id, n)?;
        ctx.db.execute(&format!("UPDATE effects SET state='superseded' WHERE team=?1 AND node=?2 AND state IN {OPEN_EFFECT_STATES}"),params![ctx.team.id,n.id]).map_err(storage)?;
        queue::set_hold(ctx, n.id, false)?;
        ctx.event = Some("person_override");
    }
    Ok(())
}
