//! A ready node that no member can take (zerus-63rru.45). The plan kernel
//! names these nodes (`ready::unstaffed`); the lead is told once per node and
//! the item goes when a member can take the node again.
//!
//! The service's hand-out pass compares the kernel's set, over its uncooled
//! readiness view, with the lead's open items, and asks for
//! `ReconcileUnstaffed` only when the two differ. The store never takes the
//! service's list: it recomputes the set from its own readiness view, members
//! and open effects through [`unstaffed_of`], the same function, and in one
//! operation tells the lead of each new node and closes the items of the
//! nodes that left the set. An unstaffed stage is retired and its parent
//! handed to the lead with the kernel's reason, as at the stage's creation.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use zerus_team_plan::ready::{self, MemberView, PlanView, Unstaffed};

/// The prefix of an unstaffed item's kind. The item's kind is
/// `unstaffed:<revision>`, the reconciling operation's revision; the inbox
/// keys an item by team, kind, node and generation, so the nodes of one
/// reconcile never share an item.
pub const UNSTAFFED_KIND: &str = "unstaffed:";

/// The ready nodes that no member can take, by the kernel's rule, without
/// the nodes that have an open effect: such a node is in somebody's hands or
/// on its way and is never reconciled. Busy, paused and not yet created
/// members count as members who could take a node, as the kernel says. One
/// function for the store's reconcile and the service's comparison, so the
/// two cannot drift.
pub fn unstaffed_of(
    team: &Team,
    readiness: &Readiness,
    members: &[Member],
    open_effects: &BTreeSet<u32>,
) -> Vec<Unstaffed> {
    let members: Vec<MemberView> = members
        .iter()
        .map(|m| MemberView {
            id: m.id.clone(),
            role: m.role.clone(),
            state: m.state,
            free: false,
        })
        .collect();
    let view = PlanView {
        phases: &readiness.phases,
        nodes: &readiness.nodes,
        members: &members,
        lead: &team.lead,
        integration: team.settings.integration,
    };
    ready::unstaffed(&view)
        .into_iter()
        .filter(|u| !open_effects.contains(&u.node))
        .collect()
}

pub(super) fn reconcile(ctx: &mut Context<'_>) -> Result<()> {
    if ctx.team.status != TeamStatus::Running {
        return Err(invalid(
            "Only a running team's unstaffed nodes are reconciled.",
        ));
    }
    let readiness = readiness::load(ctx.db, &ctx.team.id)?;
    let members = members(ctx)?;
    let effects = store::unstaffed::open_effect_nodes(ctx.db, &ctx.team.id)?;
    let now = unstaffed_of(ctx.team, &readiness, &members, &effects);
    if now.len() > store::unstaffed::MAX_OPEN {
        return Err(over(
            "More than 500 unstaffed nodes: the reconcile fails closed.",
        ));
    }
    let open = store::unstaffed::open_items(ctx.db, &ctx.team.id)?;
    let stages: BTreeSet<u32> = readiness
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Stage)
        .map(|n| n.id)
        .collect();
    let wanted: BTreeMap<u32, &str> = now
        .iter()
        .filter(|u| !stages.contains(&u.node))
        .map(|u| (u.node, u.reason.as_str()))
        .collect();
    let told: BTreeSet<u32> = open.iter().map(|(_, node)| *node).collect();
    let mut changed = false;
    // A node that left the set: a member can take it again, it is no longer
    // ready, or it left the plan.
    for (seq, node) in &open {
        if !wanted.contains_key(node) {
            ctx.db
                .execute(
                    "UPDATE inbox SET state='closed' WHERE team=?1 AND seq=?2",
                    params![ctx.team.id, seq],
                )
                .map_err(storage)?;
            changed = true;
        }
    }
    let revision = ctx
        .team
        .revision
        .checked_add(1)
        .ok_or_else(|| over("Revision is exhausted."))?;
    for (node, reason) in &wanted {
        if told.contains(node) {
            continue;
        }
        let text = format!(
            "Task {node} is ready, but no member can take it: {reason} Route it to another role, reassign or cancel it."
        );
        ctx.inbox(*node, &format!("{UNSTAFFED_KIND}{revision}"), &text)?;
        changed = true;
    }
    for u in now.iter().filter(|u| stages.contains(&u.node)) {
        stage_to_lead(ctx, u.node, &u.reason)?;
        changed = true;
    }
    if !changed {
        return Err(Problem::new(
            code::UNSTAFFED_UNCHANGED,
            "The lead's items already show every unstaffed node; nothing was written.",
        ));
    }
    Ok(())
}

/// An unstaffed stage goes the way a stage nobody could take goes at its
/// creation: it is taken from its pipeline as an override takes it (its
/// creation superseded), retired through the one retirement function, and
/// its parent is handed to the lead with the kernel's reason. A stage that is
/// no longer its parent's current one is retired alone.
fn stage_to_lead(ctx: &mut Context<'_>, stage: u32, reason: &str) -> Result<()> {
    let n = ctx.node(stage)?;
    let parent = n.stage_of.ok_or_else(missing)?;
    let current = pipeline::current_stage(ctx, stage, parent)?;
    ctx.db.execute("UPDATE stage_tasks SET state='superseded' WHERE team=?1 AND node=?2 AND state IN ('wanted','reserved','created')",params![ctx.team.id,stage]).map_err(storage)?;
    pipeline::retire_stage(ctx, stage)?;
    if current {
        let parent = ctx.node(parent)?;
        // The parent's hold while its stage runs is the pipeline's own
        // (`want_stage`); handing the run to the lead ends it, as at the
        // stage's creation.
        pipeline::to_lead(ctx, &parent, reason)?;
    }
    Ok(())
}

/// Every member of the team, bounded as the pipeline's staffing read is.
fn members(ctx: &Context<'_>) -> Result<Vec<Member>> {
    let mut s = ctx
        .db
        .prepare("SELECT data FROM members WHERE team=?1 ORDER BY id LIMIT 602")
        .map_err(storage)?;
    let members = s
        .query_map([&ctx.team.id], |r| store::sql_json::<Member>(r, 0))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if members.len() > 601 {
        return Err(over("Too many members in the unstaffed view."));
    }
    Ok(members)
}
