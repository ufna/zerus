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
