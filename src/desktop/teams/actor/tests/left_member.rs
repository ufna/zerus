//! zerus-63rru.85 through the production Native and the real pass: a
//! hand-out reserved for a member that left the active set (paused,
//! stopped, unbound) is refused by the spawn admission on each replay, and
//! after five non-starts (about 26 minutes) the effect is refused and the
//! next pass hands the task to another member, with nobody told. The fix
//! settles the reservation at the next pass like a removal does, with one
//! item per task; whether the person paused the member is not stored, so
//! the lead is told in every case.
use super::real_rows::Production;
use super::rows::seed_row;
use crate::desktop::teams::actor::ports::{DeliveryResult, Ports};
use crate::teams::ops::tests::*;
use crate::teams::ops::{Actor, HandoutOutcome, Operation};
use serde_json::{json, Value};
use zerus_team_plan::model::{Member, Node, TaskRef, Team};

const MINUTE: i64 = 60_000;
const T0: i64 = 10_000_000;

/// The member's committed fleet row, as its producer writes it.
fn row(p: &Production, h: &Harness, member: &str, changes: &[(&str, Value)]) {
    let m = h.store.member(TEAM, member).unwrap().unwrap();
    let launch = m.session.as_ref().unwrap().session_id["launch_id:".len()..].to_owned();
    let run = format!("fixture-{member}");
    let mut all = vec![
        ("name", json!(format!("{}/fixture/{member}", m.agent))),
        ("run_id", json!(run)),
        ("launch_id", json!(launch)),
        ("conversation_id", json!(format!("conversation-{member}"))),
    ];
    all.extend(changes.iter().cloned());
    let row = seed_row(&m.agent, &all).to_string();
    rusqlite::Connection::open(p.native.config.state.join("desktop/state.sqlite3"))
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,?1,?2,?3)",
            rusqlite::params![format!("run:{run}"), row, row.len()],
        )
        .unwrap();
}

/// A running team whose task 1 can go to m1 only (m2 is busy), the lead
/// bound; the first pass has not run.
fn team() -> (Harness, super::super::Runtime, Production, Actor) {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    h.add(&person(), 1);
    let native = super::production::native(&h);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let p = Production {
        native,
        deliveries: vec![],
    };
    row(&p, &h, "m2", &[("activity", json!("busy"))]);
    (h, a, p, lead)
}

/// The member's store state as the ops' own operations leave it.
fn left(h: &mut Harness, how: &str) {
    match how {
        "paused" => {
            h.run(
                &person(),
                Operation::PauseMember {
                    member: "m1".into(),
                },
            )
            .unwrap();
        }
        "stopped" => {
            h.run(
                &person(),
                Operation::StopMember {
                    member: "m1".into(),
                },
            )
            .unwrap();
        }
        "unbound" => {
            let m = h.store.member(TEAM, "m1").unwrap().unwrap();
            let session = m.session.unwrap();
            let launch = session.session_id["launch_id:".len()..].to_owned();
            crate::state::observe_team_session(
                &h.temp.path().join("state"),
                "gone-session",
                &launch,
                &m.agent,
                "the binding ended",
                std::time::Instant::now() + std::time::Duration::from_secs(5),
            );
        }
        other => panic!("{other}"),
    }
}

/// The hand-out crashes after its intent (the effect reserved, no outcome
/// recorded); then the member leaves and the service restarts: the
/// reservation exists with the member no longer in the active set.
fn reserved_then_left(
    h: &mut Harness,
    a: &mut super::super::Runtime,
    p: &mut Production,
    how: &str,
) -> (super::super::Runtime, String) {
    super::super::delivery::CRASH.with(|c| c.set(Some("intent")));
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.tick(&mut h.store, p, TEAM, T0, false)
    }));
    assert!(
        crashed.is_err(),
        "{how}: the intent boundary was not exercised"
    );
    assert!(p.deliveries.is_empty(), "{how}: {:?}", p.deliveries);
    let id = effect_id(h);
    drop(a);
    left(h, how);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    (
        super::super::Runtime::open(h.temp.path(), &machine).unwrap(),
        id,
    )
}

fn effects(h: &Harness) -> Vec<(String, String, u32)> {
    h.store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .map(|e| (e.state, e.member, e.node))
        .collect()
}

fn effect_id(h: &Harness) -> String {
    h.store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .find(|e| e.member == "m1")
        .unwrap()
        .request_id
}

fn items(h: &Harness) -> Vec<(String, String)> {
    h.store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .map(|i| (i.kind, i.text))
        .collect()
}

/// The defect measured: the reservation is replayed through the non-start
/// tiers (0/1/3/7/15 minutes, about 26 in all), then the effect is refused
/// and the next pass hands the task to m2 — and nobody is told. The fixed
/// asserts (red until the fix): settled at the next pass with one item.
#[test]
fn a_reservation_for_a_member_that_left_the_active_set_is_settled_at_the_next_pass_with_one_item() {
    for how in ["paused", "stopped"] {
        let (mut h, mut a, mut p, lead) = team();
        let (mut a, id) = reserved_then_left(&mut h, &mut a, &mut p, how);
        assert_eq!(effects(&h), [("reserved".into(), "m1".into(), 1)], "{how}");
        // The old behavior replays through the tiers; the fix settles at
        // the next pass.
        a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
            .unwrap();
        assert_eq!(
            effects(&h),
            [("superseded".into(), "m1".into(), 1)],
            "{how}"
        );
        assert_eq!(
            items(&h),
            [(
                format!("member_left:{id}"),
                format!(
                    "Member m1 was {how} before the hand-out of task 1 reached it; the task is offered to another member."
                )
            )],
            "{how}"
        );
        // No failure row for the task, and the next pass hands the task to
        // the other free member.
        row(&p, &h, "m2", &[("activity", json!("idle"))]);
        a.tick(&mut h.store, &mut p, TEAM, T0 + 2 * MINUTE, false)
            .unwrap();
        assert!(
            effects(&h)
                .iter()
                .any(|(s, m, n)| s == "submitted" && m == "m2" && *n == 1),
            "{how}: {:?}",
            effects(&h)
        );
        assert!(
            a.ledger
                .get(TEAM, &format!("gate:handout:1"))
                .unwrap()
                .is_none(),
            "{how}: no failure gate"
        );
        let _ = lead;
    }
}

/// With no other member free, the task waits in the queue after the
/// settlement; the unstaffed rules apply.
#[test]
fn a_settled_reservation_with_no_other_member_leaves_the_task_in_the_queue() {
    let (mut h, mut a, mut p, _lead) = team();
    let (mut a, _id) = reserved_then_left(&mut h, &mut a, &mut p, "paused");
    a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
        .unwrap();
    assert_eq!(items(&h).len(), 1, "{:?}", items(&h));
    assert!(effects(&h).iter().all(|(s, _, _)| s == "superseded"));
    // m2 stays busy; over the following passes nothing is handed out and
    // nothing more is told.
    for minute in 2..6 {
        a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
            .unwrap();
    }
    assert_eq!(items(&h).len(), 1);
    assert!(
        a.ledger.get(TEAM, "gate:handout:1").unwrap().is_none(),
        "no failure gate"
    );
}

/// An unbound member's reservation: the binding observation already
/// records the outcome as uncertain (zerus-63rru.55's path, before the
/// .85 settlement can see a reserved effect); the delivery row ends with
/// no failure row and nothing replays it.
#[test]
fn an_unbound_members_reservation_is_settled_by_the_binding_observation() {
    let (mut h, mut a, mut p, _lead) = team();
    let (mut a, _id) = reserved_then_left(&mut h, &mut a, &mut p, "unbound");
    a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
        .unwrap();
    assert_eq!(effects(&h), [("uncertain".into(), "m1".into(), 1)]);
    assert!(
        a.ledger.get(TEAM, "gate:handout:1").unwrap().is_none(),
        "no failure gate"
    );
    for minute in 2..6 {
        a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
            .unwrap();
    }
    assert!(
        a.ledger
            .prefix_page(TEAM, "delivery:", "")
            .unwrap()
            .is_empty(),
        "the row ended"
    );
}

/// A member that comes back before the next pass keeps its reservation:
/// nothing settles and the hand-out proceeds.
#[test]
fn a_member_that_returns_before_the_next_pass_keeps_its_reservation() {
    let (mut h, mut a, mut p, lead) = team();
    let (mut a, _id) = reserved_then_left(&mut h, &mut a, &mut p, "paused");
    h.run(
        &lead,
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
        .unwrap();
    // The returned member keeps its reservation: nothing settles through
    // the .85 rule and nothing is told; the interrupted delivery's own
    // rule (the service restarted) records the uncertain outcome, which
    // is never replayed.
    assert!(effects(&h)
        .iter()
        .any(|(s, m, n)| s == "uncertain" && m == "m1" && *n == 1));
    // Nothing settles through the .85 rule: no member_left item; the
    // uncertain delivery's own notice may appear.
    assert!(items(&h).iter().all(|(kind, _)| !kind.starts_with("member_left:")));
}

/// A crash between the reservation and the pass: the restarted pass settles
/// the reservation all the same.
#[test]
fn a_crash_between_the_reservation_and_the_pass_still_settles_it() {
    let (mut h, mut a, mut p, _lead) = team();
    let (mut restarted, id) = reserved_then_left(&mut h, &mut a, &mut p, "stopped");
    restarted
        .tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
        .unwrap();
    assert_eq!(
        items(&h),
        [(
            format!("member_left:{id}"),
            "Member m1 was stopped before the hand-out of task 1 reached it; the task is offered to another member.".into()
        )]
    );
    assert!(effects(&h).iter().all(|(s, _, _)| s == "superseded"));
}
