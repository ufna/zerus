use super::*;

/// The pinned CLI replies of package 5, copied beside this module so the
/// branch's tests run before package 5 merges; they are the same bytes as
/// `crates/team-plan/fixtures/cli-*.json` at its `fa46dcc08`.
fn fixture(name: &str) -> Value {
    let text = match name {
        "status" => include_str!("../fixtures/cli-status.json"),
        "queue" => include_str!("../fixtures/cli-queue.json"),
        "cards" => include_str!("../fixtures/cli-cards.json"),
        "waiting" => include_str!("../fixtures/cli-waiting.json"),
        other => panic!("no fixture {other}"),
    };
    serde_json::from_str(text).expect("a pinned CLI fixture parses")
}

const TEAM: &str = "00000000-0000-4000-8000-00000000beef";

fn group<'a>(view: &'a TeamView, state: &str) -> &'a BoardGroup {
    view.groups
        .iter()
        .find(|group| group.state == state)
        .expect("the board carries every group")
}

#[test]
fn the_pinned_status_and_queue_project_the_team() {
    let view = team(&fixture("status"), &fixture("queue"));
    assert_eq!(view.bar.id, TEAM);
    assert_eq!(view.bar.name, "Fixes");
    assert_eq!(view.bar.project, "example-project");
    assert_eq!(view.bar.status, "running");
    assert_eq!(view.bar.lead, "lead");
    assert_eq!(view.bar.goal, "");
    assert_eq!(view.bar.integration_branch, "zerus/team/fixes");
    assert_eq!(
        view.bar.base_commit,
        "0123456789abcdef0123456789abcdef01234567"
    );
    assert_eq!(view.bar.revision, 16);
    assert_eq!(view.revision, 16);
    assert!(!view.truncated);
    assert_eq!(view.next, None);
    assert_eq!(
        view.bar.counts,
        TeamCounts {
            members: MemberCounts {
                total: 4,
                not_created: 2,
                active: 2,
                paused: 0,
                removed: 0,
            },
            nodes: NodeCounts {
                waiting: 0,
                pending: 1,
                in_progress: 0,
                reported: 1,
                accepted: 0,
                cancelled: 0,
            },
            inbox: 1,
            cards: 1,
        }
    );

    // The members: two launched and two still not created.
    assert_eq!(
        view.members
            .iter()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        ["lead", "m1", "m2", "m3"]
    );
    assert_eq!(view.members[0].state, "active");
    assert!(view.members[0].lead);
    assert!(!view.members[1].lead);
    assert_eq!(view.members[1].role, "developer");
    assert_eq!(
        view.members[1].session,
        Some(SessionRef {
            machine: "00000000-0000-4000-8000-0000000000aa".into(),
            session_id: "launch_id:00000000-0000-4000-8000-000000000031".into(),
        })
    );
    assert_eq!(
        view.members[2].state, "not_created",
        "the CLI omits a default"
    );
    assert_eq!(view.members[2].account, "");
    assert_eq!(view.members[2].session, None);

    // Every group, in their order.
    assert_eq!(
        view.groups
            .iter()
            .map(|group| group.state.as_str())
            .collect::<Vec<_>>(),
        GROUPS
    );

    let pending = group(&view, "pending");
    assert_eq!(pending.cards.len(), 1);
    assert_eq!(pending.cards[0].node, 1);
    assert_eq!(pending.cards[0].title, "Task 1");
    assert_eq!(
        pending.cards[0].task,
        Some(TaskRefView {
            project: "example-project".into(),
            uuid: "00000000-0000-4000-8000-000000000001".into(),
        })
    );
    assert!(pending.cards[0].admitted);
    assert_eq!(pending.cards[0].round, 1);
    assert_eq!(pending.cards[0].rank, "a01");

    let reported = group(&view, "reported");
    assert_eq!(reported.cards.len(), 1);
    assert_eq!(reported.cards[0].node, 2);
    assert_eq!(reported.cards[0].implementer, "m1");
    assert!(reported.cards[0].stages.is_empty());
}

#[test]
fn a_card_shows_a_stage_in_each_state() {
    let mut rows = vec![serde_json::json!({
        "id": 1, "state": "reported", "title": "Fix it", "rank": "a0",
        "task": {"project": "p", "uuid": "u"},
    })];
    for (at, state) in [
        "waiting",
        "pending",
        "in_progress",
        "reported",
        "accepted",
        "cancelled",
    ]
    .into_iter()
    .enumerate()
    {
        rows.push(serde_json::json!({
            "id": at as u64 + 2, "state": state, "kind": "stage",
            "stage_of": 1, "stage_index": at as u64, "role": "reviewer",
        }));
    }
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": rows, "next": null});
    let view = team(&fixture("status"), &queue);
    let card = &group(&view, "reported").cards[0];
    assert_eq!(card.stages.len(), 6);
    assert_eq!(
        card.stages
            .iter()
            .map(|stage| stage.state.as_str())
            .collect::<Vec<_>>(),
        [
            "waiting",
            "pending",
            "in_progress",
            "reported",
            "accepted",
            "cancelled"
        ]
    );
    assert_eq!(
        card.stages
            .iter()
            .map(|stage| stage.index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn an_empty_queue_has_every_group_and_no_card() {
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [], "next": null});
    let view = team(&fixture("status"), &queue);
    assert_eq!(view.groups.len(), GROUPS.len());
    assert!(view.groups.iter().all(|group| group.cards.is_empty()));
    assert!(!view.truncated);
    assert_eq!(view.next, None);
}

#[test]
fn the_board_keeps_one_page_and_says_so() {
    let rows: Vec<Value> = (1..=250)
        .map(|id| {
            serde_json::json!({
                "id": id, "state": "pending", "title": format!("Task {id}"),
                "rank": format!("a{id:03}"),
            })
        })
        .collect();
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": rows,
        "next": "t1.bmV4dA"});
    let view = team(&fixture("status"), &queue);
    let cards = view.groups.iter().flat_map(|group| &group.cards).count();
    assert_eq!(cards, PAGE, "the view keeps one page");
    assert!(view.truncated);
    assert_eq!(
        view.next.as_deref(),
        Some("t1.bmV4dA"),
        "the next page's cursor is visible"
    );
}

#[test]
fn a_draft_node_without_a_state_is_grouped_as_a_draft() {
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [
        {"id": 9, "title": "Draw it up", "rank": "a0"},
    ], "next": null});
    let view = team(&fixture("status"), &queue);
    assert_eq!(group(&view, "draft").cards[0].node, 9);
}

#[test]
fn long_texts_are_cut() {
    let long = "x".repeat(TEXT_CHARS + 100);
    let mut status = fixture("status");
    status["team"]["goal"] = serde_json::json!(long);
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [
        {"id": 1, "title": long, "state": "pending"},
    ], "next": null});
    let view = team(&status, &queue);
    assert_eq!(view.bar.goal.chars().count(), TEXT_CHARS);
    assert_eq!(
        group(&view, "pending").cards[0].title.chars().count(),
        LINE_CHARS
    );
}

#[test]
fn a_stage_shows_its_own_worker_not_the_task_implementer() {
    // The wave-2 binding decision keeps a stage's `implementer` as the
    // person who implemented the task being checked; the stage's own
    // worker is its `assignee`.
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [
        {"id": 1, "state": "reported"},
        {"id": 2, "kind": "stage", "stage_of": 1, "state": "in_progress",
         "assignee": "reviewer", "implementer": "developer"},
    ], "next": null});
    let view = team(&fixture("status"), &queue);
    let card = view
        .groups
        .iter()
        .flat_map(|group| &group.cards)
        .find(|card| card.node == 1)
        .expect("the checked card");
    assert_eq!(card.stages[0].assignee, "reviewer");
    assert_eq!(card.stages[0].implementer, "developer");
}

#[test]
fn an_unknown_state_and_a_huge_revision_do_not_panic() {
    let mut status = fixture("status");
    status["team"]["status"] = serde_json::json!("future_state");
    status["team"]["revision"] = serde_json::json!(1e100);
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [
        {"id": 1, "state": "future_state", "title": "new", "round": -2},
    ], "next": null});
    let view = team(&status, &queue);
    assert_eq!(view.bar.status, "future_state");
    assert_eq!(view.revision, u64::MAX);
    assert!(view
        .groups
        .iter()
        .any(|group| group.state == "future_state" && group.cards.len() == 1));
    assert!(team(&Value::Null, &Value::Null).members.is_empty());
}

#[test]
fn a_stage_without_its_card_stays_a_card() {
    let queue = serde_json::json!({"ok": true, "team": TEAM, "rows": [
        {"id": 7, "state": "reported", "kind": "stage", "stage_of": 99,
         "stage_index": 0, "role": "reviewer", "title": "Review"},
    ], "next": null});
    let view = team(&fixture("status"), &queue);
    let card = &group(&view, "reported").cards[0];
    assert_eq!(card.node, 7);
    assert_eq!(card.title, "Review");
    assert!(
        card.stages.is_empty(),
        "the stage checks a card this page does not carry"
    );
}

#[test]
fn the_list_projects_the_teams_of_the_computer() {
    let list = list(&serde_json::json!({
        "ok": true,
        "actor": {"kind": "person"},
        "rows": [
            {"id": TEAM, "project": "example-project", "name": "Fixes",
             "status": "running", "lead": "lead",
             "home": "00000000-0000-4000-8000-0000000000aa", "revision": 16},
            {"id": "00000000-0000-4000-8000-00000000dead", "project": "example-project",
             "name": "Old", "status": "stopped", "lead": "lead",
             "home": "00000000-0000-4000-8000-0000000000aa", "revision": 4},
        ],
        "next": "t1.bmV4dA",
    }));
    assert_eq!(list.teams.len(), 2);
    assert!(!list.truncated);
    assert_eq!(list.next.as_deref(), Some("t1.bmV4dA"));
    assert_eq!(list.teams[0].id, TEAM);
    assert_eq!(list.teams[0].name, "Fixes");
    assert_eq!(list.teams[0].status, "running");
    assert_eq!(list.teams[0].lead, "lead");
    assert_eq!(list.teams[0].revision, 16);
    assert!(!list.teams[0].draft());
    assert_eq!(list.teams[1].status, "stopped");
}

#[test]
fn waiting_projects_the_open_owner_cards() {
    // The `waiting` read is the cards of every team, each row carrying its
    // team; the pinned cards reply gives the row.
    let cards = fixture("cards");
    let rows: Vec<Value> = cards["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row["team"] = serde_json::json!(TEAM);
            row
        })
        .collect();
    let view = waiting(&serde_json::json!({
        "ok": true,
        "actor": {"kind": "person"},
        "rows": rows,
        "next": null,
    }));
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.next, None);
    let item = &view.items[0];
    assert_eq!(item.id, format!("{TEAM}:1"));
    assert_eq!(item.card, 1);
    assert_eq!(item.kind, "note");
    assert_eq!(item.text, "Tell the team the release moved to Monday.");
    assert_eq!(item.state, "open");
    assert_eq!(item.by, "local");
    assert_eq!(item.node, None);
    assert_eq!(item.created_ms, 1700000000000);
}

#[test]
fn waiting_keeps_one_page() {
    let rows: Vec<Value> = (1..=250)
        .map(|seq| serde_json::json!({"team": TEAM, "seq": seq, "state": "open"}))
        .collect();
    let view = waiting(&serde_json::json!({
        "ok": true,
        "actor": {"kind": "person"},
        "rows": rows,
        "next": "t1.bmV4dA",
    }));
    assert_eq!(view.items.len(), PAGE);
    assert_eq!(view.next.as_deref(), Some("t1.bmV4dA"));
}

#[test]
fn the_pinned_waiting_reply_projects_the_open_owner_card() {
    let view = waiting(&fixture("waiting"));
    assert_eq!(view.next, None);
    assert_eq!(view.items.len(), 1);
    let item = &view.items[0];
    assert_eq!(item.id, "00000000-0000-4000-8000-00000000beef:1");
    assert_eq!(item.team, TEAM);
    assert_eq!(item.card, 1);
    assert_eq!(item.kind, "note");
    assert_eq!(item.node, None);
    assert_eq!(item.text, "Tell the team the release moved to Monday.");
    assert_eq!(item.state, "open");
    assert_eq!(item.by, "local");
    assert_eq!(item.created_ms, 1_700_000_000_000);
}

/// One hand-written queue row in the shape package 9 will publish with the
/// additive fields; until it regenerates `cli-queue.json` this keeps the
/// client core's parsing tested against the contract.
fn queue_row(id: u64, extra: Value) -> Value {
    let mut row =
        serde_json::json!({"id": id, "state": "pending", "role": "developer", "rank": "a01"});
    if let (Some(row), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            row.insert(key.clone(), value.clone());
        }
    }
    row
}

fn status_with(members: Value) -> Value {
    serde_json::json!({
        "ok": true,
        "team": {"id": TEAM, "project": "example-project", "name": "Fixes", "status": "running", "lead": "lead", "revision": 16},
        "members": members,
        "counts": {"members": {}, "nodes": {}, "inbox": {"open": 0}, "cards": {"open": 0}}
    })
}

fn queue_with(rows: Value) -> Value {
    serde_json::json!({"ok": true, "rows": rows})
}

#[test]
fn a_stage_row_carries_its_kind_its_target_and_the_target_round() {
    let queue = queue_with(serde_json::json!([
        queue_row(
            1,
            serde_json::json!({"kind": "task", "rounds": 3, "title": "Task 1"})
        ),
        queue_row(
            7,
            serde_json::json!({"kind": "stage", "stage_of": 1, "stage_index": 0, "state": "pending",
            "stage": {"kind": "review", "target": 1, "round": 2}})
        ),
        queue_row(
            8,
            serde_json::json!({"kind": "stage", "stage_of": 1, "stage_index": 1, "state": "waiting",
            "stage": {"kind": "land", "target": 1, "round": 2}})
        ),
    ]));
    let status = status_with(serde_json::json!([
        {"id": "lead", "role": "lead", "agent": "claude", "state": "active", "model": "sonnet-5"}
    ]));
    let view = team(&status, &queue);
    let card = &view
        .groups
        .iter()
        .find(|group| group.state == "pending")
        .expect("the pending group carries the card")
        .cards[0];
    assert_eq!(card.kind, Some(RowKind::Task));
    assert_eq!(card.rounds, Some(3));
    assert_eq!(card.stages.len(), 2);
    assert_eq!(card.stages[0].kind, Some(StageKind::Review));
    assert_eq!(card.stages[0].target, Some(1));
    assert_eq!(card.stages[0].round, Some(2));
    assert_eq!(card.stages[1].kind, Some(StageKind::Land));
    assert_eq!(card.stages[1].target, Some(1));
    assert_eq!(view.members[0].model.as_deref(), Some("sonnet-5"));
}

#[test]
fn an_unknown_stage_kind_is_absent_and_never_an_error() {
    let queue = queue_with(serde_json::json!([
        queue_row(1, serde_json::json!({"title": "Task 1"})),
        queue_row(
            7,
            serde_json::json!({"kind": "stage", "stage_of": 1, "stage_index": 0,
            "stage": {"kind": "deploy", "target": 1, "round": 4}})
        ),
    ]));
    let view = team(&status_with(serde_json::json!([])), &queue);
    let stage = &view
        .groups
        .iter()
        .flat_map(|group| &group.cards)
        .next()
        .unwrap()
        .stages[0];
    assert_eq!(stage.kind, None, "an unknown kind is kept out as absent");
    assert_eq!(stage.target, Some(1), "its target and round still parse");
    assert_eq!(stage.round, Some(4));
}

#[test]
fn the_additive_fields_are_absent_without_an_error_in_an_older_reply() {
    let queue = queue_with(serde_json::json!([queue_row(
        1,
        serde_json::json!({"title": "Task 1"})
    )]));
    let status = status_with(
        serde_json::json!([{"id": "lead", "role": "lead", "agent": "claude", "state": "active"}]),
    );
    let view = team(&status, &queue);
    let card = &view
        .groups
        .iter()
        .find(|group| group.state == "pending")
        .expect("the pending group carries the card")
        .cards[0];
    assert_eq!(card.kind, None);
    assert_eq!(card.rounds, None);
    assert_eq!(view.members[0].model, None);
}

#[test]
fn the_pinned_replies_still_parse_without_the_additive_fields() {
    let view = team(&fixture("status"), &fixture("queue"));
    assert!(view
        .groups
        .iter()
        .flat_map(|group| &group.cards)
        .all(|card| card.kind.is_none() && card.rounds.is_none()));
    assert!(view.members.iter().all(|member| member.model.is_none()));
}
