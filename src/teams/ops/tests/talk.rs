use super::*;
#[test]
fn questions_are_bound_to_the_effect_holder_and_capped_per_attempt() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let m1 = h.bind("m1");
    let m2 = h.bind("m2");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    refused(h.run(&m2, ask(node, "Other attempt")), code::NOT_ALLOWED);
    let mut first = 0;
    for n in 0..3 {
        let item = h
            .run(&m1, ask(node, format!("Question {n}")))
            .unwrap()
            .item
            .unwrap();
        if n == 0 {
            first = item;
        }
    }
    refused(h.run(&m1, ask(node, "Overflow")), code::OVER_CAP);
    h.run(
        &lead,
        Operation::Reply {
            item: first,
            text: "Use the documented behavior".into(),
        },
    )
    .unwrap();
    h.run(&m1, ask(node, "Replacement")).unwrap();
    refused(
        h.run(
            &lead,
            Operation::Reply {
                item: first,
                text: "Again".into(),
            },
        ),
        code::INVALID_STATE,
    );
}
#[test]
fn delegation_revocation_blocks_new_accept_and_reply_decisions() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.settings(|s| {
        s.delegations.accept_results = false;
        s.delegations.answer_members = false;
    });
    refused(h.run(&lead, Operation::Accept { node }), code::NOT_ALLOWED);
    refused(
        h.run(
            &lead,
            Operation::Reply {
                item: 1,
                text: "Answer".into(),
            },
        ),
        code::NOT_ALLOWED,
    );
}
#[test]
fn owner_card_records_proposer_and_person_decision_and_refuses_a_second_decision() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let card = h
        .run(
            &lead,
            Operation::OpenCard {
                kind: "proposal".into(),
                text: "Add a member?".into(),
                node: None,
            },
        )
        .unwrap()
        .item
        .unwrap();
    h.run(
        &person(),
        Operation::DecideCard {
            card,
            decision: "Keep the roster".into(),
        },
    )
    .unwrap();
    refused(
        h.run(
            &person(),
            Operation::DecideCard {
                card,
                decision: "Again".into(),
            },
        ),
        code::INVALID_STATE,
    );
    let c = h.store.cards(TEAM, 200, None).unwrap().rows.pop().unwrap();
    assert_eq!(c.actor["person"], "local");
    assert_eq!(c.decided_actor.unwrap()["person"], "local");
}

fn ask(node: u32, body: impl Into<String>) -> Operation {
    Operation::Ask {
        node,
        generation: 1,
        text: body.into(),
    }
}
