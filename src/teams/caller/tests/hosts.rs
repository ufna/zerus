//! The match against recorded resident agent hosts, on every platform. The
//! real processes are in `regressions`.
use super::*;
use crate::state::session_graph::{Hosts, Named, Registered};
use ancestry::{Ancestor, Inspection, Process};

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

fn host(pid: u32, birth: &str) -> Option<Host> {
    Some(Host {
        pid,
        birth: birth.into(),
    })
}

/// The host records as [`hosted_ancestor`] reads them.
fn hosts(hosts: Hosts) -> impl FnOnce() -> Result<Hosts, crate::state::session_graph::Unread> {
    move || Ok(hosts)
}

fn chain() -> Result<Vec<Ancestor>, Problem> {
    Ok(vec![row(10, Some("b10")), row(11, Some("b11"))])
}

fn never() -> Result<Vec<Ancestor>, Problem> {
    panic!("no walk without a recorded host")
}

#[test]
fn a_deepseek_host_matches_by_process_id_and_birth() {
    let chain = [
        row(10, Some("b10")),
        row(11, Some("b11")),
        row(12, Some("b12")),
    ];
    assert_eq!(
        hosted(&chain, &deepseek(11, Some("b11")), unread),
        Ok(host(11, "b11"))
    );
    assert_eq!(hosted(&chain, &deepseek(13, Some("b13")), unread), Ok(None));
    assert_eq!(hosted(&chain, &Hosts::default(), unread), Ok(None));
}

/// The top of the user's tree is an ancestor like any other: on macOS launchd
/// adopts the host and it is the top.
#[test]
fn a_host_at_the_top_of_the_chain_matches() {
    let mut top = row(11, Some("b11"));
    top.top = true;
    let chain = [row(10, Some("b10")), top];
    assert_eq!(
        hosted(&chain, &deepseek(11, Some("b11")), unread),
        Ok(host(11, "b11"))
    );
    let started = |_: u32| Some(("b11".to_owned(), 90.0));
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started),
        Ok(host(11, "b11"))
    );
}

/// The record's birth is the walk's own `process_identity` value, so the
/// recorded host always matches; a different birth is a reused process ID.
#[test]
fn a_host_record_whose_birth_differs_is_not_that_host() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    assert_eq!(hosted(&chain, &deepseek(11, Some("old")), unread), Ok(None));
}

/// An unreadable birth is a refusal, never a miss: a recorded process ID
/// without a birth, an ancestor whose birth cannot be read, or an OpenCode
/// service whose start cannot be read, wherever that ID is an ancestor's.
#[test]
fn an_unreadable_birth_is_a_refusal_never_a_miss() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    assert_eq!(
        hosted(&chain, &deepseek(11, None), unread),
        Err(Refusal::Birth)
    );
    let top = [row(10, Some("b10")), row(11, None)];
    assert_eq!(
        hosted(&top, &deepseek(11, Some("b11")), unread),
        Err(Refusal::Birth)
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), unread),
        Err(Refusal::Birth)
    );
    assert_eq!(
        hosted_ancestor_read(hosts(deepseek(11, None)), chain_of(&chain), unread),
        Err(Refusal::Birth)
    );
    assert_eq!(host_refusal(Refusal::Birth), host_unreadable());
}

fn chain_of(rows: &[Ancestor]) -> impl FnOnce() -> Result<Vec<Ancestor>, Problem> {
    let rows: Vec<_> = rows
        .iter()
        .map(|row| Ancestor {
            pid: row.pid,
            identity: row.identity.clone(),
            claim: None,
            top: row.top,
        })
        .collect();
    move || Ok(rows)
}

/// OpenCode's file names no birth: its process ID counts when the ancestor
/// is not younger than the file, with a second's allowance.
#[test]
fn an_opencode_service_counts_when_it_is_not_younger_than_its_file() {
    let chain = [row(10, Some("b10")), row(11, Some("b11"))];
    let started = |at: f64| move |_: u32| Some(("b11".to_owned(), at));
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(90.0)),
        Ok(host(11, "b11"))
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(100.5)),
        Ok(host(11, "b11"))
    );
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), started(102.0)),
        Ok(None)
    );
    assert_eq!(
        hosted(&chain, &opencode(12, 100.0), started(90.0)),
        Ok(None)
    );
    let replaced = |_: u32| Some(("other".to_owned(), 90.0));
    assert_eq!(
        hosted(&chain, &opencode(11, 100.0), replaced),
        Err(Refusal::Birth)
    );
}

/// The check reads the records first and walks only when some host is
/// recorded, OpenCode's registered service among them (zerus-63rru.28).
#[test]
fn the_check_reads_the_records_first_and_walks_only_with_some() {
    assert_eq!(
        hosted_ancestor_read(hosts(Hosts::default()), never, unread),
        Ok(None)
    );
    assert_eq!(
        hosted_ancestor_read(hosts(deepseek(11, Some("b11"))), chain, unread),
        Ok(host(11, "b11"))
    );
    assert_eq!(
        hosted_ancestor_read(hosts(deepseek(12, Some("b12"))), chain, unread),
        Ok(None)
    );
    let started = |_: u32| Some(("b11".to_owned(), 90.0));
    assert_eq!(
        hosted_ancestor_read(hosts(opencode(11, 100.0)), chain, started),
        Ok(host(11, "b11"))
    );
    assert_eq!(
        hosted_ancestor_read(hosts(opencode(12, 100.0)), chain, started),
        Ok(None)
    );
}

/// When the check cannot tell, its refusal names the cause, each its own,
/// and each refuses in the person path's words.
#[test]
fn the_checks_refusal_tells_its_causes_apart() {
    use crate::state::session_graph::Unread;
    let causes = [
        hosted_ancestor_read(|| Err(Unread::TooManyFolders), never, unread),
        hosted_ancestor_read(|| Err(Unread::TooManyEntries), never, unread),
        hosted_ancestor_read(|| Err(Unread::Unreadable), never, unread),
        hosted_ancestor_read(hosts(deepseek(11, None)), chain, unread),
        hosted_ancestor_read(
            hosts(deepseek(12, Some("b12"))),
            || Err(unreadable()),
            unread,
        ),
    ];
    let all = [
        Refusal::TooManyFolders,
        Refusal::TooManyEntries,
        Refusal::Unreadable,
        Refusal::Birth,
        Refusal::Walk,
    ];
    assert_eq!(causes, all.map(Err));
    assert_eq!(
        all.map(host_refusal),
        [
            too_many_hosts(),
            too_many_entries(),
            host_unreadable(),
            host_unreadable(),
            unreadable(),
        ]
    );
}

/// The walk fails closed at its bound: 64 processes may reach the top of
/// the user's tree, and a 65th refuses as a walk past its bound, never as no
/// host above.
#[test]
fn the_checks_walk_fails_closed_at_its_bound() {
    // Each process is the child of the one below it; 1 is another user's.
    let table = |pid: u32| {
        Ok(if pid > 1 {
            Inspection::Own(Process {
                parent: pid - 1,
                birth: Some(format!("b{pid}")),
            })
        } else {
            Inspection::Foreign
        })
    };
    let empty = |_: u32| Ok(Vec::new());
    let at_top = || hosts(deepseek(2, Some("b2")));
    assert_eq!(
        hosted_ancestor_read(at_top(), || ancestry::walk(65, table, empty), unread),
        Ok(host(2, "b2"))
    );
    assert_eq!(
        hosted_ancestor_read(at_top(), || ancestry::walk(66, table, empty), unread),
        Err(Refusal::Walk)
    );
    let elsewhere = || hosts(deepseek(1000, Some("b1000")));
    assert_eq!(
        hosted_ancestor_read(elsewhere(), || ancestry::walk(66, table, empty), unread),
        Err(Refusal::Walk)
    );
}

#[test]
fn only_sessions_of_a_resident_host_are_host_served() {
    assert!(host_served_agent("dsh") && host_served_agent("opencode"));
    for agent in ["claude", "codex", "kimi", "", "DSH"] {
        assert!(!host_served_agent(agent), "{agent}");
    }
}

/// The record rules of one host folder, read by the reader itself.
#[cfg(unix)]
mod records {
    use crate::state::session_graph::{caller_host_folder, written_boot, Named};
    use serde_json::{json, Value};

    const THIS: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "22222222-2222-4222-8222-222222222222";

    fn folder(record: &Value, host_pid: Option<u32>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("host.identity.json"), record.to_string()).unwrap();
        if let Some(pid) = host_pid {
            std::fs::write(dir.path().join("host.pid"), pid.to_string()).unwrap();
        }
        dir
    }

    fn running(boot: &str) -> Value {
        json!({"state":"running","version":2,"pid":4242,"birth":"77","pid_file":[1,2,3,4],"boot":boot})
    }

    fn named(pid: u32, birth: Option<&str>) -> Named {
        Named {
            pid,
            birth: birth.map(Into::into),
        }
    }

    /// The writer's boot read is the reader's: a record stamped with it is
    /// this boot's, here as in every real-process test.
    #[test]
    fn a_record_stamped_with_the_writers_boot_is_this_boots() {
        let Some(boot) = written_boot() else {
            eprintln!("skipped: this computer's boot cannot be read");
            return;
        };
        let dir = folder(&running(&boot), Some(4242));
        let got = caller_host_folder(dir.path(), Some(&boot)).unwrap();
        assert_eq!(got, vec![named(4242, Some("77"))]);
    }

    /// When this boot cannot be read, no record is set aside for its boot.
    #[test]
    fn an_unread_boot_sets_nothing_aside() {
        let dir = folder(&running(OTHER), Some(4242));
        assert_eq!(
            caller_host_folder(dir.path(), None).unwrap(),
            vec![named(4242, Some("77"))]
        );
        let got = caller_host_folder(dir.path(), Some(THIS)).unwrap();
        assert_eq!(got, Vec::new(), "another boot, with its own host.pid");
    }

    /// Only the form the writer stamps can set a record aside; a boot in any
    /// other form is unknown, and the record stays named.
    #[test]
    fn only_the_writers_boot_form_sets_a_record_aside() {
        let lettered = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
        let dir = folder(&running(lettered), None);
        let got = caller_host_folder(dir.path(), Some(THIS)).unwrap();
        assert_eq!(got, Vec::new(), "the written form of another boot");
        for form in [
            lettered.to_uppercase(),
            format!("{{{lettered}}}"),
            lettered.replace('-', ""),
            "garbage".to_owned(),
        ] {
            let dir = folder(&running(&form), None);
            let got = caller_host_folder(dir.path(), Some(THIS)).unwrap();
            assert_eq!(got, vec![named(4242, Some("77"))], "{form}");
        }
    }

    /// A starting record from another boot sets aside the `host.pid` it
    /// publishes as its PID file; a different `host.pid` stays named.
    #[test]
    fn an_old_boot_starting_record_covers_its_own_host_pid() {
        let starting = |pid_file: u32| {
            json!({"state":"starting","version":2,"attempt":"33333333-3333-4333-8333-333333333333",
                "boot":OTHER,"launcher":{"pid":1u32 << 30,"birth":"1"},"phase":"spawning",
                "pid_file":[pid_file,[1,2,3,4]],"previous":null})
        };
        let dir = folder(&starting(5151), Some(5151));
        assert_eq!(
            caller_host_folder(dir.path(), Some(THIS)).unwrap(),
            Vec::new()
        );
        let dir = folder(&starting(5151), Some(6161));
        assert_eq!(
            caller_host_folder(dir.path(), Some(THIS)).unwrap(),
            vec![named(6161, None)]
        );
        let dir = folder(&starting(5151), Some(5151));
        assert_eq!(
            caller_host_folder(dir.path(), Some(OTHER)).unwrap(),
            vec![named(5151, None)],
            "this boot's own starting record names its PID file once"
        );
    }
}
