use super::*;

pub(super) fn ask(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Ask {
        node,
        generation,
        text: body,
    } = ctx.op
    else {
        unreachable!()
    };
    ctx.active()?;
    let n = ctx.live_node(*node)?;
    let holder = handout::holder(ctx, *node)?;
    if let Actor::Session { member, .. } = ctx.actor {
        if holder.as_ref() != Some(member) {
            return Err(Problem::new(
                code::NOT_ALLOWED,
                "Only the attempt holder may ask about it.",
            ));
        }
    }
    if *generation != ctx.generation(*node)?
        || !matches!(n.state, NodeState::InProgress | NodeState::Reported)
    {
        return Err(invalid("Question needs a current active attempt."));
    }
    reason(body)?;
    let count:u32=ctx.db.query_row("SELECT count(*) FROM inbox WHERE team=?1 AND node=?2 AND generation=?3 AND kind LIKE 'question:%' AND state='open'",params![ctx.team.id,node,generation],|r|r.get(0)).map_err(storage)?;
    if count >= ctx.team.settings.caps.max_open_questions {
        return Err(over("Open question cap reached."));
    }
    let next: u64 = ctx
        .db
        .query_row("SELECT COALESCE(MAX(seq),0)+1 FROM inbox", [], |r| r.get(0))
        .map_err(storage)?;
    ctx.reply.item = Some(ctx.inbox(*node, &format!("question:{next}"), body)?);
    Ok(())
}
pub(super) fn reply(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::Reply { item, text: body } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    reason(body)?;
    let changed=ctx.db.execute("UPDATE inbox SET reply=?3,reply_actor=?4,state='closed' WHERE team=?1 AND seq=?2 AND state='open' AND kind LIKE 'question:%'",params![ctx.team.id,item,body,encode(ctx.actor)?]).map_err(storage)?;
    if changed != 1 {
        return Err(invalid("Question is missing or already answered."));
    }
    Ok(())
}
pub(super) fn open_card(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::OpenCard { kind, text, node } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    if matches!(kind.as_str(), "integration" | "pipeline_acceptance") {
        return Err(invalid(
            "Acceptance cards are opened by their pipeline or decision.",
        ));
    }
    if let Some(node) = node {
        ctx.node(*node)?;
    }
    ctx.reply.item = Some(card(ctx, kind, text, *node, None)?);
    Ok(())
}
pub(super) fn card(
    ctx: &Context<'_>,
    kind: &str,
    body: &str,
    node: Option<u32>,
    decision: Option<u64>,
) -> Result<u64> {
    reason(kind)?;
    text(kind, 128)?;
    reason(body)?;
    ctx.db.execute("INSERT INTO cards(team,kind,node,actor,text,created_ms,decision_seq) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![ctx.team.id,kind,node,encode(ctx.actor)?,body,ctx.now,decision]).map_err(storage)?;
    Ok(ctx.db.last_insert_rowid() as u64)
}
pub(super) fn decide_card(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::DecideCard { card, decision } = ctx.op else {
        unreachable!()
    };
    reason(decision)?;
    let row: Option<(String, Option<u32>, Option<u64>)> = ctx
        .db
        .query_row(
            "SELECT kind,node,decision_seq FROM cards WHERE team=?1 AND seq=?2 AND state='open'",
            params![ctx.team.id, card],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(storage)?;
    let Some((kind, node, seq)) = row else {
        return Err(unavailable_card(ctx, *card)?);
    };
    let stale = if kind == "integration" {
        decide_integration(
            ctx,
            node.ok_or_else(missing)?,
            seq.ok_or_else(missing)?,
            decision,
        )?
    } else if kind == "pipeline_acceptance" {
        pipeline::decide_card(ctx, node.ok_or_else(missing)?, decision)?
    } else {
        ctx.active()?;
        false
    };
    if stale {
        ctx.event = Some("stale_card_decision");
    }
    ctx.db
        .execute(
            "UPDATE cards SET decision=?3,decided_actor=?4,state='closed' WHERE team=?1 AND seq=?2",
            params![
                ctx.team.id,
                card,
                if stale { "stale" } else { decision },
                encode(ctx.actor)?
            ],
        )
        .map_err(storage)?;
    Ok(())
}
fn decide_integration(ctx: &Context<'_>, node: u32, seq: u64, answer: &str) -> Result<bool> {
    if ctx.pending(node)? != Some((seq, "accept".into(), DecisionState::AwaitingOwner)) {
        return Ok(true);
    }
    ctx.active()?;
    match answer {
        "approve" => {
            ctx.db
                .execute(
                    "UPDATE decisions SET state=?2 WHERE seq=?1",
                    params![seq, DecisionState::Ready],
                )
                .map_err(storage)?;
        }
        "reject" => {
            ctx.settle(node)?;
            ctx.db
                .execute(
                    "UPDATE nodes SET decided_by=NULL,hold=0 WHERE team=?1 AND id=?2",
                    params![ctx.team.id, node],
                )
                .map_err(storage)?;
        }
        _ => return Err(invalid("Integration decision must be approve or reject.")),
    }
    Ok(false)
}

pub(super) fn close_card(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::CloseCard { card, reason: why } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    reason(why)?;
    let changed = ctx.db.execute("UPDATE cards SET state='closed',decision='stale',decided_actor=?3 WHERE team=?1 AND seq=?2 AND state='open' AND kind NOT IN ('integration','pipeline_acceptance') AND actor=?3",params![ctx.team.id,card,encode(ctx.actor)?]).map_err(storage)?;
    if changed != 1 {
        return Err(invalid(
            "Only an open ordinary card opened by this service can be closed.",
        ));
    }
    Ok(())
}

fn unavailable_card(ctx: &Context<'_>, card: u64) -> Result<Problem> {
    let stale = rows::card(ctx.db, &ctx.team.id, card)?
        .is_some_and(|c| c.decision.as_deref() == Some("stale"));
    Ok(invalid(if stale {
        "Card is stale."
    } else {
        "Card is missing or already decided."
    }))
}
