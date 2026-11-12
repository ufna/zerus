//! Cancel decisions carried out by the service, and their order with the
//! cleanup of a stopped team's task keys.
use super::*;
use crate::teams::store::timers::PendingCancel;

fn record(node: u32, decision: u64, state: CancelState, reason: &str) -> Operation {
    Operation::RecordCancel {
        node,
        decision,
        state,
        reason: reason.into(),
        attempt: 0,
    }
}
fn pending(h: &Harness) -> Vec<PendingCancel> {
    h.store.pending_cancels(TEAM, 200, 0).unwrap()
}
fn decision(h: &Harness, node: u32) -> u64 {
    pending(h)
        .into_iter()
        .find(|p| p.node == node)
        .unwrap()
        .decision
}
fn outcomes(h: &Harness) -> u32 {
    let db = h.store.db.as_ref().unwrap();
    db.query_row("SELECT count(*) FROM cancel_outcomes", [], |r| r.get(0))
        .unwrap()
}
/// A lead's Cancel of a running task, still pending.
fn cancelling() -> (Harness, u32, Actor) {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    )
    .unwrap();
    (h, node, lead)
}

#[test]
fn a_leads_cancel_is_pending_once_with_its_task() {
    let (h, node, _) = cancelling();
    let rows = pending(&h);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].node, rows[0].generation, rows[0].task.clone()),
        (node, 1, Some(task(1)))
    );
}

#[test]
fn the_operation_then_the_observation_end_the_node_once() {
    let (mut h, node, _) = cancelling();
    let d = decision(&h, node);
    h.run(&service(), record(node, d, CancelState::Cancelled, ""))
        .unwrap();
    assert_eq!(h.node(node).state, NodeState::Cancelled);
    assert!(pending(&h).is_empty());
    assert!(h
        .store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .all(|e| e.state != "submitted"));
    let before = h.node(node);
    h.cancel_fact(node);
    assert_eq!(
        h.node(node),
        before,
        "the observation finds nothing left to do"
    );
    refused(
        h.run(&service(), record(node, d, CancelState::Cancelled, "")),
        code::INVALID_STATE,
    );
}

#[test]
fn the_observation_then_the_operation_which_writes_nothing() {
    let (mut h, node, _) = cancelling();
    let d = decision(&h, node);
    h.cancel_fact(node);
    assert_eq!(h.node(node).state, NodeState::Cancelled);
    assert!(pending(&h).is_empty());
    let rev = h.store.team(TEAM).unwrap().unwrap().revision;
    refused(
        h.run(&service(), record(node, d, CancelState::Cancelled, "")),
        code::INVALID_STATE,
    );
    assert_eq!(h.store.team(TEAM).unwrap().unwrap().revision, rev);
    assert_eq!(outcomes(&h), 0);
}

#[test]
fn a_refused_cancel_in_a_live_team_goes_back_to_the_lead() {
    let (mut h, node, lead) = cancelling();
    let d = decision(&h, node);
    refused(
        h.run(&service(), record(node, d, CancelState::Refused, " ")),
        code::INVALID_STATE,
    );
    refused(
        h.run(&service(), record(node, d, CancelState::Cancelled, "why")),
        code::INVALID_STATE,
    );
    refused(
        h.run(
            &lead,
            record(node, d, CancelState::Refused, "Delivery unknown"),
        ),
        code::NOT_ALLOWED,
    );
    h.run(
        &service(),
        record(node, d, CancelState::Refused, "Delivery unknown"),
    )
    .unwrap();
    let item = h
        .store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .find(|i| i.kind.starts_with("cancel_refused:"))
        .unwrap();
    assert!(item.text.contains(&node.to_string()) && item.text.contains("Delivery unknown"));
    assert_eq!(h.node(node).state, NodeState::InProgress);
    assert!(
        h.node(node).hold,
        "nothing is handed out until the lead decides again"
    );
    assert!(pending(&h).is_empty());
    refused(
        h.run(&service(), record(node, d, CancelState::Refused, "Again")),
        code::INVALID_STATE,
    );
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Try again".into(),
        },
    )
    .unwrap();
    assert_eq!(pending(&h).len(), 1, "the lead may decide again");
}

#[test]
fn a_cancel_of_an_older_generation_is_not_pending_or_recordable() {
    let (mut h, node, _) = cancelling();
    let d = decision(&h, node);
    let db = h.store.db.as_ref().unwrap();
    db.execute(
        "UPDATE nodes SET generation=generation+1 WHERE team=?1 AND id=?2",
        rusqlite::params![TEAM, node],
    )
    .unwrap();
    assert!(pending(&h).is_empty());
    refused(
        h.run(&service(), record(node, d, CancelState::Cancelled, "")),
        code::INVALID_STATE,
    );
}

/// A stopped team with tasks in every stage and a stage node without a task.
fn stopped() -> (Harness, Vec<u32>) {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let waiting = h.enqueue(&person(), 1);
    let pending = h.add(&person(), 2);
    let running = h.add(&person(), 3);
    h.submit(running, "m1");
    let reported = h.add(&person(), 4);
    h.submit(reported, "m2");
    h.report(reported, "m2");
    h.run(&person(), Operation::Stop).unwrap();
    (h, vec![waiting, pending, running, reported])
}

#[test]
fn a_stop_cancels_first_and_clears_each_key_only_after_its_cancel_settles() {
    let (mut h, nodes) = stopped();
    let rows = pending(&h);
    assert_eq!(rows.iter().map(|p| p.node).collect::<Vec<_>>(), nodes);
    assert!(rows.iter().all(|p| p.task.is_some()));
    assert!(
        h.store.keyed_tasks(TEAM, 0, 200).unwrap().is_empty(),
        "no key before its cancel"
    );
    let before = h.node(nodes[2]);
    h.run(
        &service(),
        record(nodes[2], rows[2].decision, CancelState::Cancelled, ""),
    )
    .unwrap();
    assert_eq!(
        h.node(nodes[2]).state,
        before.state,
        "after a stop nothing else of the node changes"
    );
    h.run(
        &service(),
        record(
            nodes[3],
            rows[3].decision,
            CancelState::Refused,
            "Delivery unknown",
        ),
    )
    .unwrap();
    assert_eq!(
        h.store.cancel_refusals(TEAM, (0, 0), 200).unwrap(),
        vec![(nodes[3], rows[3].decision, "Delivery unknown".to_owned())]
    );
    assert!(h
        .store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .all(|i| !i.kind.starts_with("cancel_refused")));
    let keyed: Vec<u32> = h
        .store
        .keyed_tasks(TEAM, 0, 200)
        .unwrap()
        .iter()
        .map(|k| k.node)
        .collect();
    assert_eq!(
        keyed,
        nodes[2..].to_vec(),
        "the page takes a node once its cancel is recorded"
    );
    for node in &nodes[2..] {
        h.run(
            &service(),
            Operation::RecordKeyClear {
                node: *node,
                state: KeyClearState::Cleared,
                reason: String::new(),
            },
        )
        .unwrap();
    }
    assert_eq!(
        h.store.keyed_teams("", 10).unwrap(),
        vec![TEAM.to_owned()],
        "two cancels still wait"
    );
    for (node, row) in nodes[..2].iter().zip(&rows) {
        h.run(
            &service(),
            record(*node, row.decision, CancelState::Cancelled, ""),
        )
        .unwrap();
    }
    assert_eq!(
        h.store.keyed_teams("", 10).unwrap(),
        vec![TEAM.to_owned()],
        "two keys still wait"
    );
    for node in &nodes[..2] {
        h.run(
            &service(),
            Operation::RecordKeyClear {
                node: *node,
                state: KeyClearState::Cleared,
                reason: String::new(),
            },
        )
        .unwrap();
    }
    assert!(
        h.store.keyed_teams("", 10).unwrap().is_empty(),
        "only at the end"
    );
}

#[test]
fn a_node_without_a_task_is_cancelled_without_a_child() {
    let mut create = create();
    if let Operation::Create { bundle, .. } = &mut create {
        let catalog = zerus_team_plan::builtin();
        bundle.roles = catalog.roles;
        bundle.pipelines = catalog.pipelines;
        bundle.template.default_pipeline = "reviewed".into();
    }
    let mut p = Harness::empty();
    p.run(&person(), create).unwrap();
    for member in ["lead", "m1", "m3"] {
        p.bind(member);
    }
    let parent = p.add(&person(), 1);
    p.submit(parent, "m1");
    p.report(parent, "m1");
    p.run(&person(), Operation::Stop).unwrap();
    let rows = pending(&p);
    let stage = rows.iter().find(|r| r.node != parent).unwrap().clone();
    let first = rows.iter().find(|r| r.node == parent).unwrap().clone();
    assert_eq!(stage.task, None, "the wanted stage has no task");
    p.run(
        &service(),
        record(parent, first.decision, CancelState::Cancelled, ""),
    )
    .unwrap();
    let clear = Operation::RecordKeyClear {
        node: parent,
        state: KeyClearState::Cleared,
        reason: String::new(),
    };
    p.run(&service(), clear).unwrap();
    assert_eq!(
        p.store.keyed_teams("", 10).unwrap(),
        vec![TEAM.to_owned()],
        "the team stays while a cancel waits, even one without a key"
    );
    p.run(
        &service(),
        record(stage.node, stage.decision, CancelState::Cancelled, ""),
    )
    .unwrap();
    assert!(p.store.keyed_tasks(TEAM, 0, 200).unwrap().is_empty());
    assert!(p.store.keyed_teams("", 10).unwrap().is_empty());
}

#[test]
fn pending_cancels_page_in_node_order() {
    let (h, nodes) = stopped();
    let mut seen = Vec::new();
    let mut after = 0;
    loop {
        let page = h.store.pending_cancels(TEAM, 1, after).unwrap();
        let Some(row) = page.first() else { break };
        after = row.node;
        seen.push(row.node);
    }
    assert_eq!(seen, nodes);
    refused(h.store.pending_cancels(TEAM, 201, 0), code::OVER_CAP);
}

#[test]
fn a_page_boundary_between_two_refused_cancels_of_one_node_loses_neither() {
    let (mut h, node, lead) = cancelling();
    let first = decision(&h, node);
    h.run(
        &service(),
        record(node, first, CancelState::Refused, "Delivery unknown"),
    )
    .unwrap();
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Try again".into(),
        },
    )
    .unwrap();
    let second = decision(&h, node);
    h.run(
        &service(),
        record(node, second, CancelState::Refused, "Still unknown"),
    )
    .unwrap();
    let all = h.store.cancel_refusals(TEAM, (0, 0), 200).unwrap();
    assert_eq!(all.len(), 2);
    let mut paged = Vec::new();
    let mut after = (0, 0);
    while let Some(row) = h.store.cancel_refusals(TEAM, after, 1).unwrap().pop() {
        after = (row.0, row.1);
        paged.push(row);
    }
    assert_eq!(paged, all, "a page of one returns both rows of the node");
    assert_eq!((all[0].1, all[1].1), (first, second));
}
