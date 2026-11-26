//! Every session effect and durable boundary is explicit for tests.
use super::types::*;

pub(super) type Result<T> = std::result::Result<T, String>;

pub(super) enum ValidationError {
    ArchiveCoverage(String),
    Retired(ObservationDisposition),
    Refused(String),
}

impl ValidationError {
    pub fn into_detail(self) -> String {
        match self {
            Self::ArchiveCoverage(detail) | Self::Refused(detail) => detail,
            Self::Retired(_) => "The observation does not admit a standing confirmation".into(),
        }
    }
}

impl From<String> for ValidationError {
    fn from(detail: String) -> Self {
        Self::Refused(detail)
    }
}

impl From<&str> for ValidationError {
    fn from(detail: &str) -> Self {
        Self::Refused(detail.into())
    }
}

pub(super) trait Ports: Sized {
    fn facts(&mut self, binding: &Binding) -> Result<Facts>;
    fn observe(&mut self, binding: &Binding) -> Result<Snapshot>;
    fn observe_for_validation(
        &mut self,
        binding: &Binding,
    ) -> std::result::Result<Snapshot, ValidationError> {
        self.observe(binding).map_err(ValidationError::Refused)
    }
    fn archive_coverage_deferred(&mut self, _detail: &str) {}
    fn retire_observation(
        &mut self,
        _binding: &Binding,
        _disposition: ObservationDisposition,
    ) -> Result<()> {
        Ok(())
    }
    /// None means an exact manual archive or a disproved projected exit.
    fn observe_member(&mut self, binding: &Binding) -> Result<Option<Snapshot>> {
        self.observe(binding).map(Some)
    }
    fn ownership(&mut self, binding: &Binding) -> Result<Ownership>;
    fn unsettled(&mut self, binding: &Binding) -> Result<Option<Record>>;
    fn pending_brief(&mut self, binding: &Binding) -> Result<Option<Plan>>;
    fn claim(&mut self, plan: &Plan, epoch: &str, now: i64) -> Result<Claim>;
    fn save(&mut self, row: &Record) -> Result<()>;
    fn audit_begin(&mut self, plan: &Plan) -> Result<Audit>;
    fn audit_finish(&mut self, row: &Record) -> Result<()>;
    fn card(&mut self, row: &Record) -> Result<()>;
    /// Just before a resume of a Codex member under the trust setting:
    /// Codex's trust recorded for exactly the covered pending hooks
    /// (`hook_trust::before_start`). Its outcome is journaled there.
    fn before_start(&mut self, plan: &Plan);
    /// What a Codex hooks panel the person must answer lists: the pending
    /// hooks Zerus did not trust, named (`hook_trust`).
    fn hook_listing(&mut self, plan: &Plan) -> Result<String>;
    /// Feed lines this member launch owes.
    fn notes_owed(&mut self, binding: &Binding) -> Result<Vec<Note>>;
    /// Writes one owed feed line, once.
    fn note(&mut self, note: &Note) -> Result<()>;
    fn original(&mut self, row: &Record, check: &mut dyn FnMut(&mut Self) -> Result<()>)
        -> Outcome;
}
