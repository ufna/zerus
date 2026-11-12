//! zerus-63rru.40: each service item is written and cleared with the row it
//! mirrors, and the cleanup pass removes one whose row is gone.
use super::*;
use crate::desktop::teams::{put_service_item, service_items, test_actor_ledger, ServiceFailure};

fn items(h: &Harness, team: &str, now: i64) -> Vec<(String, String, String, u32)> {
    service_items(h.temp.path(), team, Some(true), now)
        .unwrap()
        .items
        .into_iter()
        .map(|i| (i.scope.to_owned(), i.kind, i.subject, i.failures))
        .collect()
}
fn put(h: &Harness, team: &str, kind: &str, subject: &str) {
    let failure = ServiceFailure {
        team,
        kind,
        subject,
        node: None,
        reason: "A failure",
        failures: 1,
        now: 10,
        next_ms: Some(60_010),
    };
    put_service_item(&test_actor_ledger(h.temp.path()), &failure).unwrap();
}
fn runtime(h: &Harness) -> super::super::Runtime {
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    super::super::Runtime::open(h.temp.path(), &machine).unwrap()
}

/// The terminal page's fault is one service-wide item, refreshed with each
/// failure in its retry record's transaction and cleared when it succeeds.
#[test]
fn the_terminal_pages_fault_is_a_service_wide_item_until_it_succeeds() {
    let h = Harness::new();
    let mut ledger = Ledger::open(h.temp.path()).unwrap();
    ledger.terminal_failed(100, "store\u{7} busy").unwrap();
    ledger.terminal_failed(60_100, "store busy again").unwrap();
    let read = service_items(h.temp.path(), TEAM, Some(true), 60_100)
        .unwrap()
        .items;
    assert_eq!(read.len(), 1);
    let item = &read[0];
    assert_eq!(
        (item.scope, item.kind.as_str(), item.failures, item.since_ms),
        ("service", "terminal_page", 2, 100)
    );
    assert_eq!(item.reason, "store busy again");
    assert_eq!(
        item.next_ms,
        Some(ledger.terminal_retry().unwrap().unwrap().next_ms)
    );
    ledger.terminal_succeeded().unwrap();
    assert!(items(&h, TEAM, 60_100).is_empty());
}

/// A stopped team's finished key sweep that left keys without a final
/// answer is its item, counting such sweeps; a sweep that leaves none
/// clears it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_stop_cleanup_that_keeps_failing_is_a_service_item_until_a_sweep_leaves_none() {
    use crate::desktop::teams::timers::Finished;
    let h = Harness::new();
    let mut a = runtime(&h);
    for now in [100, 200] {
        a.auxiliary.push_finished(Finished {
            team: TEAM.into(),
            unanswered: 2,
            last: Some((3, "No answer\nfrom the child".into())),
        });
        a.sweep_items(now).unwrap();
    }
    let read = service_items(h.temp.path(), TEAM, Some(true), 200)
        .unwrap()
        .items;
    assert_eq!(read.len(), 1);
    let item = &read[0];
    assert_eq!(
        (
            item.kind.as_str(),
            item.subject.as_str(),
            item.node,
            item.failures
        ),
        ("key_clear", "", Some(3), 2)
    );
    assert_eq!(
        item.reason,
        "2 task keys got no final answer on the last sweep; the last: No answer from the child"
    );
    assert_eq!(item.next_ms, Some(200 + 900_000));
    a.auxiliary.push_finished(Finished {
        team: TEAM.into(),
        unanswered: 0,
        last: None,
    });
    a.sweep_items(300).unwrap();
    assert!(items(&h, TEAM, 300).is_empty());
}

/// The cleanup pass removes, a page of 64 at a time, every item whose
/// mirrored row is gone: a cancel item without its cancel row, a terminal
/// item without the terminal retry record, a key-clear item of a team with
/// no key left; it keeps an item whose row is there and a kind it does not
/// know.
#[test]
fn the_cleanup_pass_removes_items_whose_rows_are_gone_within_its_bound() {
    let h = Harness::new();
    let mut a = runtime(&h);
    a.ledger.reserve(TEAM, "cancel:7", json!({})).unwrap();
    put(&h, TEAM, "cancel_retry", "7");
    put(&h, TEAM, "kind_of_a_later_version", "x");
    put(&h, "", "terminal_page", "");
    put(&h, TEAM, "key_clear", "");
    for n in 0..100 {
        put(&h, TEAM, "cancel_halted", &format!("{:04}", 1000 + n));
    }
    let count = |h: &Harness| {
        test_actor_ledger(h.temp.path())
            .query_row("SELECT COUNT(*) FROM service_items", [], |r| {
                r.get::<_, u32>(0)
            })
            .unwrap()
    };
    assert_eq!(count(&h), 104);
    a.maintain(&h.store).unwrap();
    let after_one = count(&h);
    assert!(after_one < 104 && after_one >= 104 - 64, "{after_one}");
    for _ in 0..2 {
        a.maintain(&h.store).unwrap();
    }
    let mut left = items(&h, TEAM, 10);
    left.sort();
    assert_eq!(
        left,
        [
            (
                "team".to_owned(),
                "cancel_retry".to_owned(),
                "7".to_owned(),
                1
            ),
            (
                "team".to_owned(),
                "kind_of_a_later_version".to_owned(),
                "x".to_owned(),
                1
            ),
        ]
    );
}

/// When a terminal team's rows are reclaimed, its items go with them; the
/// service-wide ones and another team's stay.
#[test]
fn a_reclaimed_teams_items_go_with_its_rows() {
    let h = Harness::new();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE teams SET status=?2 WHERE id=?1",
            rusqlite::params![TEAM, serde_json::to_string(&TeamStatus::Finished).unwrap()],
        )
        .unwrap();
    let mut a = runtime(&h);
    a.ledger.reserve(TEAM, "cancel:7", json!({})).unwrap();
    put(&h, TEAM, "kind_of_a_later_version", "x");
    put(&h, "another-team", "kind_of_a_later_version", "y");
    let mut ledger = Ledger::open(h.temp.path()).unwrap();
    ledger.terminal_failed(10, "store busy").unwrap();
    for _ in 0..4 {
        a.maintain(&h.store).unwrap();
    }
    assert!(
        a.ledger.page(TEAM, "").unwrap().is_empty(),
        "rows reclaimed"
    );
    assert_eq!(
        items(&h, TEAM, 10),
        [(
            "service".to_owned(),
            "terminal_page".to_owned(),
            String::new(),
            1
        )]
    );
    assert_eq!(items(&h, "another-team", 10).len(), 2);
}
