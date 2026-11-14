//! Dormant, bounded data snapshot of one already-resolved stored record slot.
//! No name lookup, repair, native observation or mutation authority. This keeps
//! a logical source, not an original-object pin, exclusion or native lease.
use super::*;
use crate::state::record_source::Digest;

pub(in crate::state) struct Snapshot {
    slot: PathBuf,
    name: String,
    record: Value,
    source: Digest,
}

impl Snapshot {
    /// The caller resolves the exact legacy/ID slot before entering here. A
    /// relative locator could silently retarget after a cwd change, so refuse
    /// it instead of resolving or repairing anything on the caller's behalf.
    pub(in crate::state) fn capture(slot: &Path, name: &str) -> Result<Self> {
        if !slot.is_absolute() {
            return Err("Selected source requires an already-resolved absolute record slot".into());
        }
        let record = read_metadata_at(slot, name)?;
        let source = Digest::capture(&record);
        Ok(Self {
            slot: slot.to_owned(),
            name: name.to_owned(),
            record,
            source,
        })
    }

    /// Complete unrepaired metadata from the same read that was fingerprinted.
    /// Consumers must prepare from this value, not perform a separate read().
    pub(in crate::state) fn record(&self) -> &Value {
        &self.record
    }

    /// Equality only. Even a match grants no write/input authority, and no
    /// refreshed record is adopted. Missing slots never follow a label holder.
    pub(in crate::state) fn recheck(&self) -> Result<()> {
        let current = read_metadata_at(&self.slot, &self.name)?;
        if !self.source.matches(&current) {
            return Err(
                "Selected stored source changed; refresh the selection before continuing".into(),
            );
        }
        Ok(())
    }
}
