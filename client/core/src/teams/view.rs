//! Compact view models of the Teams reads. Each is projected once in the core
//! from the checked reply and serialized as it is to the webview, which never
//! parses an `hgs` reply. Texts and lists are bounded; a view that left
//! something out says so (`truncated`, `more`).
//!
//! Adapted from the fork's `client/core/src/teams/view.rs` (WhiteWh/zerus at
//! `ca4cc4481`), reduced to the views of M1 and to the reply shapes package 5
//! pins: the team list, one team (its bar, its members and its queue as the
//! board's groups, each card with its stages) and the open owner cards.
use serde::Serialize;
use serde_json::Value;

/// The longest name, title or short field kept.
pub const NAME_CHARS: usize = 200;
/// The longest free text kept.
pub const TEXT_CHARS: usize = 8000;
/// The longest line kept (a task title, a waiting text).
pub const LINE_CHARS: usize = 600;
/// Rows one page read asks for and one view keeps: the store's page bound.
pub const PAGE: usize = 200;
/// Teams one list keeps.
pub const LIST_KEPT: usize = PAGE;
/// Members one team keeps.
pub const MEMBERS_KEPT: usize = 256;
/// Stages one card keeps.
pub const STAGES_KEPT: usize = 16;
/// The board's groups, in their order; every board carries them all so the
/// view never reshuffles its columns.
pub const GROUPS: [&str; 7] = [
    "draft",
    "waiting",
    "pending",
    "in_progress",
    "reported",
    "accepted",
    "cancelled",
];

// ---------------------------------------------------------------- helpers

fn cut(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => text[..at].to_owned(),
        None => text.to_owned(),
    }
}

fn text(value: &Value, key: &str, chars: usize) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| cut(text, chars))
        .unwrap_or_default()
}

fn name(value: &Value, key: &str) -> String {
    text(value, key, NAME_CHARS)
}

/// A string field whose absence means the type's default (`not_created` for a
/// member, `draft` for a node): the CLI leaves defaults out.
fn or_default(value: &Value, key: &str, default: &str) -> String {
    match value.get(key).and_then(Value::as_str) {
        Some(found) if !found.is_empty() => cut(found, NAME_CHARS),
        _ => default.to_owned(),
    }
}

fn count(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|found| {
            found
                .as_u64()
                .or_else(|| found.as_f64().map(|n| n.max(0.0) as u64))
        })
        .unwrap_or(0)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Whether the reply says more rows follow: the page's cursor.
fn given_more(value: &Value) -> bool {
    value.get("next").is_some_and(|next| !next.is_null())
}

/// Whether a node is a pipeline stage: it checks its target instead of being
/// a board card of its own.
fn is_stage(node: &Value) -> bool {
    name(node, "kind") == "stage" || node.get("stage_of").and_then(Value::as_u64).is_some()
}

// ---------------------------------------------------------------- the list

/// One team of `hgs team list`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamSummary {
    pub id: String,
    pub project: String,
    pub name: String,
    pub status: String,
    /// The lead's member id.
    pub lead: String,
    /// The computer the lead runs on.
    pub home: String,
    pub revision: u64,
}

impl TeamSummary {
    pub fn draft(&self) -> bool {
        self.status == "draft"
    }
}

/// The teams of this computer, as the store lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamList {
    pub teams: Vec<TeamSummary>,
    pub truncated: bool,
    /// The reply carried a next page: the view may read it on request.
    pub more: bool,
}

/// The teams of the reply's `rows`, at most [`LIST_KEPT`].
pub fn list(reply: &Value) -> TeamList {
    let rows = array(reply, "rows");
    let teams = rows
        .iter()
        .take(LIST_KEPT)
        .map(|row| TeamSummary {
            id: name(row, "id"),
            project: name(row, "project"),
            name: name(row, "name"),
            status: name(row, "status"),
            lead: name(row, "lead"),
            home: name(row, "home"),
            revision: count(row, "revision"),
        })
        .collect();
    TeamList {
        teams,
        truncated: rows.len() > LIST_KEPT,
        more: given_more(reply),
    }
}

// --------------------------------------------------------------- one team

/// A member's session: the computer UUID and the session identity
/// (`launch_id:<uuid>` or `run_id:<id>`). A name is never an identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SessionRef {
    pub machine: String,
    pub session_id: String,
}

fn session(value: &Value) -> Option<SessionRef> {
    let found = value.get("session").filter(|session| session.is_object())?;
    let view = SessionRef {
        machine: name(found, "machine"),
        session_id: name(found, "session_id"),
    };
    (!view.session_id.is_empty()).then_some(view)
}

/// One member of a team, as the Team tab's members column shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemberCard {
    pub id: String,
    pub role: String,
    pub agent: String,
    pub account: String,
    /// `not_created`, `active`, `paused`, `removed`.
    pub state: String,
    pub lead: bool,
    pub session: Option<SessionRef>,
}

/// The member counts of `team status`'s `counts.members`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemberCounts {
    pub total: u64,
    pub not_created: u64,
    pub active: u64,
    pub paused: u64,
    pub removed: u64,
}

/// The node counts of `team status`'s `counts.nodes`, by state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NodeCounts {
    pub waiting: u64,
    pub pending: u64,
    pub in_progress: u64,
    pub reported: u64,
    pub accepted: u64,
    pub cancelled: u64,
}

/// The numbers the team bar shows: `team status`'s `counts`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamCounts {
    pub members: MemberCounts,
    pub nodes: NodeCounts,
    /// The lead's open inbox items.
    pub inbox: u64,
    /// The person's open cards.
    pub cards: u64,
}

fn team_counts(value: &Value) -> TeamCounts {
    let members = &value["members"];
    let nodes = &value["nodes"];
    TeamCounts {
        members: MemberCounts {
            total: count(members, "total"),
            not_created: count(members, "not_created"),
            active: count(members, "active"),
            paused: count(members, "paused"),
            removed: count(members, "removed"),
        },
        nodes: NodeCounts {
            waiting: count(nodes, "waiting"),
            pending: count(nodes, "pending"),
            in_progress: count(nodes, "in_progress"),
            reported: count(nodes, "reported"),
            accepted: count(nodes, "accepted"),
            cancelled: count(nodes, "cancelled"),
        },
        inbox: count(&value["inbox"], "open"),
        cards: count(&value["cards"], "open"),
    }
}

/// One stage of a card: the review or test node its pipeline created.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StageCard {
    pub node: u64,
    /// The position in the pipeline.
    pub index: u64,
    pub role: String,
    /// The node state (`waiting`, `pending`, `in_progress`, `reported`,
    /// `accepted`, `cancelled`).
    pub state: String,
    /// The member the stage is handed to, as the plan chose it.
    pub member: String,
}

/// The task a card stands for: the replicated task, never the node alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TaskRefView {
    pub project: String,
    pub uuid: String,
}

fn task_ref(node: &Value) -> Option<TaskRefView> {
    let task = node.get("task").filter(|task| task.is_object())?;
    let view = TaskRefView {
        project: name(task, "project"),
        uuid: name(task, "uuid"),
    };
    (!view.uuid.is_empty()).then_some(view)
}

/// One board card: a node of the plan or the queue and, when a pipeline runs,
/// its stages. A card is never a stage node of another card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QueueCard {
    pub node: u64,
    pub title: String,
    pub state: String,
    pub task: Option<TaskRefView>,
    pub role: String,
    pub pipeline: String,
    pub assignee: String,
    pub implementer: String,
    /// The task's board rank: the queue's order.
    pub rank: String,
    pub round: u64,
    pub held: bool,
    pub admitted: bool,
    pub integrated: bool,
    pub decided_by: String,
    pub stages: Vec<StageCard>,
}

/// One group of the board: its state and its cards.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BoardGroup {
    pub state: String,
    pub cards: Vec<QueueCard>,
}

/// The team's bar: what the tab shows above the board.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamBar {
    pub id: String,
    pub project: String,
    pub name: String,
    pub status: String,
    pub goal: String,
    pub lead: String,
    pub integration_branch: String,
    pub base_commit: String,
    pub revision: u64,
    pub counts: TeamCounts,
}

/// `hgs team status` and `hgs team queue` for the Team tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamView {
    pub bar: TeamBar,
    pub members: Vec<MemberCard>,
    pub groups: Vec<BoardGroup>,
    /// Cards or stages over a bound were left out.
    pub truncated: bool,
    /// The queue gave a next page.
    pub more: bool,
    pub revision: u64,
}

fn member_cards(status: &Value) -> Vec<MemberCard> {
    let lead = name(&status["team"], "lead");
    array(status, "members")
        .iter()
        .take(MEMBERS_KEPT)
        .map(|member| MemberCard {
            id: name(member, "id"),
            role: name(member, "role"),
            agent: name(member, "agent"),
            account: name(member, "account"),
            state: or_default(member, "state", "not_created"),
            lead: member.get("id").and_then(Value::as_str) == Some(lead.as_str()),
            session: session(member),
        })
        .collect()
}

fn stage_card(node: &Value) -> StageCard {
    StageCard {
        node: count(node, "id"),
        index: count(node, "stage_index"),
        role: name(node, "role"),
        state: or_default(node, "state", "draft"),
        member: {
            let implementer = name(node, "implementer");
            if implementer.is_empty() {
                name(node, "assignee")
            } else {
                implementer
            }
        },
    }
}

fn card(node: &Value) -> QueueCard {
    QueueCard {
        node: count(node, "id"),
        title: text(node, "title", LINE_CHARS),
        state: or_default(node, "state", "draft"),
        task: task_ref(node),
        role: name(node, "role"),
        pipeline: name(node, "pipeline"),
        assignee: name(node, "assignee"),
        implementer: name(node, "implementer"),
        rank: name(node, "rank"),
        round: count(node, "round"),
        held: flag(node, "hold"),
        admitted: flag(node, "admitted"),
        integrated: flag(node, "integrated"),
        decided_by: name(node, "decided_by"),
        stages: Vec::new(),
    }
}

/// The queue as the board's groups: stage nodes folded into the card they
/// check, every group present in [`GROUPS`]' order, the rest by state name.
/// A stage whose target is outside the page stays a card of its own, so its
/// work is not lost; its target's card may come with a later page.
fn board(nodes: &[Value]) -> (Vec<BoardGroup>, bool) {
    let mut cards: Vec<QueueCard> = Vec::new();
    let mut stages: Vec<(u64, StageCard, QueueCard)> = Vec::new();
    let mut dropped = false;
    let mut cards_seen = 0usize;
    for node in nodes {
        if is_stage(node) {
            stages.push((count(node, "stage_of"), stage_card(node), card(node)));
            continue;
        }
        cards_seen += 1;
        if cards.len() >= PAGE {
            dropped = true;
            continue;
        }
        cards.push(card(node));
    }
    for (target, stage, own_card) in stages {
        let Some(card) = cards.iter_mut().find(|card| card.node == target) else {
            if cards.len() < PAGE {
                cards.push(own_card);
            } else {
                dropped = true;
            }
            continue;
        };
        if card.stages.len() < STAGES_KEPT {
            card.stages.push(stage);
        } else {
            dropped = true;
        }
    }
    let mut by_state: std::collections::BTreeMap<String, Vec<QueueCard>> = Default::default();
    for card in cards {
        by_state.entry(card.state.clone()).or_default().push(card);
    }
    for cards in by_state.values_mut() {
        cards.sort_by(|a, b| a.rank.cmp(&b.rank).then(a.node.cmp(&b.node)));
        for card in cards.iter_mut() {
            card.stages
                .sort_by(|a, b| a.index.cmp(&b.index).then(a.node.cmp(&b.node)));
        }
    }
    let mut order: Vec<String> = GROUPS.iter().map(|state| (*state).to_owned()).collect();
    order.extend(
        by_state
            .keys()
            .filter(|state| !GROUPS.contains(&state.as_str()))
            .cloned(),
    );
    let groups = order
        .into_iter()
        .map(|state| {
            let cards = by_state.remove(&state).unwrap_or_default();
            BoardGroup { state, cards }
        })
        .collect();
    (groups, dropped || cards_seen > PAGE)
}

/// The Team tab's view from a status reply (the team, its members and counts)
/// and a queue reply (one page of nodes).
pub fn team(status: &Value, queue: &Value) -> TeamView {
    let team = &status["team"];
    let nodes = array(queue, "rows");
    let (groups, truncated) = board(nodes);
    let revision = count(team, "revision");
    TeamView {
        bar: TeamBar {
            id: name(team, "id"),
            project: name(team, "project"),
            name: name(team, "name"),
            status: name(team, "status"),
            goal: text(team, "goal", TEXT_CHARS),
            lead: name(team, "lead"),
            integration_branch: name(team, "integration_branch"),
            base_commit: name(team, "base_commit"),
            revision,
            counts: team_counts(&status["counts"]),
        },
        members: member_cards(status),
        groups,
        truncated,
        more: given_more(queue),
        revision,
    }
}

// ----------------------------------------------------------------- waiting

/// One thing that waits for the person: an open owner card.
///
/// A request that only the person can answer inside a team *session* is not
/// here: the desktop's session attention data already carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingItem {
    /// Team and card sequence, for the decision that answers it.
    pub id: String,
    pub team: String,
    pub card: u64,
    pub kind: String,
    pub node: Option<u64>,
    pub text: String,
    pub state: String,
    /// The person the card is for, from the record's actor.
    pub by: String,
    pub created_ms: i64,
}

/// `hgs team waiting`: the open owner cards of every team, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingView {
    pub items: Vec<WaitingItem>,
    pub more: bool,
}

fn waiting_item(row: &Value) -> Option<WaitingItem> {
    let team = name(row, "team");
    let card = row.get("seq").and_then(Value::as_u64)?;
    Some(WaitingItem {
        id: format!("{team}:{card}"),
        team,
        card,
        kind: name(row, "kind"),
        node: row.get("node").and_then(Value::as_u64),
        text: text(row, "text", TEXT_CHARS),
        state: name(row, "state"),
        by: row
            .get("actor")
            .map(|actor| name(actor, "person"))
            .unwrap_or_default(),
        created_ms: row.get("created_ms").and_then(Value::as_i64).unwrap_or(0),
    })
}

/// The reply's cards, at most [`PAGE`].
pub fn waiting(reply: &Value) -> WaitingView {
    let rows = array(reply, "rows");
    WaitingView {
        items: rows.iter().take(PAGE).filter_map(waiting_item).collect(),
        more: given_more(reply),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pinned CLI replies of package 5, copied beside this module so the
    /// branch's tests run before package 5 merges; they are the same bytes as
    /// `crates/team-plan/fixtures/cli-*.json` at its `fa46dcc08`.
    fn fixture(name: &str) -> Value {
        let text = match name {
            "status" => include_str!("fixtures/cli-status.json"),
            "queue" => include_str!("fixtures/cli-queue.json"),
            "cards" => include_str!("fixtures/cli-cards.json"),
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
        assert!(!view.more);
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
        assert!(!view.more);
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
        assert!(view.more, "the reply's next page is visible to the view");
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
        assert!(list.more);
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
        assert!(!view.more);
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
        assert!(view.more);
    }
}
