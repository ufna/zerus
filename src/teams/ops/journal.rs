use super::*;
pub(super) fn kind(operation: &Operation) -> Result<String, Problem> {
    Ok(serde_json::to_value(operation).map_err(storage)?["op"]
        .as_str()
        .ok_or_else(|| invalid("Missing operation kind."))?
        .to_owned())
}

pub(super) fn references(request: &Request) -> Result<Vec<String>, Problem> {
    let value = serde_json::to_value(&request.operation).map_err(storage)?;
    let mut refs = vec![format!("team:{}", request.team)];
    for (field, prefix) in [
        ("node", "node"),
        ("member", "member"),
        ("effect_request_id", "effect"),
        ("item", "inbox"),
        ("card", "card"),
    ] {
        if let Some(value) = value.get(field) {
            if let Some(value) = value.as_str() {
                refs.push(format!("{prefix}:{value}"));
            } else if let Some(value) = value.as_u64() {
                refs.push(format!("{prefix}:{value}"));
            }
        }
    }
    if let Some(task) = value
        .get("task")
        .and_then(|t| t.get("uuid"))
        .and_then(|id| id.as_str())
    {
        refs.push(format!("task:{task}"));
    }
    Ok(refs)
}

pub(super) fn append(ctx: &Context<'_>, request: &Request) -> Result<()> {
    let kind = ctx
        .event
        .map(str::to_owned)
        .map_or_else(|| kind(ctx.op), Ok)?;
    ctx.db.execute("INSERT INTO journal(team,at_ms,actor,kind,request_id,text,refs) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![ctx.team.id,ctx.now,encode(ctx.actor)?,kind,request.request_id,format!("{kind} at revision {}",ctx.reply.revision),encode(&references(request)?)?]).map_err(storage)?;
    let cutoff: Option<u64> = ctx
        .db
        .query_row(
            "SELECT seq FROM journal WHERE team=?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
            params![ctx.team.id, store::LIVE_JOURNAL_CAP],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if let Some(cutoff) = cutoff {
        archive(ctx, Some(cutoff))?;
    }
    if matches!(ctx.op, Operation::Archive)
        || matches!(ctx.team.status, TeamStatus::Archived | TeamStatus::Finished)
    {
        archive(ctx, None)?;
    }
    Ok(())
}
fn archive(ctx: &Context<'_>, cutoff: Option<u64>) -> Result<()> {
    ctx.db.execute("INSERT INTO journal_archive SELECT * FROM journal WHERE team=?1 AND (?2 IS NULL OR seq<=?2)",params![ctx.team.id,cutoff]).map_err(storage)?;
    ctx.db
        .execute(
            "DELETE FROM journal WHERE team=?1 AND (?2 IS NULL OR seq<=?2)",
            params![ctx.team.id, cutoff],
        )
        .map_err(storage)?;
    Ok(())
}
