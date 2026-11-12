//! zerus-63rru.45: a ready node that no member can take is told to the lead
//! once, and the item goes when a member can take the node again.
use super::*;
use crate::teams::store::{InboxItem, JournalEntry};

fn reconcile(h: &mut Harness) -> Result<Reply> {
    h.run(&service(), Operation::ReconcileUnstaffed)
}
fn unchanged(h: &mut Harness) {
    let e = reconcile(h).unwrap_err();
    assert_eq!(e.code, code::UNSTAFFED_UNCHANGED, "{e:?}");
}
/// The open unstaffed items, `(node, kind, text)`.
fn open(h: &Harness) -> Vec<(u32, String, String)> {
    h.store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .filter(|i| i.state == "open" && i.kind.starts_with("unstaffed:"))
        .map(|i| (i.node, i.kind, i.text))
        .collect()
}
fn row(h: &Harness, node: u32) -> Option<String> {
    h.store
        .queue_rows(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.node.id == node)
        .unwrap()
        .unstaffed
}
fn revision(h: &Harness) -> u64 {
    h.store.team(TEAM).unwrap().unwrap().revision
}
fn removed(h: &mut Harness, member: &str) {
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: member.into(),
        },
    )
    .unwrap();
}
fn snapshot(h: &Harness) -> (u64, Vec<JournalEntry>, Vec<InboxItem>) {
    (
        revision(h),
        h.store.journal(TEAM, 200, None).unwrap().rows,
        h.store.inbox(TEAM, 200, None).unwrap().rows,
    )
}
/// A Reassign settled at once (the task was never handed out), then its
/// assignee removed: the first probe of the survey.
fn stranded_assignee(h: &mut Harness) -> (u32, Actor) {
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
    removed(h, "m2");
    (node, lead)
}

#[test]
fn a_settled_reassigns_removed_assignee_is_told_to_the_lead_once() {
    let mut h = Harness::new();
    let (node, _) = stranded_assignee(&mut h);
    assert!(open(&h).is_empty(), "nothing told before the reconcile");
    let before = revision(&h);
    reconcile(&mut h).unwrap();
    let items = open(&h);
    assert_eq!(items.len(), 1);
    let (n, kind, text) = &items[0];
    assert_eq!(*n, node);
    assert_eq!(kind, &format!("unstaffed:{}", before + 1));
    assert_eq!(
        text,
        &format!("Task {node} is ready, but no member can take it: The assignee m2 is not a member with the role developer. Route it to another role, reassign or cancel it.")
    );
    assert_eq!(row(&h, node).as_deref(), Some(text.as_str()));
    // Told once: a second reconcile finds nothing to change.
    let after = snapshot(&h);
    unchanged(&mut h);
    assert_eq!(snapshot(&h), after);
}

#[test]
fn a_role_whose_members_are_all_removed_is_told_to_the_lead() {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let node = h.add(&person(), 1);
    removed(&mut h, "m1");
    removed(&mut h, "m2");
    reconcile(&mut h).unwrap();
    let items = open(&h);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, node);
    assert!(items[0]
        .2
        .contains("No member of the team has the role developer."));
}

/// One removal strands two nodes: one reconcile, one item each, under the
/// same kind; the inbox keys an item by team, kind, node and generation.
#[test]
fn two_nodes_stranded_by_one_removal_get_one_item_each() {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let a = h.add(&person(), 1);
    let b = h.add(&person(), 2);
    removed(&mut h, "m1");
    removed(&mut h, "m2");
    let before = revision(&h);
    reconcile(&mut h).unwrap();
    let items = open(&h);
    assert_eq!(items.iter().map(|i| i.0).collect::<Vec<_>>(), vec![a, b]);
    assert!(items
        .iter()
        .all(|i| i.1 == format!("unstaffed:{}", before + 1)));
    let seqs: std::collections::BTreeSet<u64> = h
        .store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .filter(|i| i.kind.starts_with("unstaffed:"))
        .map(|i| i.seq)
        .collect();
    assert_eq!(seqs.len(), 2);
}

/// Each way a node leaves the set closes its item, and the row clears: a
/// Route, a Reassign, a hold, and the task leaving the team.
#[test]
fn a_node_fixed_by_route_reassign_hold_or_leaving_closes_its_item() {
    for fix in ["route", "reassign", "hold", "leave"] {
        let mut h = Harness::new();
        let (node, lead) = stranded_assignee(&mut h);
        reconcile(&mut h).unwrap();
        assert_eq!(open(&h).len(), 1, "{fix}");
        match fix {
            "route" => {
                h.run(&lead, route(node)).unwrap();
            }
            "reassign" => {
                h.run(
                    &lead,
                    Operation::Reassign {
                        node,
                        member: "m1".into(),
                        reason: "m1 takes it".into(),
                    },
                )
                .unwrap();
            }
            "hold" => {
                h.run(&lead, Operation::Hold { node }).unwrap();
            }
            _ => {
                let mut o = h.observation(node, TaskStage::Todo);
                o.key = KeyState::Absent;
                h.run(
                    &service(),
                    Operation::ObserveTask {
                        node,
                        observation: o,
                    },
                )
                .unwrap();
            }
        }
        if fix == "leave" {
            // The node's end already closed its items.
            assert!(open(&h).is_empty(), "{fix}");
            unchanged(&mut h);
        } else {
            reconcile(&mut h).unwrap();
            assert!(open(&h).is_empty(), "{fix}");
            assert_eq!(row(&h, node), None, "{fix}");
            unchanged(&mut h);
        }
    }
}

/// A node that is fixed and then stranded again is told again, once.
#[test]
fn a_node_stranded_again_is_told_again_once() {
    let mut h = Harness::new();
    let (node, lead) = stranded_assignee(&mut h);
    reconcile(&mut h).unwrap();
    let first = open(&h)[0].1.clone();
    h.run(
        &lead,
        Operation::Reassign {
            node,
            member: "m1".into(),
            reason: "m1 takes it".into(),
        },
    )
    .unwrap();
    reconcile(&mut h).unwrap();
    assert!(open(&h).is_empty());
    removed(&mut h, "m1");
    reconcile(&mut h).unwrap();
    let again = open(&h);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].0, node);
    assert_ne!(again[0].1, first);
    // m2 went first, so no developer is left: the kernel names the role.
    assert!(again[0]
        .2
        .contains("No member of the team has the role developer."));
    unchanged(&mut h);
}

/// A hundred passes that find nothing to change write nothing: the revision,
/// the journal and the inbox stay as they are.
#[test]
fn a_hundred_reconciles_with_nothing_changed_write_nothing() {
    let mut h = Harness::new();
    stranded_assignee(&mut h);
    reconcile(&mut h).unwrap();
    let before = snapshot(&h);
    for _ in 0..100 {
        unchanged(&mut h);
    }
    assert_eq!(snapshot(&h), before);
}

/// A reconcile saved and replayed by its request ID answers its saved reply
/// and changes nothing.
#[test]
fn a_replayed_reconcile_answers_its_saved_reply_and_changes_nothing() {
    let mut h = Harness::new();
    stranded_assignee(&mut h);
    let r = request(revision(&h), Operation::ReconcileUnstaffed);
    let first = super::super::apply(&mut h.store, &service(), &r, 1234).unwrap();
    let after = snapshot(&h);
    let again = super::super::apply(&mut h.store, &service(), &r, 1234).unwrap();
    assert_eq!(again, first);
    assert_eq!(snapshot(&h), after);
}

/// Only the service reconciles, and only in a running team.
#[test]
fn only_the_service_reconciles_a_running_team() {
    let mut h = Harness::new();
    let (_, lead) = stranded_assignee(&mut h);
    refused(
        h.run(&lead, Operation::ReconcileUnstaffed),
        code::NOT_ALLOWED,
    );
    refused(
        h.run(&person(), Operation::ReconcileUnstaffed),
        code::NOT_ALLOWED,
    );
    h.run(&person(), Operation::Pause).unwrap();
    refused(reconcile(&mut h), code::INVALID_STATE);
    assert!(open(&h).is_empty());
}

/// One removal strands 500 ready nodes: one reconcile, 500 items. A 501st
/// open item makes the next reconcile fail closed with a named refusal, not
/// a silent cut, and the service's read fails the same way.
#[test]
fn a_removal_that_strands_500_nodes_is_one_reconcile_and_the_bound_fails_closed() {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let nodes: Vec<u32> = (1..=500).map(|n| h.add(&person(), n)).collect();
    removed(&mut h, "m1");
    removed(&mut h, "m2");
    reconcile(&mut h).unwrap();
    assert_eq!(h.store.open_unstaffed(TEAM).unwrap().len(), 500);
    assert_eq!(
        h.store.open_unstaffed(TEAM).unwrap(),
        nodes.iter().copied().collect()
    );
    let db = h.store.db.as_ref().unwrap();
    db.execute(
        "INSERT INTO inbox(team,kind,node,generation,actor,text,created_ms) VALUES(?1,'unstaffed:999999',9999,1,'{}','A forged 501st item',0)",
        params![TEAM],
    )
    .unwrap();
    let before = snapshot(&h);
    let e = reconcile(&mut h).unwrap_err();
    assert_eq!(e.code, code::OVER_CAP);
    assert!(
        e.message.contains("More than 500 open unstaffed items"),
        "{e:?}"
    );
    assert_eq!(snapshot(&h), before);
    let e = h.store.open_unstaffed(TEAM).unwrap_err();
    assert_eq!(e.code, code::OVER_CAP);
}

/// The plans and costs of the new reads at 1,000 and 10,000 settled items:
/// the open items and the queue page's `unstaffed` read the partial index
/// `inbox_unstaffed`, scan no table, and execute the same number of virtual
/// machine steps whatever the closed history. `--nocapture` prints them.
#[test]
fn the_unstaffed_reads_cost_the_same_at_1000_and_10000_settled_items() {
    use rusqlite::StatementStatus;
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    let a = h.add(&person(), 1);
    h.add(&person(), 2);
    removed(&mut h, "m1");
    removed(&mut h, "m2");
    reconcile(&mut h).unwrap();
    let db = h.store.db.as_ref().unwrap();
    let page = crate::teams::store::pages::node_page_sql("nodes", false);
    let run = |sql: &str, page: bool| -> (usize, i32, i32) {
        let mut s = db.prepare(sql).unwrap();
        let rows = if page {
            s.query_map(params![TEAM, 0, 1_000_000, 200, ""], |_| Ok(()))
                .unwrap()
                .count()
        } else {
            s.query_map(params![TEAM], |_| Ok(())).unwrap().count()
        };
        (
            rows,
            s.get_status(StatementStatus::VmStep),
            s.get_status(StatementStatus::FullscanStep),
        )
    };
    let plan = |sql: &str| -> Vec<String> {
        let mut s = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let nulls = vec![rusqlite::types::Null; s.parameter_count()];
        s.query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let settle = |from: usize, to: usize| {
        for i in from..to {
            db.execute(
                "INSERT INTO inbox(team,kind,node,generation,actor,text,state,created_ms) VALUES(?1,?2,?3,1,'{}','A settled episode','closed',0),(?1,?4,?3,1,'{}','A settled report','closed',0)",
                params![TEAM, format!("unstaffed:settled-{i}"), a, format!("report:settled-{i}")],
            )
            .unwrap();
        }
    };
    let open_items = crate::teams::store::unstaffed::OPEN_ITEMS;
    let mut costs = Vec::new();
    for (from, to) in [(0, 1_000), (1_000, 10_000)] {
        settle(from, to);
        let items = run(open_items, false);
        let rows = run(&page, true);
        println!(
            "UNSTAFFED-COST settled={to}: open items rows={} vm_steps={} fullscan_steps={}; queue page rows={} vm_steps={} fullscan_steps={}",
            items.0, items.1, items.2, rows.0, rows.1, rows.2
        );
        assert_eq!(items.0, 2);
        assert_eq!(rows.0, 2);
        assert_eq!((items.2, rows.2), (0, 0), "no table scanned");
        costs.push((items.1, rows.1));
    }
    assert_eq!(costs[0], costs[1], "the cost does not grow with history");
    for (name, sql) in [("OPEN_ITEMS", open_items), ("QUEUE_PAGE", page.as_str())] {
        let p = plan(sql);
        println!("UNSTAFFED-PLAN {name}:\n  {}", p.join("\n  "));
        assert!(
            p.iter().any(|l| l.contains("inbox_unstaffed")),
            "{name} reads the partial index: {p:?}"
        );
        assert!(
            !p.iter()
                .any(|l| l.starts_with("SCAN i") || l == "SCAN inbox"),
            "{name} scans no inbox: {p:?}"
        );
    }
}
