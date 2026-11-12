use super::*;

pub(super) fn set_session(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::SetMemberSession {
        member,
        session,
        account,
        worktree,
        ownership_nonce,
    } = ctx.op
    else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if m.state == MemberState::Removed || m.session.is_some() {
        return Err(invalid("Member is removed or already bound."));
    }
    uuid(&session.machine)?;
    if let Some(id) = session.session_id.strip_prefix("launch_id:") {
        uuid(id)?;
    } else if let Some(id) = session.session_id.strip_prefix("run_id:") {
        reason(id)?;
        text(id, 256)?;
    } else {
        return Err(invalid("Session must use launch_id or run_id."));
    }
    if session.machine != ctx.team.home || worktree.is_empty() || ownership_nonce.is_empty() {
        return Err(invalid(
            "A local session with an owned worktree is required.",
        ));
    }
    text(account, 256)?;
    text(worktree, 4096)?;
    text(ownership_nonce, 256)?;
    let bound: bool = ctx
        .db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE machine=?1 AND session_id=?2)",
            params![session.machine, session.session_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if bound {
        return Err(invalid("Session is already bound to a member."));
    }
    m.session = Some(session.clone());
    m.account = account.clone();
    m.worktree = worktree.clone();
    m.ownership_nonce = ownership_nonce.clone();
    m.state = MemberState::Active;
    save(ctx, &m)
}
pub(super) fn set_state(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::SetMemberState { member, state } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if m.state == *state
        || m.state == MemberState::Removed
        || matches!(state, MemberState::Removed | MemberState::NotCreated)
    {
        return Err(invalid("Invalid member state transition."));
    }
    if *state == MemberState::Active && (m.session.is_none() || ctx.person_stopped(member)?) {
        return Err(invalid(
            "The member has no session or was stopped by the person.",
        ));
    }
    m.state = *state;
    save(ctx, &m)
}
pub(super) fn pause(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::PauseMember { member } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if m.state != MemberState::Active {
        return Err(invalid("Only an active member can be paused."));
    }
    m.state = MemberState::Paused;
    save(ctx, &m)
}
pub(super) fn resume(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::ResumeMember { member } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if m.state != MemberState::Paused
        || m.session.is_none()
        || (ctx.person_stopped(member)? && !is_person(ctx.actor))
    {
        return Err(invalid("The member cannot be resumed by this actor."));
    }
    ctx.db
        .execute(
            "UPDATE members SET person_stopped=0 WHERE team=?1 AND id=?2",
            params![ctx.team.id, member],
        )
        .map_err(storage)?;
    m.state = MemberState::Active;
    save(ctx, &m)
}
pub(super) fn stop(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::StopMember { member } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if !matches!(m.state, MemberState::Active | MemberState::Paused)
        || ctx.person_stopped(member)?
    {
        return Err(invalid("The member cannot be stopped."));
    }
    ctx.db
        .execute(
            "UPDATE members SET person_stopped=1 WHERE team=?1 AND id=?2",
            params![ctx.team.id, member],
        )
        .map_err(storage)?;
    m.state = MemberState::Paused;
    save(ctx, &m)
}
pub(super) fn archive(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::ArchiveMember { member } = ctx.op else {
        unreachable!()
    };
    ctx.active()?;
    let mut m = ctx.member(member)?;
    if m.state == MemberState::Removed || m.id == ctx.team.lead {
        return Err(invalid("The member cannot be removed."));
    }
    m.state = MemberState::Removed;
    save(ctx, &m)?;
    hand_back(ctx, member)
}
/// Removal settles the Returns and Reassigns aimed at the member that have
/// not started and hands each node back to the lead as a reopen that could
/// not be delivered does: the decision settled, the node held, and one inbox
/// item. A stopped or busy member keeps them; that wait is intended.
fn hand_back(ctx: &mut Context<'_>, member: &str) -> Result<()> {
    for (seq, node, kind) in reopen_decisions(ctx)? {
        if ctx.unstarted(node)?.map(|(s, _)| s) != Some(seq)
            || aimed_at(ctx, node, &kind)?.as_deref() != Some(member)
        {
            continue;
        }
        let body = format!(
            "Member {member} was removed before the {kind} of task {node} started; the decision is settled and the task waits for a new one."
        );
        hand_to_lead(ctx, node, &format!("member_removed:{seq}"), &body)?;
    }
    Ok(())
}
/// The member a pending Return or Reassign is aimed at, read only to hand
/// its node back when that member is removed: a Reassign's assignee, and for
/// a Return the member of the newest effect of the node's current
/// generation, whatever that effect's state. That is wider than the holder a
/// reopen is delivered to (`handout::reopen_target`), so that a Return
/// admitted while its member's hand-out was only reserved is still found. It
/// never chooses a recipient for a delivery.
fn aimed_at(ctx: &Context<'_>, node: u32, kind: &str) -> Result<Option<String>> {
    if kind == "reassign" {
        return Ok(ctx.node(node)?.assignee);
    }
    ctx.db
        .query_row(
            "SELECT member FROM effects WHERE team=?1 AND node=?2 AND generation=?3 ORDER BY seq DESC LIMIT 1",
            params![ctx.team.id, node, ctx.generation(node)?],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)
}
/// The member a Return of the node would go back to, its holder, when that
/// member was removed from the team: such a Return could never be carried
/// out. Every writer of a Return asks it.
pub(super) fn removed_holder(ctx: &Context<'_>, node: u32) -> Result<Option<String>> {
    let Some(holder) = handout::holder(ctx, node)? else {
        return Ok(None);
    };
    Ok((ctx.member(&holder)?.state == MemberState::Removed).then_some(holder))
}
/// Hands a node to the lead instead of a Return or Reassign that its member
/// can no longer carry out, through the store's one writer of that outcome
/// (`rows::hand_back`): its unsettled decision settled, the node held, one
/// inbox item of the cause's own kind. The operation moves the revision.
pub(super) fn hand_to_lead(
    ctx: &mut Context<'_>,
    node: u32,
    kind: &str,
    text: &str,
) -> Result<u64> {
    rows::hand_back(
        ctx.db,
        &ctx.team.id,
        node,
        &encode(ctx.actor)?,
        kind,
        text,
        ctx.now,
    )
}
/// The team's unsettled Returns and Reassigns, from the partial index of
/// open decisions and within its bound.
fn reopen_decisions(ctx: &Context<'_>) -> Result<Vec<(u64, u32, String)>> {
    let mut s = ctx.db.prepare("SELECT seq,node,kind FROM decisions INDEXED BY decision_pending WHERE team=?1 AND state<>'settled' AND kind IN ('return','reassign') ORDER BY node LIMIT ?2").map_err(storage)?;
    let rows = s
        .query_map(
            params![ctx.team.id, delegation::MAX_OPEN_DECISIONS + 1],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() > delegation::MAX_OPEN_DECISIONS {
        return Err(over("Too many open decisions to settle for the member."));
    }
    Ok(rows)
}
fn save(ctx: &Context<'_>, m: &Member) -> Result<()> {
    ctx.db
        .execute(
            "UPDATE members SET data=?3,machine=?4,session_id=?5,state=?6 WHERE team=?1 AND id=?2",
            params![
                ctx.team.id,
                m.id,
                encode(m)?,
                m.session.as_ref().map(|s| s.machine.as_str()),
                m.session.as_ref().map(|s| s.session_id.as_str()),
                encode(&m.state)?
            ],
        )
        .map_err(storage)?;
    Ok(())
}
