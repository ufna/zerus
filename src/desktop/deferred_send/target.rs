//! The selected session's actual backend and run, checked last before the
//! original child (zerus-5wmh.66.4.4, slice 3). The session name's agent
//! prefix only routes a send to this kind; it never authorizes executing it.
//!
//! From the held original state root, without following a link
//! ([`crate::desktop::original_store::session_current`]): exactly one of the
//! name's session records, the CLI's own and, where the ID layout exists
//! (zerus-ucck P3a), every session its label index proves, must name this
//! session, a terminal agent, and the plan's run and conversation, with no
//! OpenCode or DeepSeek binding, since `hgs send` routes those names to their
//! own native backends. Anything else, an unreadable record or an unproven
//! index included, refuses before the child: nothing ran.
use super::receipt::Roots;
use zerus_client_core::desktop::deferred_send::Plan;

/// The selected terminal record and its durable identity are captured before the
/// original starts. Legacy records can still send without the terminal proof.
pub(super) fn record(roots: &Roots, plan: &Plan) -> Option<serde_json::Value> {
    crate::desktop::original_store::session_record(
        roots,
        &plan.session,
        &plan.run_id,
        &plan.conversation_id,
    )
}

#[derive(Clone)]
pub(super) struct Identity {
    pub machine_id: String,
    pub session_id: String,
    pub provider: String,
    pub namespace: String,
}

impl Identity {
    pub fn from_record(record: &serde_json::Value, machine_id: &str) -> Option<Self> {
        let session_id = record["session_id"].as_str()?;
        if !uuid::Uuid::parse_str(session_id)
            .is_ok_and(|value| !value.is_nil() && value.to_string() == session_id)
        {
            return None;
        }
        // The CLI derives the binding from this same graph endpoint. Capture
        // its namespace from the original record, using the lease's machine.
        let target = crate::state::session_graph::target_receipt_for_machine(record, machine_id)?;
        let endpoint = &target["endpoint"];
        Some(Self {
            machine_id: machine_id.into(),
            session_id: session_id.into(),
            provider: endpoint["provider"].as_str()?.into(),
            namespace: endpoint["namespace"].as_str()?.into(),
        })
    }
}

/// Whether the plan's session is a terminal agent's, running the plan's run
/// and conversation, with no native binding.
#[cfg(test)]
pub(super) fn current(roots: &Roots, plan: &Plan) -> bool {
    record(roots, plan).is_some()
}
