use super::*;
/// The document attempt the command actually acted on. This is not the pair
/// read before the child: it comes only from the child's authoritative reply.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::desktop::teams) struct Attempt {
    pub number: u64,
    pub id: String,
}
#[derive(Clone, Debug)]
pub(in crate::desktop::teams) struct Answer {
    pub reply: TaskReply,
    pub attempt: Option<Attempt>,
}
impl Answer {
    /// A node with no task has no document attempt and needs no child.
    pub(super) fn without_task() -> Self {
        Self {
            reply: TaskReply::Cancelled,
            attempt: Some(Attempt::default()),
        }
    }
}
