//! Replicated Zerus documents on Loro. This crate is the only code in Zerus
//! that imports `loro`: hgs and the mobile apps use its typed operations, so
//! a Loro upgrade or a schema change happens here once.
mod attachment;
mod error;
pub mod execution;
pub mod ids;
mod ops;
pub mod phone;
mod project;
pub mod rank;
mod render;
mod schema;
mod store;
mod sync;
mod task;

pub use attachment::{
    TaskAttachment, MAX_ATTACHMENT, MAX_ATTACHMENTS, MAX_ATTACHMENT_TOTAL, TASK_SCHEMA,
};

/// The largest serialized task document the phone can assemble: one current
/// requirement's text and media plus one retained attachment generation.
/// Attachment edits stop before a task document outgrows it.
pub const MAX_TASK_DOCUMENT: usize = 2 * (MAX_DOCUMENT + MAX_ATTACHMENT_TOTAL.div_ceil(3) * 4);
pub use error::{Error, ErrorKind, Result};
pub use project::{CutoverHold, ProjectDoc, Settings};
pub use render::{
    cancel_message, command_reference, command_word, task_dispatch_content, task_prompt, Tools,
};
pub use schema::{
    Actor, LogEntry, SessionKey, Stage, Target, TaskDelivery, TaskView, TaskWorker, MAX_DOCUMENT,
    MAX_TEXT, SCHEMA,
};
#[cfg(any(test, feature = "test-support"))]
pub use store::load_count_for_test;
#[cfg(test)]
mod placement_tests;
pub use store::{
    task_file, Admitted, Changes, CopyReview, CopyReviewOutcome, Damaged, Keep, Placed, Placement,
    Position, ProjectStore, TaskFile, DEPENDS_ON_INCOMING_COPY, MARK_FORMAT,
};
pub use sync::{
    canonical_version, version_includes, Document, DoneEntry, DonePage, DoneRow, Imported,
    Inventory, Update, Version,
};
#[cfg(not(feature = "test-without-team"))]
pub use task::TaskTeam;
pub use task::{NewTask, Settled, Settlement, TaskDoc};
