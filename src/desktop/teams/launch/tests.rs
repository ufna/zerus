use super::*;
use crate::teams::ops::tests::{Harness, TEAM};
#[test]
fn blocking_a_row_releases_its_live_admission_immediately() {
    let h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut r = Runtime::open(h.temp.path(), &t.home, "epoch").unwrap();
    let mut row = Row::new(&t, "lead", "epoch", State::WorktreeIntent);
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    r.ledger().insert(&row, 1).unwrap();
    r.admitted = Some((t.id, row.original.clone()));
    r.block(&mut row, uncertain("Synthetic interrupted effect"), 2)
        .unwrap();
    assert!(!r.busy());
}
#[test]
fn owner_card_text_is_bounded_by_bytes_at_a_utf8_boundary() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut r = Runtime::open(h.temp.path(), &t.home, "epoch").unwrap();
    let mut row = Row::new(&t, "lead", "epoch", State::Blocked);
    row.problem =
        Some(Problem::new(code::INVALID_STATE, "⚙".repeat(5000)).with_refs(["⏳".repeat(3000)]));
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    r.ledger().insert(&row, 1).unwrap();
    r.card(&mut h.store, &mut row, 2).unwrap();
    let card = h.store.card(TEAM, row.card_id.unwrap()).unwrap().unwrap();
    assert!(card.text.len() <= 3072);
    assert!(std::str::from_utf8(card.text.as_bytes()).is_ok());
}

/// zerus-63rru.24 and zerus-63rru.63: a stopped team's launch row whose
/// failures are at the retry cap gets one service item, visible in
/// `hgs team status`, and the row's successful pass ends it.
fn failing_row(h: &mut Harness, member: &str, times: u32) -> Runtime {
    let t = h.store.team(TEAM).unwrap().unwrap();
    let machine = t.home.clone();
    let mut r = Runtime::open(h.temp.path(), &machine, "epoch").unwrap();
    let mut row = Row::new(&t, member, "epoch", State::Retry);
    r.ledger = Some(Ledger::open(h.temp.path()).unwrap());
    r.ledger.as_mut().unwrap().insert(&row, 1).unwrap();
    for n in 0..times {
        r.failure(
            &mut h.store,
            &mut row,
            Problem::new(zerus_team_plan::problem::code::INVALID_STATE, "The launch keeps failing"),
            1000 + i64::from(n) * 60_000,
            false,
        )
        .unwrap();
    }
    r
}
fn status_items(h: &Harness, now: i64) -> Vec<(String, String, String, u32)> {
    crate::desktop::teams::service_items(h.temp.path(), TEAM, Some(true), now)
        .unwrap()
        .items
        .into_iter()
        .map(|i| (i.kind.to_owned(), i.subject, i.reason, i.failures))
        .collect()
}

#[test]
fn a_terminal_cleanup_item_appears_only_at_the_retry_cap_and_goes_on_success() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    // One row below the cap and one at it, failed while the team runs.
    let _ = failing_row(&mut h, "m2", 1);
    let mut at_cap = failing_row(&mut h, "m3", 5);
    h.run(&crate::teams::ops::tests::person(), Operation::Stop).unwrap();
    // The first pass after the stop writes the cap row's item, once, and
    // nothing for the row below the cap.
    at_cap.note_late(&t, 400_000).unwrap();
    let items = status_items(&h, 400_000);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        (items[0].0.as_str(), items[0].1.as_str(), items[0].3),
        ("launch_failing", "m3", 5)
    );
    assert!(items[0].2.contains("The launch keeps failing"), "{items:?}");
    // The row's successful pass clears the item.
    let mut row = at_cap
        .ledger
        .as_mut()
        .unwrap()
        .get(TEAM, "m3")
        .unwrap()
        .unwrap();
    at_cap.close_stale(&mut h.store, &t, &mut row, 460_000).unwrap();
    assert!(status_items(&h, 460_000).is_empty());
}

#[test]
fn a_stopped_teams_failing_row_is_visible_on_the_first_pass_and_never_written_twice() {
    let mut h = Harness::new();
    let t = h.store.team(TEAM).unwrap().unwrap();
    let mut r = failing_row(&mut h, "m3", 5);
    h.run(&crate::teams::ops::tests::person(), Operation::Stop).unwrap();
    for now in [400_000, 460_000, 520_000, 580_000] {
        r.note_late(&t, now).unwrap();
        let items = status_items(&h, now);
        assert_eq!(items.len(), 1, "pass at {now}: {items:?}");
        assert_eq!(items[0].3, 5, "the pass at {now} wrote the item again");
    }
}
