//! zerus-63rru.102 round 2: a stopped or archived team settles a hosted
//! delivery whose actor outcome was never recorded, only from a durable
//! outcome it already holds; with neither, the delivery stays unknown.
//! Every store row comes from the product's own writers: the real Create,
//! bind and Add, the actor's real send up to a crash boundary, the child's
//! own Ledger::reserve_input in its data shape, the person's real Stop and
//! Archive, and ArchiveMember.
use super::*;
use crate::teams::ops::Operation;
use serde_json::json;

fn input_kept(h: &Harness, id: &str) -> bool {
    super::super::ledger::Ledger::open(h.temp.path())
        .unwrap()
        .input(TEAM, &format!("hosted-input:{id}"))
        .unwrap()
        .is_some()
}
#[derive(Debug)]
struct Case {
    point: &'static str,
    remove_member: bool,
    archive: bool,
}
fn run_case(case: &Case) -> (bool, bool, bool, String, bool, bool) {
    let mut h = Harness::empty();
    let mut create = create();
    if let Operation::Create { bundle, .. } = &mut create {
        bundle
            .roles
            .iter_mut()
            .find(|r| r.id == "developer")
            .unwrap()
            .agent = "dsh".into();
    }
    h.run(&person(), create).unwrap();
    h.bind("m1");
    h.add(&person(), 1);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let mut p = super::core::Fake::new();
    super::super::delivery::CRASH.with(|c| c.set(Some(case.point)));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.begin_pass();
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap()
    }))
    .is_err());
    let row = a
        .ledger
        .prefix_page(TEAM, "delivery:", "")
        .unwrap()
        .remove(0);
    let id = row.data["id"].as_str().unwrap().to_string();
    let persisted_outcome = !row.data["outcome"].is_null();
    drop(a);
    super::super::ledger::Ledger::open(h.temp.path())
        .unwrap()
        .reserve_input(
            TEAM,
            &format!("hosted-input:{id}"),
            json!({"input": {"text": "retained task text"}, "to": null, "body": {"id": id},
                   "reply": {"schema": 1, "id": id, "team": TEAM, "node": 1, "member": "m1",
                             "state": "submitted", "receipt": {"request_id": id}}}),
        )
        .unwrap();
    if case.remove_member {
        h.run(
            &person(),
            Operation::ArchiveMember {
                member: "m1".into(),
            },
        )
        .unwrap();
    }
    h.run(&person(), Operation::Stop).unwrap();
    if case.archive {
        h.run(&person(), Operation::Archive).unwrap();
    }
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let team = h.store.team(TEAM).unwrap().unwrap();
    for pass in 0..4i64 {
        a.begin_pass();
        a.tick(&mut h.store, &mut p, TEAM, 1_000_000 + pass * 900_000, false)
            .unwrap();
    }
    for pass in 0..4i64 {
        a.terminal_deliveries(&mut h.store, &team, 5_000_000 + pass)
            .unwrap();
    }
    for pass in 0..2i64 {
        a.terminal_cancels(&mut h.store, &mut p, 6_000_000 + pass * 900_000)
            .unwrap();
    }
    for _ in 0..4 {
        a.maintain(&h.store).unwrap();
    }
    let row_kept = a
        .ledger
        .get(TEAM, &format!("delivery:{id}"))
        .unwrap()
        .is_some();
    let gate_gone = a.ledger.get(TEAM, "handout:1").unwrap().is_none();
    let reserved = h.store.effect_reserved(TEAM, &id).unwrap();
    let state = h
        .store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .find(|e| e.request_id == id)
        .map(|e| e.state)
        .unwrap_or_default();
    drop(a);
    let input = input_kept(&h, &id);
    (
        row_kept,
        input,
        reserved,
        state,
        persisted_outcome,
        gate_gone,
    )
}

/// P2-1: every table row from the reviewer's probe settles from the durable
/// outcome it holds (the child's retained submitted reply), records it as a
/// fact, and removes the row and the input.
#[test]
fn the_terminal_team_settles_each_probe_row_from_its_durable_outcome() {
    for case in [
        Case {
            point: "effect",
            remove_member: false,
            archive: false,
        },
        Case {
            point: "effect",
            remove_member: true,
            archive: true,
        },
        Case {
            point: "outcome",
            remove_member: false,
            archive: false,
        },
        Case {
            point: "outcome",
            remove_member: true,
            archive: true,
        },
    ] {
        let (row_kept, input, reserved, state, persisted, gate_gone) = run_case(&case);
        assert!(!row_kept, "{case:?}: the delivery row stayed");
        assert!(!input, "{case:?}: the input stayed");
        assert!(!reserved, "{case:?}: the effect stayed reserved");
        assert!(
            state == "submitted" || state == "superseded",
            "{case:?}: {state}"
        );
        assert!(gate_gone, "{case:?}: the hand-out gate stayed");
        let _ = persisted;
    }
}

/// P2-1: with neither a persisted outcome nor a retained reply, the delivery
/// stays unknown: keep the row and the input, never replay.
#[test]
fn an_unknown_delivery_without_a_durable_outcome_stays_unknown() {
    let mut h = Harness::empty();
    let mut create = create();
    if let Operation::Create { bundle, .. } = &mut create {
        bundle
            .roles
            .iter_mut()
            .find(|r| r.id == "developer")
            .unwrap()
            .agent = "dsh".into();
    }
    h.run(&person(), create).unwrap();
    h.bind("m1");
    h.add(&person(), 1);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let mut p = super::core::Fake::new();
    super::super::delivery::CRASH.with(|c| c.set(Some("effect")));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.begin_pass();
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap()
    }))
    .is_err());
    let row = a
        .ledger
        .prefix_page(TEAM, "delivery:", "")
        .unwrap()
        .remove(0);
    let id = row.data["id"].as_str().unwrap().to_string();
    drop(a);
    super::super::ledger::Ledger::open(h.temp.path())
        .unwrap()
        .reserve_input(
            TEAM,
            &format!("hosted-input:{id}"),
            json!({"input": {"text": "retained task text"}, "to": null, "body": {"id": id},
                   "reply": null}),
        )
        .unwrap();
    h.run(&person(), Operation::Stop).unwrap();
    h.run(&person(), Operation::Archive).unwrap();
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let team = h.store.team(TEAM).unwrap().unwrap();
    for pass in 0..4i64 {
        a.begin_pass();
        a.tick(&mut h.store, &mut p, TEAM, 1_000_000 + pass * 900_000, false)
            .unwrap();
    }
    for pass in 0..4i64 {
        a.terminal_deliveries(&mut h.store, &team, 5_000_000 + pass)
            .unwrap();
    }
    for _ in 0..4 {
        a.maintain(&h.store).unwrap();
    }
    assert!(
        a.ledger
            .get(TEAM, &format!("delivery:{id}"))
            .unwrap()
            .is_some(),
        "the unknown delivery's row was removed"
    );
    assert!(
        input_kept(&h, &id),
        "the unknown delivery's input was removed"
    );
    assert!(
        h.store.effect_reserved(TEAM, &id).unwrap(),
        "the unknown delivery's effect was settled"
    );
}

/// P3-4: the running path's not-reserved removal in recover_delivery is
/// pinned: a running team's delivery whose effect the store superseded
/// (ArchiveMember) leaves no row and no input on the next tick.
#[test]
fn the_running_teams_not_reserved_delivery_removes_the_input() {
    let mut h = Harness::empty();
    let mut create = create();
    if let Operation::Create { bundle, .. } = &mut create {
        bundle
            .roles
            .iter_mut()
            .find(|r| r.id == "developer")
            .unwrap()
            .agent = "dsh".into();
    }
    h.run(&person(), create).unwrap();
    h.bind("m1");
    h.add(&person(), 1);
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let mut p = super::core::Fake::new();
    super::super::delivery::CRASH.with(|c| c.set(Some("effect")));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.begin_pass();
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap()
    }))
    .is_err());
    let row = a
        .ledger
        .prefix_page(TEAM, "delivery:", "")
        .unwrap()
        .remove(0);
    let id = row.data["id"].as_str().unwrap().to_string();
    drop(a);
    super::super::ledger::Ledger::open(h.temp.path())
        .unwrap()
        .reserve_input(
            TEAM,
            &format!("hosted-input:{id}"),
            json!({"input": {"text": "retained task text"}, "to": null, "body": {"id": id},
                   "reply": null}),
        )
        .unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    a.begin_pass();
    a.tick(&mut h.store, &mut p, TEAM, 1_000_000, false).unwrap();
    assert!(
        a.ledger
            .get(TEAM, &format!("delivery:{id}"))
            .unwrap()
            .is_none(),
        "the not-reserved delivery row stayed"
    );
    assert!(!input_kept(&h, &id), "the not-reserved input stayed");
}
