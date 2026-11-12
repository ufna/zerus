//! Member creation is owned by the desktop service. All session effects use
//! the shared original-child runner and maintained CLI; stored plans are data,
//! never authority to continue an original after a restart.
#![cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod child;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use child::adopt_custody;
mod actor;
pub(crate) use actor::presence;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod confirm;
mod health;
mod kept;
mod launch;
mod ledger;
pub(crate) use health::Health;
/// Last launch-worker fault, read without creating a launch ledger.
pub(crate) fn health(state: &std::path::Path) -> Option<Health> {
    health::read(state)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
mod ports;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod timers;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod worker;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) use worker::entry_hold::Hold as WorkerEntryHold;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) use worker::Worker;
#[cfg(test)]
mod tests;
