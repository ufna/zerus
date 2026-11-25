//! Agent team types and the pure rules that decide what may run.
//!
//! A team is a lead session, member sessions with roles, and their work: a
//! plan of phases, a queue of tasks sent one by one, or both. A team task is a
//! Zerus task; this crate holds the structure a team adds around it.
//!
//! Nothing here reads a file, a clock or a session. Callers pass state in and
//! get decisions back, so every rule can be tested without a running agent.
//!
//! Agent Teams were designed and first built by WhiteWh in the `WhiteWh/zerus`
//! fork. See `docs/superpowers/specs/2026-11-11-agent-teams-design.md`.

pub mod accounts;
pub mod builtin;
pub mod catalog;
pub mod draft;
pub mod graph;
pub mod member;
pub mod model;
pub mod observed_timing;
pub mod overlap;
pub mod phase_control;
pub mod pipeline;
pub mod problem;
pub mod ready;
pub mod settings;
#[cfg(test)]
mod testkit;
pub mod validate;
pub mod validation;

pub use builtin::builtin;
pub use catalog::{
    AccountChoice, Catalog, LeadSpec, Pipeline, Role, Seat, Stage, StageKind, Template,
    TemplateBundle,
};
pub use model::{
    Acceptance, Approval, Caps, ContextMode, Decider, Delegations, Dep, DepKind, Integration,
    Member, MemberId, MemberState, Node, NodeId, NodeKind, NodeSource, NodeState, PersonId, Phase,
    PhaseKind, PhaseMode, PhaseState, Routing, SessionKey, Settings, Severity, TaskRef, Team,
    TeamFixture, TeamId, TeamStatus, Timers,
};
pub use problem::Problem;
pub use validate::{
    admission_problems, bundle, forbidden_char, id_problem, name_problem, validate_bundle,
    validate_catalog, validate_ids,
};

/// True when a value equals its type's default; used to keep JSON short.
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// The same short plain text is shown in the native prompt and stored verbatim.
pub fn lead_message(value: &str) -> Result<(), Problem> {
    if value.trim().is_empty() {
        return Err(Problem::new(problem::code::INVALID_STATE, "Enter a message for the lead."));
    }
    if value.len() > 4_000 || value.chars().count() > 1_000 {
        return Err(Problem::new(problem::code::OVER_CAP, "Use up to 1,000 characters and 4,000 bytes."));
    }
    if value.split('\n').any(|line| {
        forbidden_char(line).is_some()
            || line.chars().any(|c| matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}'))
    }) {
        return Err(Problem::new(problem::code::INVALID_STATE, "The message contains unsupported control characters."));
    }
    Ok(())
}
