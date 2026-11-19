//! Explicit native-original model for producer-backed unit fixtures. This is
//! not a window gesture, and it is not reachable in a production executable.
use super::*;
use crate::{config::Config, desktop::ui_authority::Validated, teams::store};
use std::sync::{atomic::AtomicBool, Arc};
use zerus_client_core::desktop::{creation_clone::Scope, team_person_actions as wire};

pub(crate) fn intake(
    config: &Config,
    draft: wire::TaskDraft,
) -> Result<serde_json::Value, zerus_team_plan::Problem> {
    let scope = Scope {
        configuration: "a".repeat(64),
        service_epoch: uuid::Uuid::new_v4().to_string(),
    };
    let value = intake::review(config, &scope, draft)?;
    let plan: wire::Plan = serde_json::from_value(value["plan"].clone()).map_err(store::storage)?;
    let review: wire::Review =
        serde_json::from_value(value["review"].clone()).map_err(store::storage)?;
    let prepared = Arc::new(prepared::Prepared::resolve(config, Arc::new(plan))?);
    let authority =
        Validated::fixture(&serde_json::json!({"op":"team_person_action","intent":review.intent}));
    intake::execute(
        config,
        scope,
        Arc::new(AtomicBool::new(false)),
        authority,
        prepared,
    )
}

/// A team action as the window confirms it, through the service's own Person
/// action runtime: plan staging, the review that builds the intent (an absent
/// store's birth binding for a first Create included), and the confirmed
/// action's worker with its preflight, original and writer. Only the window's
/// proof is modeled (`Validated::fixture`); the worker's owner is this
/// model's own lock, never a running service's.
pub(crate) fn team_action(
    config: &Config,
    plan: &wire::Plan,
) -> Result<serde_json::Value, zerus_team_plan::Problem> {
    use crate::{desktop::fanout::Queue, platform};
    use serde_json::json;
    let scope = Scope {
        configuration: "a".repeat(64),
        service_epoch: uuid::Uuid::new_v4().to_string(),
    };
    let lock = tempfile::tempfile().map_err(store::storage)?;
    let owner = Arc::new(platform::FileLock::exclusive(lock).map_err(store::storage)?);
    let mut runtime = Runtime::new(
        config,
        scope.clone(),
        owner.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    let reference = plan.reference().ok_or_else(required)?;
    let body = serde_json::to_vec(plan).map_err(store::storage)?;
    for (ordinal, bytes) in body.chunks(wire::PIECE_BYTES).enumerate() {
        let piece = wire::Piece::from_bytes(
            uuid::Uuid::new_v4().to_string(),
            scope.clone(),
            reference.clone(),
            u16::try_from(ordinal).map_err(|_| required())?,
            bytes,
        )
        .ok_or_else(required)?;
        let staged = runtime.stage(Validated::fixture(
            &json!({"op":"team_person_plan_piece","piece":piece}),
        ));
        if staged["ok"] != true {
            return Ok(staged);
        }
    }
    let answer = |queue: &Queue| -> Result<serde_json::Value, zerus_team_plan::Problem> {
        let frame = queue.next().ok_or_else(required)?;
        let frame: serde_json::Value = serde_json::from_slice(&frame).map_err(store::storage)?;
        Ok(frame["value"].clone())
    };
    let queue = Arc::new(Queue::default());
    runtime.query(
        &json!({"op":"team_person_action_prepare","scope":scope,"plan":reference}),
        queue.clone(),
        1,
    )?;
    let reviewed = answer(&queue)?;
    if reviewed["ok"] != true {
        return Ok(reviewed);
    }
    let review: wire::Review =
        serde_json::from_value(reviewed["review"].clone()).map_err(store::storage)?;
    let queue = Arc::new(Queue::default());
    runtime.action(
        Validated::fixture(&json!({"op":"team_person_action","intent":review.intent})),
        queue.clone(),
        2,
    )?;
    Ok(answer(&queue)?["reply"].clone())
}
