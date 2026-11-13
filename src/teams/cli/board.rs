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
            // zerus-63rru.52: for a stopped or archived team the row says in
            // plain words what the stop did to its node, read from the
            // store's own stop facts, never from the node's state. A live
            // team's rows carry no field.
            if !matches!(
                team.status,
                zerus_team_plan::model::TeamStatus::Running
                    | zerus_team_plan::model::TeamStatus::Paused
            ) {
                value["stop_outcome"] = json!(stop_sentence(row.stop.as_ref()));
            }
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
/// The stop's outcome for one node, in the words the queue row and the
/// desktop show (zerus-63rru.52).
fn stop_sentence(facts: Option<&crate::teams::store::StopFacts>) -> String {
    let cancel = facts.and_then(|f| f.cancel.as_deref());
    let key = facts.and_then(|f| f.key.as_deref());
    // The newest recorded fact wins: the key clear follows the cancel, so
    // its record supersedes the cancel's in the row's words.
    match (key, facts.and_then(|f| f.key_reason.as_deref()), cancel, facts.and_then(|f| f.cancel_reason.as_deref()), facts.is_some_and(|f| f.pending_cancel)) {
        (Some("refused"), Some(reason), _, _, _) => {
            format!("The team stop's key clear was refused: {reason}")
        }
        (Some("cleared"), _, _, _, _) => "The team stop cleared this task's key".to_owned(),
        (_, _, Some("refused"), Some(reason), _) => {
            format!("The team stop's cancel was refused: {reason}")
        }
        (_, _, Some("cancelled"), _, _) => "The team stop cancelled this task".to_owned(),
        (_, _, Some("released"), _, _) => "The team stop released this task".to_owned(),
        (None, _, None, _, true) => "The team stop has not reached this task yet".to_owned(),
        (None, _, None, _, false) => "The team stop left this finished task as it was".to_owned(),
        _ => "The team stop has not reached this task yet".to_owned(),
    }
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
