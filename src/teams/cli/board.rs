//! Additive board metadata from one already bounded pinned team bundle.
use super::storage;
use crate::teams::store::Page;
use serde_json::{json, Value};
use zerus_team_plan::{
    model::{Member, Node, NodeKind, Team},
    Problem,
};

pub(super) fn queue(team: &Team, page: Page<Node>) -> Result<Page<Value>, Problem> {
    let rows = page
        .rows
        .into_iter()
        .map(|node| {
            let mut value = serde_json::to_value(&node).map_err(storage)?;
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
pub(super) fn members(team: &Team, members: Vec<Member>) -> Result<Vec<Value>, Problem> {
    members
        .into_iter()
        .map(|member| {
            let mut value = serde_json::to_value(&member).map_err(storage)?;
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
