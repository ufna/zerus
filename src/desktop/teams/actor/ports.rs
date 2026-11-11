use crate::teams::{ops::HandoutOutcome, store::Store};
use zerus_team_plan::{model::*, Problem};
pub(super) type Result<T> = std::result::Result<T, Problem>;

pub(in crate::desktop::teams) trait Ports {
    /// Journal hints only; this read must not decode any task document.
    fn changes(
        &mut self,
        project: &str,
        generation: &str,
        offset: u64,
    ) -> Result<zerus_replica::Changes>;
    /// The trusted adapter reads under the project lock and records the fact.
    fn observe(&mut self, store: &mut Store, task: &TaskRef) -> Result<()>;
    /// Both a current native fleet row and the session's task index must agree.
    fn free(&mut self, team: &Team, member: &Member) -> Result<bool>;
    /// Exactly one original child. A caller cannot reconstruct this invocation.
    fn deliver(&mut self, team: &Team, node: &Node, member: &Member, id: &str) -> HandoutOutcome;
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Finish {
    Done,
    Refused(String),
    CuttingOver(String),
}
