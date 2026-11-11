//! Durable standing-confirmation policy and exact audit identities.
#![allow(dead_code)] // Native ports and worker entry follow in the next commit.
mod engine;
mod ledger;
mod policy;
mod ports;
mod resumed;
mod types;
#[allow(unused_imports)] // The agreed package-8 availability port consumes this seam.
pub(super) use resumed::{resumed_binding, ResumedBinding};
