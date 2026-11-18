//! A service-observed, terminal provider failure hands an exact submitted
//! attempt back through the ordinary task or pipeline writer.
use super::*;
use crate::state::{
    team_provider_stop::{ProviderStop, ProviderStopKind},
    TeamRecord,
};

pub(super) struct Proof<'a> {
    pub member: &'a Member,
    pub record: &'a TeamRecord,
    pub stop: ProviderStop,
}

pub(crate) const CHANGED: &str = "team_provider_stop_changed";

fn changed() -> Problem {
    Problem::new(
        CHANGED,
        "The provider stop no longer names the team's current hand-out.",
    )
}

pub(super) fn release(ctx: &mut Context<'_>) -> Result<()> {
    let Operation::ReleaseProviderStop {
        node,
        effect_request_id,
        member,
    } = ctx.op
    else {
        unreachable!()
    };
    let proof = ctx.provider_stop.as_ref().ok_or_else(|| {
        Problem::new(
            code::NOT_ALLOWED,
            "A current native provider stop is required.",
        )
    })?;
    let m = ctx.member(member)?;
    let expected = proof.member;
    let record = proof.record;
    let Some(session) = &m.session else {
        return Err(changed());
    };
    let launch = session.session_id.strip_prefix("launch_id:");
    if ctx.team.status != TeamStatus::Running
        || m.id == ctx.team.lead
        || m.state != MemberState::Active
        || m.session != expected.session
        || m.worktree != record.cwd
        || m.agent != record.agent
        || m.account != record.account
        || session.machine != ctx.team.home
        || launch != Some(record.launch_id.as_str())
    {
        return Err(changed());
    }
    let bound: bool = ctx.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM member_handles WHERE team=?1 AND member=?2 AND launch=?3 AND session_id=?4 AND machine=?5)",
        params![ctx.team.id, member, record.launch_id, record.session_id, session.machine],
        |row| row.get(0),
    ).map_err(storage)?;
    if !bound {
        return Err(changed());
    }

    let n = ctx.live_node(*node)?;
    if !matches!(
        n.state,
        NodeState::Waiting | NodeState::Pending | NodeState::InProgress
    ) || ctx.pending(*node)?.is_some()
    {
        return Err(changed());
    }
    let current: Option<(String, String, String, i64)> = ctx.db.query_row(
        "SELECT request_id,member,state,created_ms FROM effects WHERE team=?1 AND node=?2 AND generation=?3 ORDER BY seq DESC LIMIT 1",
        params![ctx.team.id, node, ctx.generation(*node)?],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional().map_err(storage)?;
    if !current.is_some_and(|(id, owner, state, handed_at)| {
        id == *effect_request_id
            && owner == *member
            && state == "submitted"
            && (handed_at..=ctx.now).contains(&proof.stop.at_ms)
    }) || rows::observation(ctx.db, &ctx.team.id, *node)?.is_some_and(|observation| {
        observation.key != KeyState::Ours
            || observation.cancelled
            || observation.attempt_conflict
            || (!observation.attempt_id.is_empty() && observation.attempt_id != *effect_request_id)
            || matches!(observation.stage, TaskStage::InReview | TaskStage::Done)
    }) {
        return Err(changed());
    }

    let note = note(&m, &n, proof.stop.kind);
    let reason = note.sentence();
    ctx.db
        .execute(
            "UPDATE effects SET state='superseded' WHERE request_id=?1",
            [effect_request_id],
        )
        .map_err(storage)?;
    if let Some(parent) = n.stage_of {
        if !pipeline::current_stage(ctx, *node, parent)? {
            return Err(changed());
        }
        unstaffed::stage_to_lead(ctx, *node, "", Some(note))?;
    } else {
        let item = members::hand_to_lead(
            ctx,
            *node,
            &format!("provider_stop:{effect_request_id}"),
            &reason,
        )?;
        ctx.db
            .execute(
                "UPDATE inbox SET notes=?2 WHERE team=?1 AND seq=?3",
                params![ctx.team.id, encode(&vec![note])?, item],
            )
            .map_err(storage)?;
        ctx.reply.item = Some(item);
    }
    ctx.reply.node = Some(*node);
    Ok(())
}

fn note(member: &Member, node: &Node, kind: ProviderStopKind) -> ReportNote {
    let (code, reason) = match kind {
        ProviderStopKind::Quota => ("quota", "hit its usage limit"),
        ProviderStopKind::SessionLimit => ("session_limit", "hit its provider session limit"),
        ProviderStopKind::Authentication => {
            ("authentication", "could not authenticate with its provider")
        }
        ProviderStopKind::ProviderPolicy => {
            ("provider_policy", "was stopped by its provider's policy")
        }
        ProviderStopKind::ContextLimit => ("context_limit", "hit its context limit"),
        ProviderStopKind::ModelUnavailable => {
            ("model_unavailable", "could not use its selected model")
        }
        ProviderStopKind::Provider => ("provider", "stopped after a provider error"),
    };
    let id = &member.id;
    let agent = match member.agent.as_str() {
        "claude" => "Claude",
        "codex" => "Codex",
        "kimi" => "Kimi",
        "dsh" => "DeepSeek",
        "opencode" => "OpenCode",
        agent => agent,
    };
    let target = node.stage_of.unwrap_or(node.id);
    let text = if node.stage_of.is_some() {
        let stage = node.id;
        format!("Member {id} ({agent}) {reason} during stage #{stage} for task #{target}. The review returned to the lead. Review the changes or choose the next action.")
    } else {
        format!("Member {id} ({agent}) {reason} while holding task #{target}. The task is back with the lead.")
    };
    let mut parameters = vec![id.clone(), agent.to_owned(), target.to_string()];
    if node.stage_of.is_some() {
        parameters.push(node.id.to_string());
    }
    ReportNote {
        code: format!("provider_stop_{code}"),
        text: Some(text),
        parameters,
    }
}
