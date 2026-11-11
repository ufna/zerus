//! Applies only the stored Pending cancellation, using existing document forms.
use super::*;
use completion::Input;
use zerus_replica::execution::Phase;
use zerus_team_plan::model::NodeState;

fn authority(store: &Store, body: &Input, team: &Team, node: &Node) -> Result<String> {
    if !matches!(
        team.status,
        TeamStatus::Running | TeamStatus::Paused | TeamStatus::Stopped | TeamStatus::Archived
    ) {
        return Err(rule("Team is not running, paused, stopped or archived."));
    }
    if node.state == NodeState::Accepted {
        return Err(rule("An accepted node cannot be cancelled."));
    }
    store.db.as_ref().expect("bound store").query_row(
        "SELECT text FROM decisions WHERE team=?1 AND node=?2 AND generation=(SELECT generation FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT generation FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1) AND kind='cancel' AND state='pending' ORDER BY seq DESC LIMIT 1",
        params![body.team,body.node], |r| r.get(0)).optional().map_err(fail)?
        .ok_or_else(||rule("No Pending cancel decision for this generation."))
}

pub(super) fn write(config: &Config, args: &[String], body: &Input) -> Result<&'static str> {
    let deadline = Instant::now() + attempts::BUDGET;
    let here = identity(config)?;
    let teams = Store::open(&config.state).map_err(problem)?;
    let (team, node, reference) = completion::bound(&teams, "team-cancel", body, &here)?;
    authority(&teams, body, &team, &node)?;
    #[cfg(test)]
    super::stage::before_store(config);
    let mut scoped = args.to_vec();
    if option(args, "--project")?.is_none() {
        scoped.extend(["--project".into(), reference.project.clone()]);
    }
    let r = super::super::reference(config, &scoped, &Actor::Person)?;
    if r.project.0 != reference.project {
        return Err(rule("REF is not this node's project."));
    }
    let tasks = super::super::store_until(config, &reference.project, deadline)?;
    let mut admitted = tasks.admit_until(deadline).ok_or_else(|| fail(BUSY))?;
    teams
        .db
        .as_ref()
        .expect("bound store")
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(fail)?;
    let (team, node, fresh) = completion::bound(&teams, "team-cancel", body, &here)?;
    if fresh != reference {
        return Err(rule(dispatch::CHANGED));
    }
    let reason = authority(&teams, body, &team, &node)?;
    let mut doc = admitted.load_complete(&reference.uuid).map_err(fail)?;
    if r.task != reference.uuid && r.task != doc.id() {
        return Err(rule("REF is not this node's task."));
    }
    let view = doc.view().map_err(fail)?;
    if view.stage == Stage::Done {
        return Err(rule("A Done task cannot be cancelled or released."));
    }
    if doc.team().map_err(fail)?.is_none() && !doc.team_invalid() && view.stage != Stage::Active {
        return Ok(if view.cancelled_at.is_some() {
            "cancelled"
        } else {
            "released"
        });
    }
    marker(&doc, &body.team, &here)?;
    if view.cancelled_at.is_some() {
        return Ok("cancelled");
    }
    if view
        .execution
        .as_ref()
        .is_some_and(|e| !e.unknown.is_empty() || e.conflict)
    {
        return Err(rule(
            "The current delivery is unknown or conflicting; the person must settle it.",
        ));
    }
    if admitted
        .project()
        .map_err(fail)?
        .cutover_hold()
        .is_some_and(|h| h.on)
    {
        return Err(cutting_over());
    }
    let lead = teams
        .member(&body.team, &team.lead)
        .map_err(problem)?
        .and_then(|m| m.session)
        .filter(|s| s.machine == here && !s.session_id.is_empty())
        .ok_or_else(|| rule("Lead has no local session binding."))?;
    let machines = machines(config)?;
    let who = Actor::Agent(
        machines
            .resolve(&SessionKey {
                machine: lead.machine,
                session_id: lead.session_id,
                session: String::new(),
            })
            .ok_or_else(|| rule("Lead session identity is unavailable."))?,
    );
    let active = view.stage == Stage::Active;
    if active {
        let known = view
            .execution
            .as_ref()
            .filter(|e| !e.conflict)
            .and_then(|e| e.current.as_ref())
            .is_some_and(|current| {
                matches!(current.phase, Phase::Submitted | Phase::Running)
                    && current.executor.as_ref() == view.session.as_ref()
                    && current.executor.is_some()
            });
        if !known {
            return Err(rule(
                "The current delivery is unknown; the person must settle it.",
            ));
        }
        doc.record_cancel(&reason, &who, now()).map_err(replica)?;
    } else if matches!(view.stage, Stage::Backlog | Stage::Todo | Stage::Review) {
        crate::teams::tasks::clear_named(
            &mut doc,
            &format!("{} (released by the team's cancellation)", team.name),
            &who,
        )
        .map_err(problem)?;
    } else {
        return Err(rule(
            "Task has no cancellable execution or releasable stage.",
        ));
    }
    admitted
        .save(&mut doc)
        .map_err(|e| fail(format!("{UNCONFIRMED_WRITE} {e}")))?;
    if active {
        let settings = admitted.project().map_err(fail)?.settings();
        let before = doc.revision();
        super::super::cancel_notice(&machines, &mut doc, &r.project.1, &who, &settings, None)
            .map_err(|e| fail(format!("{UNCONFIRMED_WRITE} {}", e.message)))?;
        if doc.revision() != before {
            admitted
                .save(&mut doc)
                .map_err(|e| fail(format!("{UNCONFIRMED_WRITE} {e}")))?;
        }
    }
    #[cfg(test)]
    if std::env::var_os("HGS_TEST_TEAM_COMPLETION_AFTER_SAVE").is_some() {
        std::process::exit(79);
    }
    Ok(if active { "cancelled" } else { "released" })
}
