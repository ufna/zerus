//! Every operation that ends a timer's condition closes its card in the same
//! transaction, written as the service's own CloseCard writes it.
use super::*;

fn waiting_card(h: &mut Harness) -> (u32, Actor, u64, u64) {
    let (node, lead, item) = waiting_report(h);
    let op = timer(TimerKind::RemindPerson, node, 1, item, 0);
    let card = at(h, &service(), op, T0 + 60 * MINUTE)
        .unwrap()
        .item
        .unwrap();
    (node, lead, item, card)
}
fn silent_card(h: &mut Harness, node: u32, item: u64) -> u64 {
    let generation = h.store.generation(TEAM, node).unwrap().unwrap();
    let op = timer(TimerKind::SilentLead, node, generation, item, T0);
    at(h, &service(), op, T0 + 30 * MINUTE)
        .unwrap()
        .item
        .unwrap()
}
fn card(h: &Harness, seq: u64) -> crate::teams::store::Card {
    h.store.card(TEAM, seq).unwrap().unwrap()
}
fn assert_settled(h: &Harness, seq: u64) {
    let c = card(h, seq);
    assert_eq!(
        (c.state.as_str(), c.decision.as_deref()),
        ("closed", Some("stale"))
    );
    assert_eq!(c.decided_actor.as_ref(), Some(&c.actor));
}

#[test]
fn a_trigger_closed_card_equals_a_card_closed_by_the_service() {
    let mut h = Harness::new();
    let (node, lead, _, timed) = waiting_card(&mut h);
    let open = Operation::OpenCard {
        kind: "standing_confirmation".into(),
        text: "Example card".into(),
        node: Some(node),
    };
    let coded = h.run(&service(), open).unwrap().item.unwrap();
    let close = Operation::CloseCard {
        card: coded,
        reason: "Overtaken by events".into(),
    };
    h.run(&service(), close).unwrap();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let (a, b) = (card(&h, coded), card(&h, timed));
    assert_eq!(
        (a.state, a.decision, a.decided_actor, a.actor, a.node),
        (b.state, b.decision, b.decided_actor, b.actor, b.node)
    );
}

#[test]
fn each_decision_on_the_report_closes_its_reminder_card() {
    let decisions: [fn(u32) -> Operation; 4] = [
        |node| Operation::Accept { node },
        |node| Operation::Return {
            node,
            reason: "Fix the edge case".into(),
        },
        |node| Operation::Reassign {
            node,
            member: "m2".into(),
            reason: "Fresh eyes".into(),
        },
        |node| Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    ];
    for decide in decisions {
        let mut h = Harness::new();
        h.bind("m2");
        let (node, lead, _, seq) = waiting_card(&mut h);
        assert_eq!(card(&h, seq).state, "open");
        h.run(&lead, decide(node)).unwrap();
        assert_settled(&h, seq);
    }
}

#[test]
fn a_person_decision_closes_the_reminder_card_too() {
    let mut h = Harness::new();
    let (node, _, _, seq) = waiting_card(&mut h);
    h.run(&person(), Operation::Accept { node }).unwrap();
    assert_settled(&h, seq);
}

#[test]
fn the_silent_lead_card_closes_when_a_reply_closes_its_question() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let m1 = h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    let ask = Operation::Ask {
        node,
        generation: 1,
        text: "Which file?".into(),
    };
    let item = h.run(&m1, ask).unwrap().item.unwrap();
    let seq = silent_card(&mut h, node, item);
    let reply = Operation::Reply {
        item,
        text: "The documented one".into(),
    };
    h.run(&lead, reply).unwrap();
    assert_settled(&h, seq);
}

#[test]
fn the_silent_lead_card_closes_when_the_report_arrives() {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    let op = timer(TimerKind::NoReport, node, 1, 0, T0);
    let item = at(&mut h, &service(), op, T0 + 5 * MINUTE)
        .unwrap()
        .item
        .unwrap();
    let seq = silent_card(&mut h, node, item);
    h.report(node, "m1");
    assert_settled(&h, seq);
}

#[test]
fn the_silent_lead_card_closes_when_its_node_ends() {
    let mut h = Harness::new();
    let (node, lead, item) = waiting_report(&mut h);
    let seq = silent_card(&mut h, node, item);
    h.run(&lead, Operation::Accept { node }).unwrap();
    assert_eq!(
        card(&h, seq).state,
        "open",
        "the report still waits for its merge"
    );
    let decision = h.accept_decision(node);
    let fact = Operation::RecordIntegration {
        decision,
        node,
        commit: COMMIT.into(),
    };
    h.run(&service(), fact).unwrap();
    h.observe(node, TaskStage::Done);
    assert_settled(&h, seq);
}

#[test]
fn the_silent_lead_card_closes_when_its_node_is_cancelled() {
    let mut h = Harness::new();
    let (node, lead, item) = waiting_report(&mut h);
    let seq = silent_card(&mut h, node, item);
    let cancel = Operation::Cancel {
        node,
        reason: "Superseded".into(),
    };
    h.run(&lead, cancel).unwrap();
    h.cancel_fact(node);
    assert_settled(&h, seq);
}

#[test]
fn an_unrelated_item_or_card_is_not_touched() {
    let mut h = Harness::new();
    let (node, lead, item) = waiting_report(&mut h);
    let open = Operation::OpenCard {
        kind: "proposal".into(),
        text: "Keep this open".into(),
        node: Some(node),
    };
    let other = h.run(&lead, open).unwrap().item.unwrap();
    silent_card(&mut h, node, item);
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let fact = Operation::RecordIntegration {
        decision,
        node,
        commit: COMMIT.into(),
    };
    h.run(&service(), fact).unwrap();
    h.observe(node, TaskStage::Done);
    assert_eq!(card(&h, other).state, "open");
}
