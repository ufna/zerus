//! The service alone applies the team's folder-trust and self-exit delegations.
//! Every original is reserved durably and never reconstructed from saved data.
//! A person quitting the agent in its terminal looks like a clean exit and is
//! resumed. Use Stop member to stop it; three resumes per hour bound a tug of war.
mod engine;
mod ledger;
mod native;
mod policy;
mod ports;
mod scan;
mod types;
pub(super) use scan::step;
mod resumed;
#[allow(unused_imports)] // The agreed package-8 availability port consumes this seam.
pub(super) use resumed::{resumed_binding, ResumedBinding};
