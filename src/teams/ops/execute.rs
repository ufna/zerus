use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use store::{decode, encode, storage};

#[derive(Serialize, Deserialize)]
enum Saved {
    Success(Reply),
    Refused(Problem),
}

/// One immediate transaction serializes revision checks, request receipts and
/// the journal. Failed operations roll back their savepoint, including all
/// partial row changes; their first refusal is retained for exact retries.
pub fn apply(
    store: &mut Store,
    actor: &Actor,
    request: &Request,
    now_ms: i64,
) -> Result<Reply, Problem> {
    store.refresh()?;
    uuid(&request.request_id)?;
    uuid(&request.team)?;
    if now_ms < 0 {
        return Err(invalid("Time must be nonnegative."));
    }
    let content = encode(request)?;
    if content.len() > 262_144 {
        return Err(over("Operation exceeds the request size limit."));
    }
    let actor_json = encode(actor)?;
    if store.db.is_none() {
        let team = creation_team(request, actor)?;
        authority(actor, &request.operation, &team)?;
        validate_create(&team)?;
        if request.expected_revision != 0 {
            return Err(stale());
        }
        store.ensure()?;
    }
    let db = store
        .db
        .as_mut()
        .ok_or_else(|| invalid("Missing team store."))?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(storage)?;
    let current = rows::team(&tx, &request.team)?;
    let team = match current.as_ref() {
        Some(team) => team.clone(),
        None => creation_team(request, actor)?,
    };
    verify_actor(&tx, actor, &team)?;
    let saved: Option<(String, String, String)> = tx
        .query_row(
            "SELECT actor,content,result FROM requests WHERE id=?1",
            [&request.request_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(storage)?;
    if let Some((original_actor, original_content, result)) = saved {
        if original_actor != actor_json || original_content != content {
            return Err(Problem::new(
                code::DUPLICATE,
                "Request ID was already used with different content or authority.",
            ));
        }
        return match decode(&result)? {
            Saved::Success(reply) => Ok(reply),
            Saved::Refused(error) => Err(error),
        };
    }
    // Receipt replay above is a read, not renewed operation authority. A new
    // request must satisfy the team's current delegations.
    authority(actor, &request.operation, &team)?;
    tx.execute_batch("SAVEPOINT operation;").map_err(storage)?;
    let result = (|| {
        if current.as_ref().map_or(0, |t| t.revision) != request.expected_revision {
            return Err(stale());
        }
        if current.is_some() && matches!(request.operation, Operation::Create { .. }) {
            return Err(invalid("The team already exists."));
        }
        let mut reply = execute(&tx, actor, &team, &request.operation, now_ms)?;
        let revision = team
            .revision
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| over("Team revision is exhausted."))?;
        tx.execute(
            "UPDATE teams SET revision=?2 WHERE id=?1",
            params![team.id, revision],
        )
        .map_err(storage)?;
        reply.revision = revision;
        let kind = kind(&request.operation)?;
        tx.execute(
            "INSERT INTO journal(team,at_ms,actor,kind,request_id,text,refs) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                team.id,
                now_ms,
                actor_json,
                kind,
                request.request_id,
                format!("{kind} at revision {revision}"),
                encode(&references(request)?)?
            ],
        )
        .map_err(storage)?;
        rows::archive_decided(&tx, &team.id)?;
        // Keep at most the most recent 10,000 journal rows in the live table.
        let cutoff: Option<u64> = tx
            .query_row(
                "SELECT seq FROM journal WHERE team=?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
                params![team.id, store::LIVE_JOURNAL_CAP],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(cutoff) = cutoff {
            tx.execute(
                "INSERT INTO journal_archive SELECT * FROM journal WHERE team=?1 AND seq<=?2",
                params![team.id, cutoff],
            )
            .map_err(storage)?;
            tx.execute(
                "DELETE FROM journal WHERE team=?1 AND seq<=?2",
                params![team.id, cutoff],
            )
            .map_err(storage)?;
        }
        if matches!(request.operation, Operation::Archive)
            || matches!(team.status, TeamStatus::Finished | TeamStatus::Archived)
        {
            tx.execute(
                "INSERT INTO journal_archive SELECT * FROM journal WHERE team=?1",
                [&team.id],
            )
            .map_err(storage)?;
            tx.execute("DELETE FROM journal WHERE team=?1", [&team.id])
                .map_err(storage)?;
        }
        Ok(reply)
    })();
    let saved = match &result {
        Ok(reply) => {
            tx.execute_batch("RELEASE operation;").map_err(storage)?;
            Saved::Success(reply.clone())
        }
        Err(error) => {
            tx.execute_batch("ROLLBACK TO operation; RELEASE operation;")
                .map_err(storage)?;
            Saved::Refused(error.clone())
        }
    };
    tx.execute(
        "INSERT INTO requests(id,team,actor,content,result) VALUES(?1,?2,?3,?4,?5)",
        params![
            request.request_id,
            request.team,
            actor_json,
            content,
            encode(&saved)?
        ],
    )
    .map_err(storage)?;
    tx.commit().map_err(storage)?;
    result
}
fn stale() -> Problem {
    Problem::new(
        code::STALE_REVISION,
        "The team changed; refresh before making a new request.",
    )
}
fn invalid(text: &str) -> Problem {
    Problem::new(code::INVALID_STATE, text)
}
fn over(text: &str) -> Problem {
    Problem::new(code::OVER_CAP, text)
}
fn missing() -> Problem {
    Problem::new(code::NOT_FOUND, "The team record does not exist.")
}
fn uuid(value: &str) -> Result<(), Problem> {
    if uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value) {
        Ok(())
    } else {
        Err(invalid("Expected a lowercase canonical UUID."))
    }
}
fn text(value: &str, max: usize) -> Result<(), Problem> {
    if value.len() <= max {
        Ok(())
    } else {
        Err(over("Text exceeds its size limit."))
    }
}
fn reason(value: &str) -> Result<(), Problem> {
    text(value, 16_384)?;
    if value.trim().is_empty() {
        Err(invalid("A reason is required."))
    } else {
        Ok(())
    }
}
fn kind(operation: &Operation) -> Result<String, Problem> {
    Ok(serde_json::to_value(operation).map_err(storage)?["op"]
        .as_str()
        .ok_or_else(|| invalid("Missing operation kind."))?
        .to_owned())
}

fn references(request: &Request) -> Result<Vec<String>, Problem> {
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

fn creation_team(request: &Request, actor: &Actor) -> Result<Team, Problem> {
    let Operation::Create {
        project,
        name,
        goal,
        home,
        bundle,
        integration_branch,
        base_commit,
    } = &request.operation
    else {
        return Err(missing());
    };
    Ok(Team {
        id: request.team.clone(),
        project: project.clone(),
        name: name.clone(),
        goal: goal.clone(),
        home: home.clone(),
        owner: actor.person().into(),
        status: TeamStatus::Running,
        lead: LEAD.into(),
        settings: bundle.template.settings.clone(),
        bundle: *bundle.clone(),
        integration_branch: integration_branch.clone(),
        base_commit: base_commit.clone(),
        revision: 0,
    })
}
fn settings(settings: &Settings) -> Result<(), Problem> {
    if settings.phase_acceptance == Acceptance::Pipeline {
        return Err(invalid("A phase cannot be accepted by a pipeline."));
    }
    if settings.caps.max_members == 0
        || settings.caps.max_members > 200
        || settings.caps.max_open_tasks == 0
        || settings.caps.max_open_tasks > 500
        || settings.caps.max_phase_tasks == 0
        || settings.caps.max_phase_tasks > 200
        || settings.caps.max_open_questions == 0
        || settings.caps.max_open_questions > 200
    {
        return Err(over("Settings exceed the supported storage bounds."));
    }
    Ok(())
}
fn validate_create(team: &Team) -> Result<(), Problem> {
    uuid(&team.home)?;
    settings(&team.settings)?;
    if team.owner.is_empty() || team.project.is_empty() || team.name.trim().is_empty() {
        return Err(invalid("Team owner, project and name are required."));
    }
    text(&team.name, 256)?;
    text(&team.project, 256)?;
    text(&team.goal, 16_384)?;
    text(&encode(&team.bundle)?, 131_072)?;
    if team.bundle.roles.len() > 200 || team.bundle.pipelines.len() > 200 {
        return Err(over("Bundle has too many entries."));
    }
    let mut ids = std::collections::BTreeSet::new();
    for role in &team.bundle.roles {
        if role.id.is_empty() || !ids.insert(&role.id) {
            return Err(invalid("Role IDs must be nonempty and unique."));
        }
    }
    let seats = team
        .bundle
        .template
        .roster
        .iter()
        .try_fold(0u32, |sum, seat| sum.checked_add(seat.count))
        .ok_or_else(|| over("Roster is too large."))?;
    if seats > team.settings.caps.max_members {
        return Err(over("Roster exceeds the member cap."));
    }
    let mut seated = std::collections::BTreeSet::new();
    for seat in &team.bundle.template.roster {
        if !ids.contains(&seat.role) {
            return Err(
                Problem::new(code::UNKNOWN_ROLE, "Roster names an unknown role.")
                    .with_refs([seat.role.clone()]),
            );
        }
        if seat.count > 0 {
            seated.insert(seat.role.as_str());
        }
    }
    let mut pipelines = std::collections::BTreeSet::new();
    for p in &team.bundle.pipelines {
        if !pipelines.insert(&p.id) {
            return Err(invalid("Pipeline IDs must be unique."));
        }
        if !seated.contains(p.implementer.as_str()) {
            return Err(Problem::new(
                code::NO_SEAT_FOR_STAGE,
                "Pipeline has no implementing seat.",
            ));
        }
        for stage in &p.stages {
            if !seated.contains(stage.role.as_str()) {
                return Err(Problem::new(
                    code::NO_SEAT_FOR_STAGE,
                    "Pipeline stage has no seat.",
                ));
            }
        }
    }
    if !pipelines.contains(&team.bundle.template.default_pipeline) {
        return Err(Problem::new(
            code::UNKNOWN_PIPELINE,
            "Default pipeline does not exist.",
        ));
    }
    Ok(())
}
fn verify_actor(db: &Connection, actor: &Actor, team: &Team) -> Result<(), Problem> {
    if let Actor::Session {
        team: id,
        member,
        session,
        ..
    } = actor
    {
        let bound = rows::member(db, id, member)?
            .filter(|m| m.state != MemberState::Removed && m.session.as_ref() == Some(session));
        if id != &team.id || bound.is_none() {
            return Err(Problem::new(
                code::NOT_ALLOWED,
                "The session is no longer bound to this team member.",
            ));
        }
    }
    Ok(())
}
fn live_node(db: &Connection, team: &Team, id: u32) -> Result<Node, Problem> {
    let node = rows::node(db, &team.id, id)?.ok_or_else(missing)?;
    if matches!(node.state, NodeState::Accepted | NodeState::Cancelled) {
        return Err(invalid("This task is already decided."));
    }
    Ok(node)
}
fn member(db: &Connection, team: &Team, id: &str) -> Result<Member, Problem> {
    rows::member(db, &team.id, id)?
        .ok_or_else(|| Problem::new(code::UNKNOWN_MEMBER, "Member does not exist.").with_refs([id]))
}
fn active_team(team: &Team) -> Result<(), Problem> {
    if matches!(team.status, TeamStatus::Running | TeamStatus::Paused) {
        Ok(())
    } else {
        Err(invalid("The team is not active."))
    }
}
fn generation(db: &Connection, team: &Team, node: u32) -> Result<u32, Problem> {
    db.query_row(
        "SELECT generation FROM nodes WHERE team=?1 AND id=?2",
        params![team.id, node],
        |r| r.get(0),
    )
    .map_err(storage)
}
fn own_node(actor: &Actor, node: &Node) -> Result<(), Problem> {
    if let Actor::Session { member, .. } = actor {
        if node.assignee.as_ref() != Some(member) {
            return Err(Problem::new(
                code::NOT_ALLOWED,
                "A member may only report or ask about its own attempt.",
            ));
        }
    }
    Ok(())
}
fn save_member(db: &Connection, team: &Team, m: &Member) -> Result<(), Problem> {
    db.execute(
        "UPDATE members SET data=?3,machine=?4,session_id=?5,state=?6 WHERE team=?1 AND id=?2",
        params![
            team.id,
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
fn decision(
    db: &Connection,
    actor: &Actor,
    team: &Team,
    node: u32,
    kind: &str,
    reason: &str,
) -> Result<(), Problem> {
    db.execute(
        "INSERT INTO decisions(team,node,generation,actor,kind,text) VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            team.id,
            node,
            generation(db, team, node)?,
            encode(actor)?,
            kind,
            reason
        ],
    )
    .map_err(storage)?;
    db.execute(
        "UPDATE nodes SET requested=?3 WHERE team=?1 AND id=?2",
        params![team.id, node, kind],
    )
    .map_err(storage)?;
    Ok(())
}
fn inbox(
    db: &Connection,
    actor: &Actor,
    team: &Team,
    node: u32,
    kind: &str,
    body: &str,
    now: i64,
) -> Result<u64, Problem> {
    db.execute("INSERT INTO inbox(team,kind,node,generation,actor,text,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![team.id,kind,node,generation(db,team,node)?,encode(actor)?,body,now]).map_err(storage)?;
    Ok(db.last_insert_rowid() as u64)
}
fn observe(
    db: &Connection,
    team: &Team,
    node: u32,
    observation: &TaskObservation,
) -> Result<(), Problem> {
    text(&observation.rank, 256)?;
    reason(&observation.revision)?;
    text(&observation.revision, 256)?;
    text(&observation.previous_revision, 256)?;
    // Reopening a task from the ordinary task board restores its live row.
    if !db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE team=?1 AND id=?2)",
            params![team.id, node],
            |r| r.get::<_, bool>(0),
        )
        .map_err(storage)?
    {
        if !observation.cancelled && observation.stage != TaskStage::Done {
            let count: u32 = db
                .query_row(
                    "SELECT count(*) FROM nodes WHERE team=?1",
                    [&team.id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if count >= team.settings.caps.max_open_tasks {
                return Err(over("Reopening the task would exceed the live task cap."));
            }
        }
        let cols = rows::NODE_COLUMNS;
        let copied=db.execute(&format!("INSERT INTO nodes({cols}) SELECT {cols} FROM node_archive WHERE team=?1 AND id=?2"),params![team.id,node]).map_err(storage)?;
        if copied == 0 {
            return Err(missing());
        }
        db.execute(
            "DELETE FROM node_archive WHERE team=?1 AND id=?2",
            params![team.id, node],
        )
        .map_err(storage)?;
    }
    let old:(String,String,bool,String)=db.query_row("SELECT task_revision,task_stage,task_cancelled,rank FROM nodes WHERE team=?1 AND id=?2",params![team.id,node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(storage)?;
    let stage = encode(&observation.stage)?.trim_matches('"').to_owned();
    if (old.0 != observation.revision && old.0 != observation.previous_revision)
        || (old.0 == observation.revision
            && (old.1 != stage || old.2 != observation.cancelled || old.3 != observation.rank))
    {
        return Err(stale());
    }
    db.execute("UPDATE nodes SET task_revision=?3,task_stage=?4,task_cancelled=?5,rank=?6 WHERE team=?1 AND id=?2",params![team.id,node,observation.revision,stage,observation.cancelled,observation.rank]).map_err(storage)?;
    if observation.cancelled || observation.stage == TaskStage::Done {
        db.execute("UPDATE effects SET state='observed_done' WHERE team=?1 AND node=?2 AND state IN ('reserved','submitted','uncertain','cutting_over')",params![team.id,node]).map_err(storage)?;
        db.execute(
            "UPDATE inbox SET state='closed' WHERE team=?1 AND node=?2 AND state='open'",
            params![team.id, node],
        )
        .map_err(storage)?;
    }
    Ok(())
}

fn execute(
    db: &Connection,
    actor: &Actor,
    team: &Team,
    op: &Operation,
    now: i64,
) -> Result<Reply, Problem> {
    let mut reply = Reply {
        ok: true,
        revision: 0,
        node: None,
        item: None,
    };
    let actor_json = encode(actor)?;
    match op {
        Operation::Create { .. } => {
            validate_create(team)?;
            db.execute("INSERT INTO teams(id,project,name,goal,home,owner,status,lead,settings,bundle,integration_branch,base_commit,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0)",params![team.id,team.project,team.name,team.goal,team.home,team.owner,encode(&team.status)?,team.lead,encode(&team.settings)?,encode(&team.bundle)?,team.integration_branch,team.base_commit]).map_err(storage)?;
            let mut members = vec![Member {
                id: LEAD.into(),
                role: LEAD.into(),
                agent: team.bundle.template.lead.agent.clone(),
                account: String::new(),
                state: MemberState::NotCreated,
                session: None,
                worktree: String::new(),
                ownership_nonce: String::new(),
            }];
            for seat in &team.bundle.template.roster {
                let role = team
                    .bundle
                    .roles
                    .iter()
                    .find(|r| r.id == seat.role)
                    .ok_or_else(|| invalid("Missing role."))?;
                for _ in 0..seat.count {
                    members.push(Member {
                        id: format!("m{}", members.len()),
                        role: role.id.clone(),
                        agent: role.agent.clone(),
                        account: String::new(),
                        state: MemberState::NotCreated,
                        session: None,
                        worktree: String::new(),
                        ownership_nonce: String::new(),
                    });
                }
            }
            for m in members {
                db.execute(
                    "INSERT INTO members(team,id,data,state) VALUES(?1,?2,?3,?4)",
                    params![team.id, m.id, encode(&m)?, encode(&m.state)?],
                )
                .map_err(storage)?;
            }
            if team.settings.queue {
                let phase = Phase {
                    n: QUEUE_PHASE,
                    order: 0,
                    title: "Queue".into(),
                    kind: PhaseKind::Queue,
                    mode: PhaseMode::Free,
                    approval: team.settings.plan_approval,
                    state: PhaseState::Approved,
                    acceptance: team.settings.phase_acceptance,
                    barrier: false,
                    paused: false,
                };
                db.execute(
                    "INSERT INTO phases(team,n,data) VALUES(?1,0,?2)",
                    params![team.id, encode(&phase)?],
                )
                .map_err(storage)?;
            }
        }
        Operation::SetMemberSession {
            member: id,
            session,
            account,
            worktree,
            ownership_nonce,
        } => {
            active_team(team)?;
            let mut m = member(db, team, id)?;
            if m.state == MemberState::Removed || m.session.is_some() {
                return Err(invalid(
                    "A removed or already bound member cannot be bound again.",
                ));
            }
            uuid(&session.machine)?;
            if let Some(launch) = session.session_id.strip_prefix("launch_id:") {
                uuid(launch)?;
            } else if let Some(run) = session.session_id.strip_prefix("run_id:") {
                reason(run)?;
                text(run, 256)?;
            } else {
                return Err(invalid("Session identity must use launch_id or run_id."));
            }
            if session.machine != team.home || worktree.is_empty() || ownership_nonce.is_empty() {
                return Err(invalid(
                    "M1 requires a local session and its owned worktree.",
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
            save_member(db, team, &m)?;
        }
        Operation::SetMemberState { member: id, state } => {
            active_team(team)?;
            let mut m = member(db, team, id)?;
            if m.state == *state
                || m.state == MemberState::Removed
                || *state == MemberState::Removed
                || *state == MemberState::NotCreated
            {
                return Err(invalid("Invalid member state transition."));
            }
            if *state == MemberState::Active && m.session.is_none() {
                return Err(invalid("A member without a session cannot be active."));
            }
            let stopped: bool = db
                .query_row(
                    "SELECT person_stopped FROM members WHERE team=?1 AND id=?2",
                    params![team.id, id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if stopped && *state == MemberState::Active {
                return Err(invalid(
                    "A person stopped this member; only the person may resume it.",
                ));
            }
            m.state = *state;
            save_member(db, team, &m)?;
        }
        Operation::Pause | Operation::Resume | Operation::Stop | Operation::Archive => {
            let next = match op {
                Operation::Pause if team.status == TeamStatus::Running => TeamStatus::Paused,
                Operation::Resume if team.status == TeamStatus::Paused => TeamStatus::Running,
                Operation::Stop
                    if matches!(team.status, TeamStatus::Running | TeamStatus::Paused) =>
                {
                    TeamStatus::Stopped
                }
                Operation::Archive
                    if matches!(team.status, TeamStatus::Stopped | TeamStatus::Finished) =>
                {
                    TeamStatus::Archived
                }
                _ => return Err(invalid("Invalid team lifecycle transition.")),
            };
            if next == TeamStatus::Stopped {
                // Cancellation is a task-writer command. Do not invent Done or
                // Cancelled observations before that writer has settled.
                db.execute("INSERT INTO decisions(team,node,generation,actor,kind,text) SELECT team,id,generation,?2,'cancel','Team stopped' FROM nodes WHERE team=?1",params![team.id,actor_json]).map_err(storage)?;
                db.execute(
                    "UPDATE nodes SET requested='cancel',hold=1 WHERE team=?1",
                    [&team.id],
                )
                .map_err(storage)?;
            }
            db.execute(
                "UPDATE teams SET status=?2 WHERE id=?1",
                params![team.id, encode(&next)?],
            )
            .map_err(storage)?;
        }
        Operation::PauseMember { member: id }
        | Operation::ResumeMember { member: id }
        | Operation::StopMember { member: id }
        | Operation::ArchiveMember { member: id } => {
            active_team(team)?;
            let mut m = member(db, team, id)?;
            let stopped: bool = db
                .query_row(
                    "SELECT person_stopped FROM members WHERE team=?1 AND id=?2",
                    params![team.id, id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            let next = match op {
                Operation::PauseMember { .. } if m.state == MemberState::Active => {
                    MemberState::Paused
                }
                Operation::ResumeMember { .. }
                    if m.state == MemberState::Paused
                        && (!stopped || matches!(actor, Actor::Person { .. })) =>
                {
                    MemberState::Active
                }
                Operation::StopMember { .. }
                    if matches!(m.state, MemberState::Active | MemberState::Paused) && !stopped =>
                {
                    MemberState::Paused
                }
                Operation::ArchiveMember { .. }
                    if m.state != MemberState::Removed && m.id != team.lead =>
                {
                    MemberState::Removed
                }
                _ => return Err(invalid("Invalid member lifecycle transition.")),
            };
            if next == MemberState::Active && m.session.is_none() {
                return Err(invalid("Member has no session."));
            }
            if matches!(
                op,
                Operation::StopMember { .. } | Operation::ResumeMember { .. }
            ) {
                db.execute(
                    "UPDATE members SET person_stopped=?3 WHERE team=?1 AND id=?2",
                    params![team.id, id, matches!(op, Operation::StopMember { .. })],
                )
                .map_err(storage)?;
            }
            m.state = next;
            save_member(db, team, &m)?;
        }
        Operation::AddTask {
            task,
            title,
            observation,
            role,
            pipeline,
        } => {
            active_team(team)?;
            if !team.settings.queue {
                return Err(invalid("The team does not take queued tasks."));
            }
            uuid(&task.uuid)?;
            if task.project != team.project {
                return Err(invalid("The task belongs to another project."));
            }
            let existing: Option<(String, u32)> = db
                .query_row(
                    "SELECT team,id FROM node_identity WHERE task_project=?1 AND task_uuid=?2",
                    params![task.project, task.uuid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(storage)?;
            if let Some((id, node)) = existing {
                if id != team.id {
                    return Err(Problem::new(
                        code::DUPLICATE,
                        "Task already belongs to another team.",
                    ));
                }
                reply.node = Some(node);
                return Ok(reply);
            }
            if observation.cancelled
                || !matches!(observation.stage, TaskStage::Todo | TaskStage::Backlog)
            {
                return Err(invalid(
                    "Only Backlog or To do tasks may be sent to a team.",
                ));
            }
            text(title, 1024)?;
            text(&observation.rank, 256)?;
            reason(&observation.revision)?;
            text(&observation.revision, 256)?;
            if !observation.previous_revision.is_empty() {
                return Err(invalid(
                    "Initial task observation cannot have a previous projection.",
                ));
            }
            let count: u32 = db
                .query_row(
                    "SELECT count(*) FROM nodes WHERE team=?1",
                    [&team.id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if count >= team.settings.caps.max_open_tasks {
                return Err(over("Team has reached its open task cap."));
            }
            let pipeline_id = pipeline
                .as_deref()
                .unwrap_or(&team.bundle.template.default_pipeline);
            let p = team
                .bundle
                .pipelines
                .iter()
                .find(|p| p.id == pipeline_id)
                .ok_or_else(|| Problem::new(code::UNKNOWN_PIPELINE, "Unknown pipeline."))?;
            let role_id = role.as_deref().unwrap_or(&p.implementer);
            route_valid(team, role_id, pipeline_id)?;
            let routed =
                team.settings.routing == Routing::Direct || role.is_some() || pipeline.is_some();
            let admitted = matches!(actor, Actor::Person { .. })
                || matches!(actor,Actor::Session{member,..} if member==&team.lead);
            let id: u32 = db
                .query_row("SELECT next_node FROM teams WHERE id=?1", [&team.id], |r| {
                    r.get(0)
                })
                .map_err(storage)?;
            if id == u32::MAX {
                return Err(over("Node counter is exhausted."));
            }
            let node = Node {
                id,
                phase: QUEUE_PHASE,
                task: Some(task.clone()),
                title: title.clone(),
                text: String::new(),
                source: NodeSource::Sent,
                deps: vec![],
                assignee: None,
                role: if routed {
                    role_id.into()
                } else {
                    String::new()
                },
                pipeline: if routed {
                    pipeline_id.into()
                } else {
                    String::new()
                },
                writes: vec![],
                estimate_minutes: 0,
                kind: NodeKind::Task,
                severity: Severity::None,
                origin: None,
                stage_of: None,
                stage_index: 0,
                round: 1,
                implementer: None,
                hold: false,
                admitted,
                state: NodeState::Pending,
                decided_by: None,
                integrated: false,
                rank: observation.rank.clone(),
            };
            db.execute(
                "INSERT INTO node_identity(team,id,task_project,task_uuid) VALUES(?1,?2,?3,?4)",
                params![team.id, id, task.project, task.uuid],
            )
            .map_err(storage)?;
            db.execute("INSERT INTO nodes(team,id,data,rank,admitted,routed,task_revision,task_stage,task_cancelled) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0)",params![team.id,id,rows::node_payload(&node)?,observation.rank,admitted,routed,observation.revision,encode(&observation.stage)?.trim_matches('"')]).map_err(storage)?;
            db.execute(
                "UPDATE teams SET next_node=next_node+1 WHERE id=?1",
                [&team.id],
            )
            .map_err(storage)?;
            reply.node = Some(id);
        }
        Operation::Admit { node } => {
            active_team(team)?;
            let n = live_node(db, team, *node)?;
            if n.admitted {
                return Err(invalid("Task is already admitted."));
            }
            db.execute(
                "UPDATE nodes SET admitted=1 WHERE team=?1 AND id=?2",
                params![team.id, node],
            )
            .map_err(storage)?;
        }
        Operation::Route {
            node,
            role,
            pipeline,
            deps,
            writes,
        } => {
            active_team(team)?;
            let mut n = live_node(db, team, *node)?;
            if !matches!(n.state, NodeState::Waiting | NodeState::Pending) {
                return Err(invalid("Only waiting or pending tasks may be routed."));
            }
            route_valid(team, role, pipeline)?;
            if deps.len() > 200 || writes.len() > 200 {
                return Err(over("Too many dependencies or write paths."));
            }
            let mut seen = std::collections::BTreeSet::new();
            for dep in deps {
                if dep.node == *node
                    || !seen.insert(dep.node)
                    || rows::node(db, &team.id, dep.node)?.is_none()
                {
                    return Err(invalid(
                        "A dependency is missing, duplicate or self-referencing.",
                    ));
                }
                let cycle:bool=db.query_row("WITH RECURSIVE ancestors(id) AS (SELECT dependency FROM dependencies WHERE team=?1 AND node=?2 UNION SELECT d.dependency FROM dependencies d JOIN ancestors a ON d.node=a.id WHERE d.team=?1) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?3)",params![team.id,dep.node,node],|r|r.get(0)).map_err(storage)?;
                if cycle {
                    return Err(Problem::new(
                        code::DEPENDENCY_CYCLE,
                        "Dependencies form a cycle.",
                    ));
                }
            }
            for path in writes {
                text(path, 4096)?;
            }
            n.role = role.clone();
            n.pipeline = pipeline.clone();
            n.writes = writes.clone();
            db.execute(
                "UPDATE nodes SET data=?3,routed=1 WHERE team=?1 AND id=?2",
                params![team.id, node, rows::node_payload(&n)?],
            )
            .map_err(storage)?;
            db.execute(
                "DELETE FROM dependencies WHERE team=?1 AND node=?2",
                params![team.id, node],
            )
            .map_err(storage)?;
            for dep in deps {
                db.execute(
                    "INSERT INTO dependencies(team,node,dependency,kind) VALUES(?1,?2,?3,?4)",
                    params![team.id, node, dep.node, encode(&dep.kind)?],
                )
                .map_err(storage)?;
            }
        }
        Operation::Hold { node } | Operation::Release { node } => {
            active_team(team)?;
            let n = live_node(db, team, *node)?;
            let hold = matches!(op, Operation::Hold { .. });
            if !hold {
                let requested: Option<String> = db
                    .query_row(
                        "SELECT requested FROM nodes WHERE team=?1 AND id=?2",
                        params![team.id, node],
                        |r| r.get(0),
                    )
                    .map_err(storage)?;
                if requested
                    .as_deref()
                    .is_some_and(|r| matches!(r, "accept" | "cancel"))
                {
                    return Err(invalid("A task-writer decision is still pending."));
                }
            }
            if n.hold == hold {
                return Err(invalid("Task already has that hold state."));
            }
            db.execute(
                "UPDATE nodes SET hold=?3 WHERE team=?1 AND id=?2",
                params![team.id, node, hold],
            )
            .map_err(storage)?;
        }
        Operation::Cancel { node, reason: why } => {
            active_team(team)?;
            live_node(db, team, *node)?;
            reason(why)?;
            decision(db, actor, team, *node, "cancel", why)?;
            db.execute(
                "UPDATE nodes SET hold=1 WHERE team=?1 AND id=?2",
                params![team.id, node],
            )
            .map_err(storage)?;
        }
        Operation::HandoutIntent {
            node,
            member: id,
            effect_request_id,
        } => {
            if team.status != TeamStatus::Running {
                return Err(invalid("A paused or stopped team cannot hand out work."));
            }
            let mut n = live_node(db, team, *node)?;
            let m = member(db, team, id)?;
            if n.state != NodeState::Pending
                || n.hold
                || m.state != MemberState::Active
                || m.session.is_none()
                || m.id == team.lead
                || m.role != n.role
                || (n.kind == NodeKind::Stage && n.implementer.as_ref() == Some(id))
                || n.assignee.as_ref().is_some_and(|a| a != id)
            {
                return Err(invalid("Task or member cannot take this hand-out."));
            }
            uuid(effect_request_id)?;
            let occupied:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND (node=?2 OR member=?3) AND state IN ('reserved','submitted','uncertain','cutting_over'))",params![team.id,node,id],|r|r.get(0)).map_err(storage)?;
            if occupied {
                return Err(invalid("Task or member has an unresolved hand-out."));
            }
            n.assignee = Some(id.clone());
            if n.kind != NodeKind::Stage {
                n.implementer = Some(id.clone());
            }
            db.execute(
                "UPDATE nodes SET data=?3,requested=NULL WHERE team=?1 AND id=?2",
                params![team.id, node, rows::node_payload(&n)?],
            )
            .map_err(storage)?;
            db.execute("INSERT INTO effects(team,node,member,generation,request_id,actor,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![team.id,node,id,generation(db,team,*node)?,effect_request_id,actor_json,now]).map_err(storage)?;
            reply.node = Some(*node);
            reply.item = Some(db.last_insert_rowid() as u64);
        }
        Operation::HandoutOutcome {
            effect_request_id,
            outcome,
        } => {
            let (node, old, g): (u32, String, u32) = db
                .query_row(
                    "SELECT node,state,generation FROM effects WHERE team=?1 AND request_id=?2",
                    params![team.id, effect_request_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(storage)?
                .ok_or_else(missing)?;
            if old != "reserved" || g != generation(db, team, node)? {
                return Err(invalid(
                    "Hand-out is already settled or belongs to an older attempt.",
                ));
            }
            let (state, why) = match outcome {
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
                    ("cutting_over", Some(r))
                }
                HandoutOutcome::Uncertain { reason: r } => {
                    reason(r)?;
                    ("uncertain", Some(r))
                }
            };
            db.execute(
                "UPDATE effects SET state=?3,outcome=?4 WHERE team=?1 AND request_id=?2",
                params![team.id, effect_request_id, state, encode(outcome)?],
            )
            .map_err(storage)?;
            if let Some(why) = why {
                reply.item = Some(inbox(
                    db,
                    actor,
                    team,
                    node,
                    "delivery_uncertain",
                    why,
                    now,
                )?);
            }
        }
        Operation::ObserveTask { node, observation } => {
            observe(db, team, *node, observation)?;
        }
        Operation::RecordReport {
            node,
            generation: g,
            observation,
            text: body,
            commit,
        } => {
            active_team(team)?;
            let n = live_node(db, team, *node)?;
            own_node(actor, &n)?;
            if *g != generation(db, team, *node)?
                || observation.stage != TaskStage::InReview
                || observation.cancelled
            {
                return Err(invalid("Report is not for the current reviewed attempt."));
            }
            let has_attempt:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM effects WHERE team=?1 AND node=?2 AND generation=?3 AND state IN ('reserved','submitted','uncertain','cutting_over'))",params![team.id,node,g],|r|r.get(0)).map_err(storage)?;
            if !has_attempt {
                return Err(invalid("No current hand-out is available to report."));
            }
            reason(body)?;
            text(commit, 256)?;
            observe(db, team, *node, observation)?;
            db.execute("INSERT INTO reports(team,node,generation,actor,text,commit_id) VALUES(?1,?2,?3,?4,?5,?6)",params![team.id,node,g,actor_json,body,commit]).map_err(storage)?;
            db.execute("UPDATE effects SET state='reported' WHERE team=?1 AND node=?2 AND generation=?3 AND state IN ('reserved','submitted','uncertain','cutting_over')",params![team.id,node,g]).map_err(storage)?;
            reply.item = Some(inbox(db, actor, team, *node, "report", body, now)?);
        }
        Operation::Accept {
            node,
            integration_commit,
        } => {
            active_team(team)?;
            let n = live_node(db, team, *node)?;
            if n.state != NodeState::Reported {
                return Err(invalid("Only a reported task may be accepted."));
            }
            let needs_merge = team.settings.integration == Integration::Branch
                && team.settings.delegations.merge_accepted
                && n.kind != NodeKind::Stage;
            let receipt = integration_commit.as_ref().filter(|_| needs_merge);
            if needs_merge && receipt.is_none_or(|s| s.is_empty()) {
                return Err(invalid(
                    "Acceptance requires the integration actor's successful commit receipt.",
                ));
            }
            if let Some(commit) = receipt {
                text(commit, 256)?;
            }
            decision(
                db,
                actor,
                team,
                *node,
                "accept",
                receipt.map(String::as_str).unwrap_or(""),
            )?;
            let decider = if matches!(actor, Actor::Person { .. }) {
                Decider::Person
            } else {
                Decider::Lead
            };
            db.execute("UPDATE nodes SET decided_by=?3,integrated=?4,integration_commit=?5,hold=1 WHERE team=?1 AND id=?2",params![team.id,node,encode(&decider)?,receipt.is_some(),receipt]).map_err(storage)?;
            db.execute(
                "UPDATE inbox SET state='closed' WHERE team=?1 AND node=?2 AND state='open'",
                params![team.id, node],
            )
            .map_err(storage)?;
        }
        Operation::Return { node, reason: why }
        | Operation::Reassign {
            node, reason: why, ..
        } => {
            active_team(team)?;
            let mut n = live_node(db, team, *node)?;
            reason(why)?;
            if matches!(op, Operation::Return { .. })
                && (n.state != NodeState::Reported || n.assignee.is_none())
            {
                return Err(invalid("Only a reported, assigned task can be returned."));
            }
            if let Operation::Reassign { member: id, .. } = op {
                let m = member(db, team, id)?;
                if m.state != MemberState::Active
                    || m.id == team.lead
                    || m.role != n.role
                    || (n.kind == NodeKind::Stage && n.implementer.as_ref() == Some(id))
                {
                    return Err(invalid("The replacement member cannot take this task."));
                }
                n.assignee = Some(id.clone());
            }
            decision(
                db,
                actor,
                team,
                *node,
                if matches!(op, Operation::Return { .. }) {
                    "return"
                } else {
                    "reassign"
                },
                why,
            )?;
            n.round = n
                .round
                .checked_add(1)
                .ok_or_else(|| over("Task rounds are exhausted."))?;
            db.execute("UPDATE nodes SET data=?3,generation=generation+1,hold=1,decided_by=NULL,integrated=0,integration_commit=NULL WHERE team=?1 AND id=?2",params![team.id,node,rows::node_payload(&n)?]).map_err(storage)?;
            // Supersession releases the old slot, retaining the original
            // uncertain receipt forever. No original effect is replayed.
            db.execute("UPDATE effects SET state='superseded' WHERE team=?1 AND node=?2 AND state IN ('reserved','submitted','uncertain','cutting_over')",params![team.id,node]).map_err(storage)?;
            db.execute(
                "UPDATE inbox SET state='closed' WHERE team=?1 AND node=?2 AND state='open'",
                params![team.id, node],
            )
            .map_err(storage)?;
        }
        Operation::Ask {
            node,
            generation: g,
            text: body,
        } => {
            active_team(team)?;
            let n = live_node(db, team, *node)?;
            own_node(actor, &n)?;
            if *g != generation(db, team, *node)?
                || !matches!(n.state, NodeState::InProgress | NodeState::Reported)
            {
                return Err(invalid("A question needs a current active attempt."));
            }
            reason(body)?;
            let count:u32=db.query_row("SELECT count(*) FROM inbox WHERE team=?1 AND node=?2 AND generation=?3 AND kind LIKE 'question:%' AND state='open'",params![team.id,node,g],|r|r.get(0)).map_err(storage)?;
            if count >= team.settings.caps.max_open_questions {
                return Err(over("Attempt has reached its open question cap."));
            }
            let next: u64 = db
                .query_row("SELECT COALESCE(MAX(seq),0)+1 FROM inbox", [], |r| r.get(0))
                .map_err(storage)?;
            reply.item = Some(inbox(
                db,
                actor,
                team,
                *node,
                &format!("question:{next}"),
                body,
                now,
            )?);
        }
        Operation::Reply { item, text: body } => {
            active_team(team)?;
            reason(body)?;
            let changed=db.execute("UPDATE inbox SET reply=?3,reply_actor=?4,state='closed' WHERE team=?1 AND seq=?2 AND state='open' AND kind LIKE 'question:%'",params![team.id,item,body,actor_json]).map_err(storage)?;
            if changed != 1 {
                return Err(invalid("Question is missing or already answered."));
            }
        }
        Operation::OpenCard {
            kind,
            text: body,
            node,
        } => {
            active_team(team)?;
            reason(kind)?;
            text(kind, 128)?;
            reason(body)?;
            if let Some(node) = node {
                rows::node(db, &team.id, *node)?.ok_or_else(missing)?;
            }
            db.execute(
                "INSERT INTO cards(team,kind,node,actor,text,created_ms) VALUES(?1,?2,?3,?4,?5,?6)",
                params![team.id, kind, node, actor_json, body, now],
            )
            .map_err(storage)?;
            reply.item = Some(db.last_insert_rowid() as u64);
        }
        Operation::DecideCard {
            card,
            decision: answer,
        } => {
            active_team(team)?;
            reason(answer)?;
            let changed=db.execute("UPDATE cards SET decision=?3,decided_actor=?4,state='closed' WHERE team=?1 AND seq=?2 AND state='open'",params![team.id,card,answer,actor_json]).map_err(storage)?;
            if changed != 1 {
                return Err(invalid("Card is missing or already decided."));
            }
        }
        Operation::ChangeSettings { settings: next } => {
            active_team(team)?;
            settings(next)?;
            let count: u32 = db
                .query_row(
                    "SELECT count(*) FROM nodes WHERE team=?1",
                    [&team.id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            let members: u32 = db
                .query_row(
                    "SELECT count(*) FROM members WHERE team=?1 AND id<>?2 AND state<>?3",
                    params![team.id, team.lead, encode(&MemberState::Removed)?],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if next.caps.max_open_tasks < count || next.caps.max_members < members {
                return Err(over("The new caps are below the current live set."));
            }
            if next == &team.settings {
                return Err(invalid("Settings are unchanged."));
            }
            if next.queue != team.settings.queue {
                return Err(invalid(
                    "Queue enablement is fixed when the team is created.",
                ));
            }
            db.execute(
                "UPDATE teams SET settings=?2 WHERE id=?1",
                params![team.id, encode(next)?],
            )
            .map_err(storage)?;
        }
    }
    Ok(reply)
}
fn route_valid(team: &Team, role: &str, pipeline: &str) -> Result<(), Problem> {
    if !team.bundle.roles.iter().any(|r| r.id == role) {
        return Err(Problem::new(code::UNKNOWN_ROLE, "Unknown role."));
    }
    let p = team
        .bundle
        .pipelines
        .iter()
        .find(|p| p.id == pipeline)
        .ok_or_else(|| Problem::new(code::UNKNOWN_PIPELINE, "Unknown pipeline."))?;
    if p.implementer != role {
        return Err(invalid("The role does not implement this pipeline."));
    }
    Ok(())
}
