//! Transactional Teams commands. No operation causes an effect in a session.
#![allow(dead_code)] // Consumed by the following CLI and service packages.

mod authority;
mod types;
use super::store::{self, rows, Store};
pub use authority::authority;
pub use types::*;
use zerus_team_plan::{model::*, problem::code, Problem};

mod execute;
#[allow(unused_imports)] // The CLI/service packages consume this entry point next.
pub use execute::apply;

#[cfg(test)]
mod tests;
