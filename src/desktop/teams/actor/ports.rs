use crate::teams::{ops::HandoutOutcome, store::Store};
use zerus_team_plan::{model::*, Problem};
pub(super) type Result<T> = std::result::Result<T, Problem>;

pub(in crate::desktop::teams) trait Ports {
    fn stopped(&self) -> bool {
        false
    }
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
    fn report(&mut self, _task: &TaskRef) -> Result<Option<String>> {
        Ok(None)
    }
    fn reported(
        &mut self,
        _team: &Team,
        _member: &Member,
    ) -> Result<crate::teams::integrate::Reported> {
        Err(unavailable())
    }
    fn repository(&mut self, _team: &Team) -> Result<String> {
        Err(unavailable())
    }
    fn descends(&mut self, _plan: &MergePlan) -> Result<bool> {
        Err(unavailable())
    }
    fn contains(&mut self, _plan: &MergePlan) -> Result<Option<String>> {
        Err(unavailable())
    }
    fn merge(&mut self, _plan: &MergePlan) -> MergeResult {
        MergeResult::Unavailable {
            reason: unavailable().message,
        }
    }
    fn finish(&mut self, _team: &Team, _node: &Node, _id: &str) -> Result<Finish> {
        Err(unavailable())
    }
    /// Exactly one original child. A caller cannot reconstruct this invocation.
    fn deliver(&mut self, team: &Team, node: &Node, member: &Member, id: &str) -> HandoutOutcome;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop::teams) enum Finish {
    Done,
    Refused(String),
    CuttingOver(String),
}

fn unavailable() -> Problem {
    Problem::new(
        zerus_team_plan::problem::code::INTEGRATION_UNAVAILABLE,
        "Team work reader is unavailable",
    )
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::desktop::teams) struct MergePlan {
    pub team: String,
    pub owner: String,
    pub machine: String,
    pub project: String,
    pub branch: String,
    pub base: String,
    pub repo: String,
    pub node: u32,
    pub generation: u32,
    pub decision: u64,
    pub member: String,
    pub commit: String,
    pub id: String,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::desktop::teams) enum MergeResult {
    Merged { commit: String },
    Conflict { files: Vec<String> },
    Unavailable { reason: String },
    Unknown { reason: String },
}
