//! Read-only preflight of caller-supplied bytes, never file freshness, writer
//! quiet, a current native mapping or a dispatch permit. No paths or I/O.
//! This structural subset does not implement provenance/invalidation, unknown
//! issued-result, cross-file commit/restart or permission-fencing checks. Success
//! cannot be used as complete optional preparation or numeric-ID authority.
use super::{decode_owned, quarantine_legacy, OwnedSaved, Refused, FILE_LIMIT, LIMIT};
use crate::native_notice::Owner;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Agent,
    Update,
    Legacy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    TooLarge,
    Invalid { slot: Slot, reason: Refused },
    UnknownFields(Slot),
    WrongOwner(Slot),
    DuplicatedLegacy(Slot),
    CrossOwnerCollision,
}
/// Roles are explicit inputs, never inferred from a path or token. None means
/// only caller-reported absence, not a verified file or source-presence proof.
pub struct Inputs<'a> {
    pub agent: Option<&'a [u8]>,
    pub update: Option<&'a [u8]>,
    pub legacy: Option<&'a [u8]>,
}
pub struct Inspection {
    agent: Option<OwnedSaved>,
    update: Option<OwnedSaved>,
    legacy: Option<OwnedSaved>,
    hashes: [Option<String>; 3],
    raw_entries: usize,
    raw_bytes: usize,
}
impl std::fmt::Debug for Inspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OwnerStoreInspection([supplied data])")
    }
}
impl Inspection {
    pub fn data(&self, slot: Slot) -> Option<&OwnedSaved> {
        match slot {
            Slot::Agent => self.agent.as_ref(),
            Slot::Update => self.update.as_ref(),
            Slot::Legacy => self.legacy.as_ref(),
        }
    }
    /// Hash of the supplied bytes only. A later handoff must securely read the
    /// then-current source and bind its presence/hash under G1's exact protocol.
    /// Never silently refresh or reuse a legacy view from a previous handoff.
    pub fn sha256(&self, slot: Slot) -> Option<&str> {
        self.hashes[match slot {
            Slot::Agent => 0,
            Slot::Update => 1,
            Slot::Legacy => 2,
        }]
        .as_deref()
    }
    pub fn raw_entries(&self) -> usize {
        self.raw_entries
    }
    pub fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }
}
fn owned(
    bytes: Option<&[u8]>,
    slot: Slot,
    expected: Owner,
) -> Result<Option<OwnedSaved>, Rejected> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let value = decode_owned(bytes).map_err(|reason| Rejected::Invalid { slot, reason })?;
    if value.has_unknown() {
        return Err(Rejected::UnknownFields(slot));
    }
    if value.legacy_uncertain().len() != 0 {
        return Err(Rejected::DuplicatedLegacy(slot));
    }
    if value
        .bindings()
        .map(|(_, token)| token)
        .chain(value.uncertain())
        .any(|token| token.owner() != expected)
    {
        return Err(Rejected::WrongOwner(slot));
    }
    Ok(Some(value))
}
/// Reject structural ambiguity without discarding, rewriting or repairing any
/// caller-owned evidence. Success supplies data only; it cannot activate an ID.
pub fn inspect(inputs: Inputs<'_>) -> Result<Inspection, Rejected> {
    let originals = [inputs.agent, inputs.update, inputs.legacy];
    let raw_bytes = originals
        .iter()
        .flatten()
        .try_fold(0_usize, |total, bytes| {
            total
                .checked_add(bytes.len())
                .filter(|n| *n as u64 <= FILE_LIMIT)
                .ok_or(Rejected::TooLarge)
        })?;
    let agent = owned(inputs.agent, Slot::Agent, Owner::AgentAttention)?;
    let update = owned(inputs.update, Slot::Update, Owner::SoftwareUpdate)?;
    let legacy = inputs
        .legacy
        .map(quarantine_legacy)
        .transpose()
        .map_err(|reason| Rejected::Invalid {
            slot: Slot::Legacy,
            reason,
        })?;
    if legacy.as_ref().is_some_and(OwnedSaved::has_unknown) {
        return Err(Rejected::UnknownFields(Slot::Legacy));
    }
    let raw_entries = [&agent, &update, &legacy]
        .into_iter()
        .flatten()
        .map(OwnedSaved::raw_entries)
        .sum();
    if raw_entries > LIMIT {
        return Err(Rejected::TooLarge);
    }
    if let Some((a, u)) = agent.as_ref().zip(update.as_ref()) {
        if let Some((a_ns, u_ns)) = a.namespace().zip(u.namespace()) {
            // Refuse hex-case ambiguity too, without normalizing the original
            // data or treating a spelling as current native authority.
            if a_ns.bus_id.eq_ignore_ascii_case(&u_ns.bus_id)
                && a_ns.owner == u_ns.owner
                && a.bindings.keys().any(|id| u.bindings.contains_key(id))
            {
                return Err(Rejected::CrossOwnerCollision);
            }
        }
    }
    let hashes = originals.map(|value| value.map(|bytes| format!("{:x}", Sha256::digest(bytes))));
    Ok(Inspection {
        agent,
        update,
        legacy,
        hashes,
        raw_entries,
        raw_bytes,
    })
}

#[cfg(test)]
mod tests;
