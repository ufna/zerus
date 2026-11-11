use super::*;

pub(super) fn needs_merge(ctx: &Context<'_>, n: &Node) -> bool {
    ctx.team.settings.integration == Integration::Branch && n.kind != NodeKind::Stage
}
pub(super) fn accept(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Accept { node } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    ctx.undecided(*node)?;
    let n = ctx.live_node(*node)?;
    if n.state != NodeState::Reported {
        return Err(invalid("Only a reported task can be accepted."));
    }
    let owner = needs_merge(ctx, &n) && !ctx.team.settings.delegations.merge_accepted;
    let seq = ctx.decision(
        *node,
        "accept",
        "",
        if owner {
            DecisionState::AwaitingOwner
        } else {
            DecisionState::Ready
        },
    )?;
    let decider = if is_person(ctx.actor) {
        Decider::Person
    } else {
        Decider::Lead
    };
    ctx.db
        .execute(
            "UPDATE nodes SET decided_by=?3 WHERE team=?1 AND id=?2",
            params![ctx.team.id, node, encode(&decider)?],
        )
        .map_err(storage)?;
    if owner {
        ctx.reply.item = Some(talk::card(
            ctx,
            "integration",
            "Approve integration of the accepted result.",
            Some(*node),
            Some(seq),
        )?);
    }
    Ok(())
}
pub(super) fn return_task(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Return { node, reason: why } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    ctx.undecided(*node)?;
    let n = ctx.live_node(*node)?;
    reason(why)?;
    if n.state != NodeState::Reported || handout::holder(ctx, *node)?.is_none() {
        return Err(invalid(
            "Only a reported task with a holder can be returned.",
        ));
    }
    ctx.decision(*node, "return", why, DecisionState::Pending)?;
    Ok(())
}
pub(super) fn reassign(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Reassign {
        node,
        member,
        reason: why,
    } = ctx.op
    else {
        unreachable!()
    };
    ctx.active()?;
    ctx.undecided(*node)?;
    let mut n = ctx.live_node(*node)?;
    let m = ctx.member(member)?;
    reason(why)?;
    if m.state != MemberState::Active
        || m.session.is_none()
        || m.id == ctx.team.lead
        || m.role != n.role
        || (n.kind == NodeKind::Stage && n.implementer.as_ref() == Some(member))
    {
        return Err(invalid("Replacement member cannot take this task."));
    }
    n.assignee = Some(member.clone());
    ctx.db
        .execute(
            "UPDATE nodes SET data=?3 WHERE team=?1 AND id=?2",
            params![ctx.team.id, node, rows::node_payload(&n)?],
        )
        .map_err(storage)?;
    let state = if handout::has_attempt(ctx, *node)? {
        DecisionState::Pending
    } else {
        DecisionState::Settled
    };
    ctx.decision(*node, "reassign", why, state)?;
    Ok(())
}
pub(super) fn cancel(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Cancel { node, reason: why } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    ctx.undecided(*node)?;
    ctx.live_node(*node)?;
    reason(why)?;
    ctx.decision(*node, "cancel", why, DecisionState::Pending)?;
    Ok(())
}
pub(super) fn integration(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::RecordIntegration {
        node,
        commit,
        decision,
    } = ctx.op
    else {
        unreachable!()
    };
    let n = ctx.node(*node)?;
    let permit = integration::permit(ctx, *node, *decision)?;
    if !matches!(commit.len(), 40 | 64)
        || !commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid(
            "Integration receipt must name a full lowercase commit ID.",
        ));
    }
    integration::record(ctx, &permit, *node, Some(commit), &[])?;
    // The fact belongs to the accepted generation. A person's restore starts
    // a new round whose completion must not inherit this older merge.
    if !needs_merge(ctx, &n) || n.integrated || permit.generation != ctx.generation(*node)? {
        return Ok(());
    }
    let table = ctx.node_table(*node)?;
    ctx.db
        .execute(
            &format!(
                "UPDATE {table} SET integrated=1,integration_commit=?3 WHERE team=?1 AND id=?2"
            ),
            params![ctx.team.id, node, commit],
        )
        .map_err(storage)?;
    let done: bool = ctx
        .db
        .query_row(
            &format!("SELECT task_stage='done' FROM {table} WHERE team=?1 AND id=?2"),
            params![ctx.team.id, node],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if done && !permit.late {
        ctx.settle(*node)?;
        ctx.close_inbox(*node)?;
    }
    Ok(())
}
pub(super) fn conflict(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::RecordIntegrationConflict {
        node,
        conflicts,
        decision,
    } = ctx.op
    else {
        unreachable!()
    };
    let n = ctx.node(*node)?;
    let permit = integration::permit(ctx, *node, *decision)?;
    if conflicts.is_empty() || conflicts.len() > 200 {
        return Err(invalid(
            "No unresolved integration conflict can be recorded.",
        ));
    }
    for path in conflicts {
        reason(path)?;
        text(path, 4096)?;
    }
    let body = conflicts.join("\n");
    text(&body, 16_384)?;
    integration::record(ctx, &permit, *node, None, conflicts)?;
    if permit.late
        || !needs_merge(ctx, &n)
        || n.integrated
        || permit.generation != ctx.generation(*node)?
    {
        return Ok(());
    }
    ctx.settle(*node)?;
    ctx.decision(*node, "return", &body, DecisionState::Pending)?;
    ctx.reply.item = Some(ctx.inbox(*node, "integration_conflict", &body)?);
    Ok(())
}
