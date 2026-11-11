//! Member creation is owned by the desktop service. All session effects use
//! the shared original-child runner and maintained CLI; stored plans are data,
//! never authority to continue an original after a restart.
#![cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod child;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use child::adopt_custody;
mod launch;
mod ledger;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
mod ports;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod worker;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) use worker::Worker;
#[cfg(test)]
mod tests;
