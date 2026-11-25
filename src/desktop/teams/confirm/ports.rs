//! Every session effect and durable boundary is explicit for tests.
use super::types::*;

pub(super) type Result<T> = std::result::Result<T, String>;

pub(super) trait Ports: Sized {
    fn facts(&mut self, binding: &Binding) -> Result<Facts>;
    fn observe(&mut self, binding: &Binding) -> Result<Snapshot>;
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
