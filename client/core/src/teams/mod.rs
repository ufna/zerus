//! The Teams read model and actions of the desktop client: what the Team tab
//! and the Waiting list show, read from `hgs team <op> --json`, projected into
//! compact typed views, and the guarded actions the person and the lead send.
//!
//! - [`view`] projects a checked reply into the view models; the webview gets
//!   these structures and never parses an `hgs` reply.
//! - [`read`] runs one read through `hgs` with its timeouts and output bound,
//!   and checks the shape the projection relies on.
//! - [`model`] decides which read is due for the followed views, keeps the
//!   latest projected view per key, and folds each reply into
//!   [`zerus_client_core::Change::Teams`] changes.
//! - [`poller`] runs the model on one thread and hands every change to the
//!   client pump.
//! - [`action`] builds and classifies the team actions as guarded calls with
//!   request identities; an uncertain outcome is surfaced, never sent again.
//!
//! The views are bounded: at most [`view::PAGE`] rows per list and the caps of
//! [`view`]. Teams are inactive unless the person turned the preview on; with
//! it off every read and action answers `teams_inactive` and the desktop shows
//! no entry point.
pub mod action;
pub mod model;
pub mod poller;
pub mod read;
pub mod view;

pub use action::{Confirmation, TeamAction, TeamCall, TeamOutcome};
pub use model::{team_key, TeamsFollow, TeamsModel, TeamsUpdate, POLL};
pub use poller::TeamsPoller;
pub use read::{read, valid_team, TeamRead};
