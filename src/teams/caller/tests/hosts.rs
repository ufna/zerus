//! The match against recorded resident agent hosts, on every platform. The
//! real processes are in `regressions`.
use super::*;
use crate::state::session_graph::{Hosts, Named, Registered};
use ancestry::{Ancestor, Process};

fn row(pid: u32, birth: Option<&str>) -> Ancestor {
    Ancestor {
        pid,
        identity: Process {
            parent: pid + 1,
            birth: birth.map(Into::into),
        },
        claim: None,
        top: false,
    }
}

fn deepseek(pid: u32, birth: Option<&str>) -> Hosts {
    Hosts {
        deepseek: vec![Named {
            pid,
            birth: birth.map(Into::into),
        }],
        opencode: None,
    }
}

fn opencode(pid: u32, written: f64) -> Hosts {
    Hosts {
        deepseek: Vec::new(),
        opencode: Some(Registered { pid, written }),
    }
}

fn unread(_: u32) -> Option<(String, f64)> {
    None
}

#[test]
fn a_deepseek_host_matches_by_process_id_and_birth() {
    let chain = [
        row(10, Some("b10")),
        row(11, Some("b11")),
        row(12, Some("b12")),
    ];
    assert_eq!(hosted(&chain, &deepseek(11, Some("b11")), unread), Ok(true));
    assert_eq!(
        hosted(&chain, &deepseek(13, Some("b13")), unread),
        Ok(false)
    );
    assert_eq!(hosted(&chain, &Hosts::default(), unread), Ok(false));
}

/// The top of the user's tree is an ancestor like any other: on macOS launchd
/// adopts the host and it is the top.
#[test]
fn a_host_at_the_top_of_the_chain_matches() {
    let mut top = row(11, Some("b11"));
    top.top = true;
    let chain = [row(10, Some("b10")), top];
    assert_eq!(hosted(&chain, &deepseek(11, Some("b11")), unread), Ok(true));
    let started = |_: u32| Some(("b11".to_owned(), 90.0));
    assert_eq!(hosted(&chain, &opencode(11, 100.0), started), Ok(true));
}

/// The record's birth is the walk's own `process_identity` value, so the
/// recorded host always matches; a different birth is a reused process ID.
#[test]
fn a_host_record_whose_birth_differs_is_not_that_host() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    assert_eq!(
        hosted(&chain, &deepseek(11, Some("old")), unread),
        Ok(false)
    );
}

#[test]
fn a_recorded_process_id_without_a_birth_on_either_side_refuses() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    assert_eq!(
        hosted(&chain, &deepseek(11, None), unread),
        Err(host_unreadable())
    );
    let top = [row(10, Some("b10")), row(11, None)];
    assert_eq!(
        hosted(&top, &deepseek(11, Some("b11")), unread),
        Err(host_unreadable())
    );
}

/// OpenCode's file names no birth: its process ID counts when the ancestor
/// is not younger than the file, with a second's allowance.
#[test]
fn an_opencode_service_counts_when_it_is_not_younger_than_its_file() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    let started = |at: f64| move |_: u32| Some(("b11".to_owned(), at));
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(90.0)),
        Ok(true)
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(100.5)),
        Ok(true)
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(102.0)),
        Ok(false)
    );
    assert_eq!(
        hosted(&chain, &opencode(12, 100.0), started(90.0)),
        Ok(false)
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), unread),
        Err(host_unreadable())
    );
    let replaced = |_: u32| Some(("other".to_owned(), 90.0));
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), replaced),
        Err(host_unreadable())
    );
}

#[test]
fn only_sessions_of_a_resident_host_are_host_served() {
    assert!(host_served_agent("dsh") && host_served_agent("opencode"));
    for agent in ["claude", "codex", "kimi", "", "DSH"] {
        assert!(!host_served_agent(agent), "{agent}");
    }
}
