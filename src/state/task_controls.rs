//! Inactive native task-control support. Publication under the native writer
//! never opens the desktop database; import belongs after that writer drops.
#![allow(dead_code)] // Removed as the runtime entry points are connected.
pub(crate) mod observation;
mod stop_facts;
#[cfg(windows)]
#[allow(unused_imports)] // Reserved opaque lease surface; runtime selection stays inactive.
pub(in crate::state) use stop_facts::SelectedLease;
#[cfg(windows)]
pub(in crate::state) use stop_facts::{CaptureFailed, SelectedCapture};
pub(crate) use stop_facts::{Captured, Fact, Source};
pub(in crate::state) mod writer;

/// Existing-run capture only, under the caller's original native exclusion.
/// Missing history refuses; inspection never creates an empty outbox.
#[cfg(not(windows))]
pub(crate) fn capture_stops(
    root: &std::path::Path,
    writer: &super::storage::Guard,
    executor: &zerus_desktop_ledger::task_controls::Executor,
) -> Result<Option<Captured>, String> {
    stop_facts::open(root, writer, executor, false)?
        .map(stop_facts::Publisher::capture)
        .transpose()
}

/// Inactive Windows support: consume the original owner and its original
/// deadline. Data custody alone grants no current-name or actor authority.
#[cfg(windows)]
pub(in crate::state) fn capture_stops(
    owned: writer::Writer,
    executor: &zerus_desktop_ledger::task_controls::Executor,
) -> Result<SelectedCapture, CaptureFailed> {
    stop_facts::capture_owned(owned, executor)
}
