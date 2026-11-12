//! zerus-63rru.45, the service half: the hand-out step asks the store to
//! reconcile the lead's unstaffed items only when the kernel's set differs
//! from them, and a race is settled, never a failure.
use super::super::{schedule, Runtime};
use super::*;
use crate::teams::ops::{HandoutOutcome, Operation};
use std::collections::BTreeSet;

struct Fake {
    sends: usize,
}
impl super::super::ports::Ports for Fake {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: Some(BTreeSet::new()),
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        Ok(())
    }
    fn free(&mut self, _: &Team, _: &Member) -> Result<bool> {
        Ok(true)
    }
    fn deliver(
        &mut self,
        _: &Team,
        _: &Node,
        _: &Member,
        _: &str,
    ) -> super::super::ports::DeliveryResult {
        self.sends += 1;
        HandoutOutcome::Submitted {
            receipt: "submitted".into(),
        }
        .into()
    }
}
/// A Reassign settled at once whose assignee is then removed: one ready node
/// that no member can take.
fn stranded() -> (Harness, Runtime, Fake, u32) {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let node = h.add(&person(), 1);
    h.run(
        &lead,
        Operation::Reassign {
            node,
            member: "m2".into(),
            reason: "Give it to m2".into(),
        },
    )
    .unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m2".into(),
        },
    )
    .unwrap();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let a = Runtime::open(h.temp.path(), &home).unwrap();
    (h, a, Fake { sends: 0 }, node)
}
fn tick(h: &mut Harness, a: &mut Runtime, p: &mut Fake, now: i64) {
    a.begin_pass();
    a.tick(&mut h.store, p, TEAM, now, false).unwrap();
}
fn open_items(h: &Harness) -> Vec<u32> {
    let rows = h.store.inbox(TEAM, 200, None).unwrap().rows;
    rows.into_iter()
        .filter(|i| i.state == "open" && i.kind.starts_with("unstaffed:"))
        .map(|i| i.node)
        .collect()
}
fn items(h: &Harness) -> usize {
    let rows = h.store.inbox(TEAM, 200, None).unwrap().rows;
    rows.iter()
        .filter(|i| i.kind.starts_with("unstaffed:"))
        .count()
}
/// The lead closing its open unstaffed items. No operation closes one for
/// the lead yet; every closer but the reconcile leaves the reply empty, as
/// this does.
fn closed_by_lead(h: &Harness) {
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE inbox SET state='closed' WHERE team=?1 AND state='open' AND kind LIKE 'unstaffed:%'",
            [TEAM],
        )
        .unwrap();
}
fn revision(h: &Harness) -> u64 {
    h.store.team(TEAM).unwrap().unwrap().revision
}
/// What a pass may not change when it has nothing to tell: the revision, the
/// journal, the inbox and the cards.
fn snapshot(
    h: &Harness,
) -> (
    u64,
    Vec<crate::teams::store::JournalEntry>,
    Vec<crate::teams::store::InboxItem>,
    Vec<crate::teams::store::Card>,
) {
    (
        h.store.team(TEAM).unwrap().unwrap().revision,
        h.store.journal(TEAM, 200, None).unwrap().rows,
        h.store.inbox(TEAM, 200, None).unwrap().rows,
        h.store.cards(TEAM, 200, None).unwrap().rows,
    )
}

#[test]
fn a_pass_tells_the_lead_once_and_a_hundred_unchanged_passes_write_nothing() {
    let (mut h, mut a, mut p, node) = stranded();
    tick(&mut h, &mut a, &mut p, 100);
    assert_eq!(open_items(&h), vec![node]);
    assert_eq!(p.sends, 0, "nobody can take it");
    let after = snapshot(&h);
    for pass in 0..100 {
        tick(&mut h, &mut a, &mut p, 200 + pass * 60_000);
    }
    assert_eq!(snapshot(&h), after);
    assert!(a.ledger.get(TEAM, "unstaffed").unwrap().is_none());
    assert!(a.ledger.get(TEAM, "gate:unstaffed").unwrap().is_none());
}

/// A hundred passes whose view is stale: each asks, the store finds nothing
/// to change and answers `unstaffed_unchanged`, which is settled: no gate
/// failure, no card, the revision unmoved.
#[test]
fn a_hundred_racing_passes_are_settled_without_a_gate_failure_card_or_revision() {
    let (mut h, mut a, mut p, node) = stranded();
    tick(&mut h, &mut a, &mut p, 100);
    let after = snapshot(&h);
    schedule::STALE_VIEW.with(|s| s.set(Some(node + 1000)));
    for pass in 0..100 {
        tick(&mut h, &mut a, &mut p, 200 + pass * 60_000);
    }
    schedule::STALE_VIEW.with(|s| s.set(None));
    assert_eq!(snapshot(&h), after);
    assert!(a.ledger.get(TEAM, "gate:unstaffed").unwrap().is_none());
    assert!(a.ledger.get(TEAM, "unstaffed").unwrap().is_none());
    let refusals: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM requests WHERE team=?1 AND json_extract(result,'$.Refused.code')='unstaffed_unchanged'",
            [TEAM],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(refusals, 100, "each racing pass asked once");
    assert_eq!(open_items(&h), vec![node]);
}

/// A restart between the store's commit and the service's own record: the
/// next pass applies the saved request again, which answers its saved reply
/// and changes nothing.
#[test]
fn a_restart_after_the_commit_replays_the_saved_reconcile_and_changes_nothing() {
    let (mut h, mut a, mut p, node) = stranded();
    schedule::UNSTAFFED_CRASH.with(|s| s.set(Some("applied")));
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tick(&mut h, &mut a, &mut p, 100);
    }));
    let remaining = schedule::UNSTAFFED_CRASH.with(|s| s.replace(None));
    assert!(
        crashed.is_err() && remaining.is_none(),
        "the crash point was visited"
    );
    assert_eq!(open_items(&h), vec![node], "the store committed");
    let saved = a.ledger.get(TEAM, "unstaffed").unwrap();
    assert!(saved.is_some(), "the service had not recorded its result");
    let after = snapshot(&h);
    drop(a);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = Runtime::open(h.temp.path(), &home).unwrap();
    tick(&mut h, &mut a, &mut p, 200);
    assert_eq!(snapshot(&h), after, "the replay changed nothing");
    assert!(a.ledger.get(TEAM, "unstaffed").unwrap().is_none());
    assert!(a.ledger.get(TEAM, "gate:unstaffed").unwrap().is_none());
}

/// Nothing in a paused or stopped team: no pass reconciles it.
#[test]
fn a_paused_or_stopped_team_gets_no_unstaffed_pass() {
    for op in [Operation::Pause, Operation::Stop] {
        let (mut h, mut a, mut p, _) = stranded();
        h.run(&person(), op).unwrap();
        tick(&mut h, &mut a, &mut p, 100);
        assert!(open_items(&h).is_empty());
    }
}

/// Review 45, P2-1 (the reviewer's actor reproducer): closing the item
/// between passes, three times, while the node stays unstaffed. The episode
/// stands: no pass tells it again and no revision moves.
#[test]
fn a_closed_item_is_not_recreated_without_a_set_transition() {
    let (mut h, mut a, mut p, node) = stranded();
    tick(&mut h, &mut a, &mut p, 100);
    assert_eq!(open_items(&h), vec![node]);
    for pass in 0..3 {
        closed_by_lead(&h);
        let before = revision(&h);
        tick(&mut h, &mut a, &mut p, 200 + pass * 60_000);
        assert!(open_items(&h).is_empty(), "pass {pass}: recreated");
        assert_eq!(revision(&h), before, "pass {pass}");
    }
    assert_eq!(items(&h), 1);
}

/// The lead closes its item on each of a hundred passes: one item in all,
/// and no revision moves after the first pass.
#[test]
fn a_lead_closing_its_item_a_hundred_times_is_told_once() {
    let (mut h, mut a, mut p, node) = stranded();
    tick(&mut h, &mut a, &mut p, 100);
    assert_eq!(open_items(&h), vec![node]);
    let first = revision(&h);
    for pass in 0..100 {
        closed_by_lead(&h);
        tick(&mut h, &mut a, &mut p, 200 + pass * 60_000);
    }
    assert_eq!(items(&h), 1);
    assert_eq!(revision(&h), first);
    assert!(a.ledger.get(TEAM, "unstaffed").unwrap().is_none());
    assert!(a.ledger.get(TEAM, "gate:unstaffed").unwrap().is_none());
    let refusals: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM requests WHERE team=?1 AND json_extract(result,'$.Refused.code')='unstaffed_unchanged'",
            [TEAM],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(refusals, 0, "the service never asked again");
}
