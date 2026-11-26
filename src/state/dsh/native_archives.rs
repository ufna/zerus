//! DeepSeek native archives keep their existing private store and wire format.
use super::*;
#[cfg(test)]
use std::time::Instant;

#[cfg(test)]
mod tests;
#[cfg(test)]
use super::super::native_archive_store::open;

const FILE: &str = "dsh/archives-v1.sqlite3";
pub(in crate::state) const STORE: native_archive_store::Store =
    native_archive_store::Store::new(FILE, "DeepSeek", |record| {
        record["agent"] == "dsh"
            && record["backend"] == "dsh"
            && string(record, "name").starts_with("dsh/")
    });

pub(super) fn contains(id: &str) -> Result<bool> {
    STORE.contains(id)
}
pub(in crate::state) fn by_id(id: &str) -> Result<Option<Value>> {
    STORE.by_id(id)
}
pub(super) fn publish(record: &Value) -> Result<String> {
    STORE.publish(record)
}
pub(in crate::state) fn mark_worktree_removal(record: &Value, cleanup: &Value) -> Result<()> {
    STORE.mark_worktree_removal(record, cleanup)
}
pub(in crate::state) fn update_name(record: &Value) -> Result<()> {
    STORE.update_name(record)
}
pub(in crate::state) fn remove(record: &Value) -> Result<()> {
    STORE.remove(record)
}
#[cfg(test)]
pub(in crate::state) fn visit_references(
    until: Instant,
    max_rows: usize,
    max_bytes: usize,
    visit: impl FnMut(Value) -> Result<()>,
) -> Result<()> {
    STORE
        .visit_references(until, max_rows, max_bytes, visit)
        .map(|_| ())
}
