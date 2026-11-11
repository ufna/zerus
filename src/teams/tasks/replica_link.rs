//! Local intake and guards use the team store. Optional replicated metadata
//! is added separately, after the replica-owner check-in.
use super::*;

pub(super) struct Link {
    pub team: String,
    pub home: String,
}

pub(super) fn read(_document: &TaskDoc) -> Result<Option<Link>> {
    Ok(None)
}
pub(super) fn write(
    _document: &mut TaskDoc,
    _team: &Team,
    _who: &TaskActor,
    _now: i64,
) -> Result<()> {
    Ok(())
}
