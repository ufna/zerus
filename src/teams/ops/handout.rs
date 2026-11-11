use super::*;

pub(super) struct Effect {
    pub node: u32,
    pub member: String,
    pub generation: u32,
    pub kind: String,
    pub state: String,
    pub outcome: Option<String>,
}
pub(super) fn holder(ctx: &Context<'_>, node: u32) -> Result<Option<String>> {
    ctx.db.query_row("SELECT member FROM effects WHERE team=?1 AND node=?2 AND generation=?3 ORDER BY seq DESC LIMIT 1",params![ctx.team.id,node,ctx.generation(node)?],|r|r.get(0)).optional().map_err(storage)
}
pub(super) fn intent(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::HandoutIntent {
        node,
        member,
        effect_request_id,
    } = ctx.op
    else {
        unreachable!()
    };
    ctx.undecided(*node)?;
    let n = ctx.live_node(*node)?;
    require_ready(ctx, *node)?;
    if n.state != NodeState::Pending || n.hold || n.assignee.as_ref().is_some_and(|id| id != member)
    {
        return Err(invalid("Task is not ready for a hand-out."));
    }
    eligible(ctx, &n, member)?;
    reserve(ctx, *node, member, effect_request_id, "handout")
}
fn eligible(ctx: &Context<'_>, node: &Node, id: &str) -> Result<()> {
    if ctx.team.status != TeamStatus::Running {
        return Err(invalid("Team is not running."));
    }
    let m = ctx.member(id)?;
    if m.state != MemberState::Active
        || m.session.is_none()
        || m.id == ctx.team.lead
        || m.role != node.role
        || (node.kind == NodeKind::Stage && node.implementer.as_deref() == Some(id))
    {
        return Err(invalid("Member cannot take this task."));
    }
    Ok(())
}
fn reserve(ctx: &mut Context<'_>, node: u32, member: &str, id: &str, kind: &str) -> Result<()> {
    uuid(id)?;
    let busy:bool=ctx.db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND (node=?2 OR member=?3) AND state IN {OPEN_EFFECT_STATES})"),params![ctx.team.id,node,member],|r|r.get(0)).map_err(storage)?;
    if busy {
        return Err(invalid("Node or member has an unresolved effect."));
    }
    ctx.db.execute("INSERT INTO effects(team,node,member,generation,request_id,actor,kind,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![ctx.team.id,node,member,ctx.generation(node)?,id,encode(ctx.actor)?,kind,ctx.now]).map_err(storage)?;
    ctx.reply.node = Some(node);
    ctx.reply.item = Some(ctx.db.last_insert_rowid() as u64);
    Ok(())
}
pub(super) fn reopen_intent(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::ReopenIntent {
        node,
        member,
        effect_request_id,
    } = ctx.op
    else {
        unreachable!()
    };
    let n = ctx.live_node(*node)?;
    if !has_attempt(ctx, *node)? {
        return Err(invalid("Reopen requires a recorded earlier attempt."));
    }
    let (seq, kind, state) = ctx
        .pending(*node)?
        .ok_or_else(|| invalid("No reopen decision is pending."))?;
    if !matches!(kind.as_str(), "return" | "reassign") || state != DecisionState::Pending {
        return Err(invalid("Reopen was already reserved or is not requested."));
    }
    let target = if kind == "reassign" {
        n.assignee.clone()
    } else {
        holder(ctx, *node)?
    };
    if target.as_deref() != Some(member) {
        return Err(invalid(
            "Reopen recipient differs from the decision's holder.",
        ));
    }
    eligible(ctx, &n, member)?;
    let busy:bool=ctx.db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND member=?2 AND node<>?3 AND state IN {OPEN_EFFECT_STATES})"),params![ctx.team.id,member,node],|r|r.get(0)).map_err(storage)?;
    if busy {
        return Err(invalid("Reopen recipient is busy."));
    }
    ctx.db.execute(&format!("UPDATE effects SET state='superseded' WHERE team=?1 AND node=?2 AND state IN {OPEN_EFFECT_STATES}"),params![ctx.team.id,node]).map_err(storage)?;
    let mut revised = n;
    ctx.advance(*node, &mut revised)?;
    ctx.db
        .execute(
            "UPDATE decisions SET state=?3,generation=?2 WHERE seq=?1",
            params![seq, ctx.generation(*node)?, DecisionState::EffectReserved],
        )
        .map_err(storage)?;
    reserve(ctx, *node, member, effect_request_id, "reopen")
}
pub(super) fn outcome(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::HandoutOutcome {
        effect_request_id,
        outcome,
    } = ctx.op
    else {
        unreachable!()
    };
    record_outcome(ctx, effect_request_id, outcome, "handout")
}
pub(super) fn reopen_outcome(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::ReopenOutcome {
        effect_request_id,
        outcome,
    } = ctx.op
    else {
        unreachable!()
    };
    record_outcome(ctx, effect_request_id, outcome, "reopen")
}
fn record_outcome(
    ctx: &mut Context<'_>,
    id: &str,
    outcome: &HandoutOutcome,
    kind: &str,
) -> Result<()> {
    let e = read_effect(ctx, id)?;
    if e.kind != kind || e.outcome.is_some() {
        return Err(invalid(
            "Effect kind differs or its outcome was already recorded.",
        ));
    }
    let (state, why) = outcome_state(outcome)?;
    let current = e.generation == ctx.generation(e.node)?;
    let open: bool = ctx
        .db
        .query_row(
            &format!(
                "SELECT state IN {OPEN_EFFECT_STATES} FROM effects WHERE team=?1 AND request_id=?2"
            ),
            params![ctx.team.id, id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    ctx.db
        .execute(
            "UPDATE effects SET outcome=?3 WHERE team=?1 AND request_id=?2",
            params![ctx.team.id, id, encode(outcome)?],
        )
        .map_err(storage)?;
    // A late receipt is still a fact. It never resurrects a superseded,
    // reported or observed-done slot and never settles a newer decision.
    if current && e.state != "superseded" && state == "submitted" {
        record_implementer(ctx, e.node, &e.member)?;
    }
    if !current || !open {
        return Ok(());
    }
    ctx.db
        .execute(
            "UPDATE effects SET state=?3 WHERE team=?1 AND request_id=?2",
            params![ctx.team.id, id, state],
        )
        .map_err(storage)?;
    if let Some(why) = why {
        ctx.reply.item = Some(ctx.inbox(e.node, "delivery_uncertain", why)?);
    }
    if kind == "reopen" && ctx.reopening(e.node)? {
        ctx.settle(e.node)?;
        queue::set_hold(ctx, e.node, state != "submitted")?;
        if state == "submitted" {
            ctx.close_inbox(e.node)?;
        }
    }
    Ok(())
}
fn outcome_state(outcome: &HandoutOutcome) -> Result<(&'static str, Option<&str>)> {
    Ok(match outcome {
        HandoutOutcome::Submitted { receipt } => {
            reason(receipt)?;
            ("submitted", None)
        }
        HandoutOutcome::Refused { reason: r } => {
            reason(r)?;
            ("refused", None)
        }
        HandoutOutcome::CuttingOver { reason: r } => {
            reason(r)?;
            ("cutting_over", Some(r.as_str()))
        }
        HandoutOutcome::Uncertain { reason: r } => {
            reason(r)?;
            ("uncertain", Some(r.as_str()))
        }
    })
}
fn read_effect(ctx: &Context<'_>, id: &str) -> Result<Effect> {
    ctx.db.query_row("SELECT node,member,generation,kind,state,outcome FROM effects WHERE team=?1 AND request_id=?2",params![ctx.team.id,id],|r|Ok(Effect{node:r.get(0)?,member:r.get(1)?,generation:r.get(2)?,kind:r.get(3)?,state:r.get(4)?,outcome:r.get(5)?})).optional().map_err(storage)?.ok_or_else(missing)
}
fn record_implementer(ctx: &Context<'_>, node: u32, member: &str) -> Result<()> {
    let mut n = ctx.node(node)?;
    // A stage keeps the implementer copied from the task it checks. Its own
    // reviewer/tester lives only in the effect row and never replaces that ID.
    if n.kind == NodeKind::Task {
        n.implementer = Some(member.into());
        let table = ctx.node_table(node)?;
        ctx.db
            .execute(
                &format!("UPDATE {table} SET data=?3 WHERE team=?1 AND id=?2"),
                params![ctx.team.id, node, rows::node_payload(&n)?],
            )
            .map_err(storage)?;
    }
    Ok(())
}
pub(super) fn report(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::RecordReport {
        node,
        generation,
        member,
        text: body,
        commit,
    } = ctx.op
    else {
        unreachable!()
    };
    ctx.active()?;
    let mut n = ctx.live_node(*node)?;
    if n.state != NodeState::Reported
        || *generation != ctx.generation(*node)?
        || holder(ctx, *node)?.as_deref() != Some(member)
    {
        return Err(invalid(
            "Report does not belong to the current reviewed attempt's holder.",
        ));
    }
    let recorded:bool=ctx.db.query_row("SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND node=?2 AND generation=?3 AND state NOT IN ('refused','superseded'))",params![ctx.team.id,node,generation],|r|r.get(0)).map_err(storage)?;
    if !recorded {
        return Err(invalid("No current effect is available to report."));
    }
    reason(body)?;
    text(commit, 256)?;
    ctx.db.execute("INSERT INTO reports(team,node,generation,actor,text,commit_id) VALUES(?1,?2,?3,?4,?5,?6)",params![ctx.team.id,node,generation,encode(ctx.actor)?,body,commit]).map_err(storage)?;
    // A trusted report proves delivery even when its submission outcome has
    // not arrived. Freeze that implementer before choosing any stage staff.
    if n.kind != NodeKind::Stage {
        n.implementer = Some(member.clone());
        ctx.db
            .execute(
                "UPDATE nodes SET data=?3 WHERE team=?1 AND id=?2",
                params![ctx.team.id, node, rows::node_payload(&n)?],
            )
            .map_err(storage)?;
    }
    ctx.db.execute(&format!("UPDATE effects SET state='reported' WHERE team=?1 AND node=?2 AND generation=?3 AND state IN {OPEN_EFFECT_STATES}"),params![ctx.team.id,node,generation]).map_err(storage)?;
    if ctx.reopening(*node)? {
        ctx.settle(*node)?;
        queue::set_hold(ctx, *node, false)?;
    }
    pipeline::reported(ctx, &n, body)
}

fn require_ready(ctx: &Context<'_>, node: u32) -> Result<()> {
    let Readiness { phases, nodes } = readiness::load(ctx.db, &ctx.team.id)?;
    let view = zerus_team_plan::ready::PlanView {
        phases: &phases,
        nodes: &nodes,
        members: &[],
        lead: &ctx.team.lead,
        integration: ctx.team.settings.integration,
    };
    // ready() does not choose members. The service owns the external native
    // free-member proof; eligible() and reserve() retain the local slot checks.
    if !zerus_team_plan::ready::ready(&view).contains(&node) {
        return Err(invalid(
            "The plan kernel does not consider this node ready.",
        ));
    }
    Ok(())
}
pub(super) fn kernel_nodes(ctx: &Context<'_>) -> Result<Vec<Node>> {
    readiness::nodes(ctx.db, &ctx.team.id)
}

pub(super) fn has_attempt(ctx: &Context<'_>, node: u32) -> Result<bool> {
    ctx.db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND node=?2 AND generation<=?3 AND state<>'refused')",
            params![ctx.team.id, node, ctx.generation(node)?],
            |r| r.get(0),
        )
        .map_err(storage)
}
