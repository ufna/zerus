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
    save(ctx, &m)
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
