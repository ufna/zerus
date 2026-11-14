//! zerus-63rru.102: a retained hosted input ends with its effect's
//! settlement, on the running path and on a terminal team.
use super::*;
use serde_json::{json, Value};

fn delivery_row(node: u32, member: &str, id: &str, not_started: bool) -> Value {
    json!({
        "node": node, "member": member, "id": id, "kind": "handout",
        "intent": null, "outcome": null, "receipt": null, "gated": false,
        "not_started": not_started, "reads": 0, "nonstarts": 0,
        "waiting": false, "dialog": null, "project": false, "probe": false,
        "command": false, "stamp": null,
    })
}
fn has_input(a: &super::super::Runtime, id: &str) -> bool {
    a.ledger
        .input(TEAM, &format!("hosted-input:{id}"))
        .unwrap()
        .is_some()
}

/// Decision 2: the success path's ops::apply removes the input row. The
/// terminal settle runs the same apply; the mutant deleting remove_input
/// kills this test.
#[test]
fn the_settled_effects_input_row_is_gone_after_the_outcomes_apply() {
    let mut h = Harness::new();
    // The real store rows the outcome's apply reads: the node, the member
    // and the effect the terminal settle records against.
    let node = h.add(&person(), 1);
    h.bind("m1");
    let intent = h.intent(node, "m1");
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE teams SET status=?2 WHERE id=?1",
            rusqlite::params![TEAM, serde_json::to_string(&TeamStatus::Stopped).unwrap()],
        )
        .unwrap();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    a.ledger
        .reserve(TEAM, "delivery:x", delivery_row(node, "m1", &intent, true))
        .unwrap();
    a.ledger
        .reserve(TEAM, &format!("hosted-input:{intent}"), json!({"input": "retained"}))
        .unwrap();
    let mut store = Store::open(h.temp.path()).unwrap();
    let team = store.team(TEAM).unwrap().unwrap();
    for _ in 0..4 {
        a.terminal_deliveries(&mut store, &team, 100).unwrap();
    }
    assert!(!has_input(&a, &intent), "the input outlived the settled outcome");
}

/// Decision 1: the stop landing between the hosted child's settlement and
/// the actor's outcome leaves a row with no outcome and no reserved effect;
/// the terminal pass removes it and its input, nothing is replayed.
#[test]
fn a_stop_between_the_childs_settlement_and_the_outcome_removes_the_input() {
    let h = Harness::new();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE teams SET status=?2 WHERE id=?1",
            rusqlite::params![TEAM, serde_json::to_string(&TeamStatus::Stopped).unwrap()],
        )
        .unwrap();
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    a.ledger
        .reserve(TEAM, "delivery:y", delivery_row(1, "m1", "y", false))
        .unwrap();
    a.ledger
        .reserve(TEAM, "hosted-input:y", json!({"input": "retained"}))
        .unwrap();
    let mut store = Store::open(h.temp.path()).unwrap();
    let team = store.team(TEAM).unwrap().unwrap();
    for _ in 0..4 {
        a.terminal_deliveries(&mut store, &team, 100).unwrap();
    }
    assert!(!has_input(&a, "y"), "the input outlived the settled effect");
    assert!(
        a.ledger.get(TEAM, "delivery:y").unwrap().is_none(),
        "the settled row stayed"
    );
}
