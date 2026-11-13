//! Additive board metadata from one already bounded pinned team bundle.
use super::storage;
use crate::teams::store::{MemberRow, Page, QueueRow};
use serde_json::{json, Value};
use zerus_team_plan::{
    model::{NodeKind, Team},
    Problem,
};

pub(super) fn queue(team: &Team, page: Page<QueueRow>) -> Result<Page<Value>, Problem> {
    let rows = page
        .rows
        .into_iter()
        .map(|row| {
            let node = row.node;
            let mut value = serde_json::to_value(&node).map_err(storage)?;
            // What a view needs to foresee a refusal: who holds the attempt
            // and the kind of a pending decision, each null when none, and
            // whether a new Return, Reassign or Cancel would replace that
            // decision (always written; false when nothing is pending).
            value["holder"] = json!(row.holder);
            value["pending_decision"] = json!(row.pending);
            value["pending_replaceable"] = json!(row.pending_replaceable);
            // The lead's open item when no member can take the ready node:
            // its sentence, or null.
            value["unstaffed"] = json!(row.unstaffed);
            value["kind"] = json!(if node.kind == NodeKind::Stage {
                "stage"
            } else {
                "task"
            });
            if let Some(pipeline) = team.bundle.pipelines.iter().find(|p| p.id == node.pipeline) {
                value["rounds"] = json!(pipeline.rounds());
                if node.kind == NodeKind::Stage {
                    if let (Some(target), Some(stage)) = (
                        node.stage_of,
                        pipeline.stages.get(node.stage_index as usize),
                    ) {
                        value["stage"] =
                            json!({"kind":stage.kind,"target":target,"round":node.round});
                    }
                }
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, Problem>>()?;
    Ok(Page {
        rows,
        next: page.next,
    })
}
pub(super) fn members(team: &Team, members: Vec<MemberRow>) -> Result<Vec<Value>, Problem> {
    members
        .into_iter()
        .map(|row| {
            let member = row.member;
            let mut value = serde_json::to_value(&member).map_err(storage)?;
            value["person_stopped"] = json!(row.person_stopped);
            let model = if member.id == team.lead {
                Some(team.bundle.template.lead.model.as_str())
            } else {
                team.bundle
                    .roles
                    .iter()
                    .find(|r| r.id == member.role)
                    .map(|r| r.model.as_str())
            };
            if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
                value["model"] = json!(model);
            }
            Ok(value)
        })
        .collect()
}
