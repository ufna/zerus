//! zerus-63rru.71: a task already handed out stays ready in the kernel's
//! view until its observation moves, and the store refuses a second intent
//! for it. The first selection holds it, so it takes no free member from a
//! task that waits and its refusal never counts as a failed hand-out.
//! Through the production Native, the real tick and the producers' rows.
use super::super::Runtime;
use super::real_rows::Production;
use super::rows::seed_row;
use crate::teams::ops::tests::*;
use serde_json::{json, Value};

const MINUTE: i64 = 60_000;
const T0: i64 = 10_000_000;

/// Commits `member`'s row: the producer's idle row of its agent, with its
/// identity and `changes`.
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

/// m1 and m2 bound, `tasks` ready, m2 working: the first pass hands task 1
/// to m1. Returns the harness, the actor and the ports after that pass.
fn first_handed(tasks: u32) -> (Harness, Runtime, Production) {
    let mut h = Harness::new();
    h.bind("m1");
    h.bind("m2");
    for n in 1..=tasks {
        h.add(&person(), n);
    }
    let native = super::production::native(&h);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = Runtime::open(h.temp.path(), &machine).unwrap();
    let mut p = Production {
        native,
        deliveries: vec![],
    };
    row(&p, &h, "m2", &[("activity", json!("busy"))]);
    a.tick(&mut h.store, &mut p, TEAM, T0, false).unwrap();
    assert_eq!(p.deliveries.len(), 1, "{:?}", p.deliveries);
    assert!(p.deliveries[0].starts_with("m1 -> Outcome(Submitted"));
    // The task stays ready until its observation moves; m2 is free now.
    row(&p, &h, "m2", &[]);
    (h, a, p)
}
fn handed(h: &Harness) -> Vec<(u32, String)> {
    h.store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .map(|e| (e.node, e.member))
        .collect()
}
fn blocked(h: &Harness) -> usize {
    h.store
        .cards(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .filter(|c| c.kind == "team_actor_blocked")
        .count()
}

/// The second ready task goes to m2 on the first pass m2 is free; the task
/// m1 holds is not offered again.
#[test]
fn a_waiting_task_takes_the_next_free_member_at_once() {
    let (mut h, mut a, mut p) = first_handed(2);
    a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
        .unwrap();
    assert_eq!(p.deliveries.len(), 2, "{:?}", p.deliveries);
    assert!(p.deliveries[1].starts_with("m2 -> Outcome(Submitted"));
    assert_eq!(handed(&h), [(1, "m1".to_owned()), (2, "m2".to_owned())]);
    assert!(a.ledger.get(TEAM, "gate:handout:1").unwrap().is_none());
    assert_eq!(blocked(&h), 0);
}

/// With no second task, the handed task is never offered to the free m2:
/// no refusal, no failure row, no card, however long its observation stays
/// as it was.
#[test]
fn a_handed_task_is_never_refused_into_a_failure_card() {
    let (mut h, mut a, mut p) = first_handed(1);
    for minute in 1..=20 {
        a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
            .unwrap();
        assert!(
            a.ledger.get(TEAM, "gate:handout:1").unwrap().is_none(),
            "minute {minute}"
        );
    }
    assert_eq!(p.deliveries.len(), 1, "{:?}", p.deliveries);
    assert_eq!(handed(&h), [(1, "m1".to_owned())]);
    assert_eq!(blocked(&h), 0);
}
