//! Compact view models of the Teams reads. Each is projected once in the core
//! from the checked reply and serialized as it is to the webview, which never
//! parses an `hgs` reply. Texts and lists are bounded; a view that left
//! something out says so (`truncated`, `more`).
//!
//! Adapted from the fork's `client/core/src/teams/view.rs` (WhiteWh/zerus at
//! `ca4cc4481`), reduced to the views of M1: the team list, one team (its bar,
//! its members and its queue as the board's groups, each card with its stages)
//! and what waits for the person.
use serde::Serialize;
use serde_json::Value;

/// The longest name, title or short field kept.
pub const NAME_CHARS: usize = 200;
/// The longest free text kept.
pub const TEXT_CHARS: usize = 8000;
/// The longest line kept (a task title, a waiting text).
pub const LINE_CHARS: usize = 600;
/// Rows one read asks for and one view keeps: the store's own page bound.
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

/// The rows of a page reply: `nodes` (a read that gives the rows directly) or
/// `page.rows` (the store's page). Empty when the reply carries neither.
fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    if let Some(rows) = value.get(key).and_then(Value::as_array) {
        return rows;
    }
    value
        .get("page")
        .and_then(|page| page.get("rows"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Whether the reply says more rows follow: a page's cursor or a top-level
/// one.
fn given_more(value: &Value) -> bool {
    value
        .get("page")
        .and_then(|page| page.get("next"))
        .is_some_and(|next| !next.is_null())
        || value.get("next").is_some_and(|next| !next.is_null())
}

/// The team object of a reply: `team` when a read wraps it, else the reply
/// itself (a read that is about one team only).
fn team_object(value: &Value) -> &Value {
    value
        .get("team")
        .filter(|team| team.is_object())
        .unwrap_or(value)
}

/// Whether a node is a pipeline stage: it checks its target instead of being
/// a board card of its own.
fn is_stage(node: &Value) -> bool {
    if name(node, "kind") == "stage" {
        return true;
    }
    node.get("stage_of").and_then(Value::as_u64).is_some()
}

// ---------------------------------------------------------------- the list

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

/// One member of a listed team, enough to find a session's team.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ListedMember {
    pub id: String,
    pub role: String,
    pub state: String,
    pub lead: bool,
    pub session: Option<SessionRef>,
}

/// The numbers a team's bar shows, by node state. A stage node is not counted
/// twice: it belongs to the card it checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BoardCounts {
    pub tasks: u64,
    pub waiting: u64,
    pub pending: u64,
    pub in_progress: u64,
    pub reported: u64,
    pub accepted: u64,
    pub attention: u64,
}

fn board_counts(item: &Value, nodes: &[Value]) -> BoardCounts {
    if let Some(counts) = item.get("counts").filter(|counts| counts.is_object()) {
        return BoardCounts {
            tasks: count(counts, "tasks"),
            waiting: count(counts, "waiting"),
            pending: count(counts, "pending"),
            in_progress: count(counts, "in_progress"),
            reported: count(counts, "reported"),
            accepted: count(counts, "accepted"),
            attention: count(counts, "attention"),
        };
    }
    let mut counts = BoardCounts::default();
    for node in nodes.iter().filter(|node| !is_stage(node)) {
        counts.tasks += 1;
        match name(node, "state").as_str() {
            "waiting" => counts.waiting += 1,
            "pending" => counts.pending += 1,
            "in_progress" => counts.in_progress += 1,
            "reported" => counts.reported += 1,
            "accepted" => counts.accepted += 1,
            _ => {}
        }
    }
    counts
}

/// One team of `hgs team list`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub project: String,
    pub status: String,
    /// The lead's member id.
    pub lead: String,
    pub revision: u64,
    pub members: Vec<ListedMember>,
    pub counts: BoardCounts,
}

impl TeamSummary {
    pub fn draft(&self) -> bool {
        self.status == "draft"
    }
}

/// The teams of `hgs team list`, as the store lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamList {
    pub teams: Vec<TeamSummary>,
    pub truncated: bool,
    /// The reply carried a next page: the view may read it on request.
    pub more: bool,
}

fn listed_members(members: &[Value], lead: &str) -> Vec<ListedMember> {
    members
        .iter()
        .take(MEMBERS_KEPT)
        .map(|member| ListedMember {
            id: name(member, "id"),
            role: name(member, "role"),
            state: name(member, "state"),
            lead: member.get("id").and_then(Value::as_str) == Some(lead),
            session: session(member),
        })
        .collect()
}

fn summary(item: &Value, members: &[Value], nodes: &[Value]) -> TeamSummary {
    let lead = name(item, "lead");
    TeamSummary {
        id: name(item, "id"),
        name: name(item, "name"),
        project: name(item, "project"),
        status: name(item, "status"),
        members: listed_members(members, &lead),
        lead,
        revision: count(item, "revision"),
        counts: board_counts(item, nodes),
    }
}

/// The teams of the reply: its `items`, else the one team it is about.
pub fn list(reply: &Value) -> TeamList {
    let items = array(reply, "items");
    if items.is_empty() {
        let team = team_object(reply);
        if team.get("id").and_then(Value::as_str).is_none() {
            return TeamList::default();
        }
        // A reply about one team (the fixture, a status read): one summary.
        let nodes = rows(reply, "nodes");
        return TeamList {
            teams: vec![summary(team, array(reply, "members"), nodes)],
            truncated: false,
            more: false,
        };
    }
    let teams = items
        .iter()
        .take(LIST_KEPT)
        .map(|item| {
            let nodes = rows(item, "nodes");
            summary(item, array(item, "members"), nodes)
        })
        .collect();
    TeamList {
        teams,
        truncated: items.len() > LIST_KEPT,
        more: given_more(reply),
    }
}

// --------------------------------------------------------------- one team

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
    pub name: String,
    pub project: String,
    pub status: String,
    pub goal: String,
    pub lead: String,
    pub integration_branch: String,
    pub base_commit: String,
    pub revision: u64,
    pub counts: BoardCounts,
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
    let lead = name(team_object(status), "lead");
    array(status, "members")
        .iter()
        .take(MEMBERS_KEPT)
        .map(|member| MemberCard {
            id: name(member, "id"),
            role: name(member, "role"),
            agent: name(member, "agent"),
            account: name(member, "account"),
            state: name(member, "state"),
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
        state: name(node, "state"),
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
        state: name(node, "state"),
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
        let state = if card.state.is_empty() {
            "waiting".to_owned()
        } else {
            card.state.clone()
        };
        by_state.entry(state).or_default().push(card);
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

/// The Team tab's view from a status reply (the team and its members) and a
/// queue reply (the nodes, a page of at most [`PAGE`]).
pub fn team(status: &Value, queue: &Value) -> TeamView {
    let team = team_object(status);
    let nodes = rows(queue, "nodes");
    let (groups, truncated) = board(nodes);
    let revision = if status.get("revision").is_some() {
        count(status, "revision")
    } else {
        count(team, "revision")
    };
    TeamView {
        bar: TeamBar {
            id: name(team, "id"),
            name: name(team, "name"),
            project: name(team, "project"),
            status: name(team, "status"),
            goal: text(team, "goal", TEXT_CHARS),
            lead: name(team, "lead"),
            integration_branch: name(team, "integration_branch"),
            base_commit: name(team, "base_commit"),
            revision,
            counts: board_counts(status, nodes),
        },
        members: member_cards(status),
        groups,
        truncated,
        more: given_more(queue),
        revision,
    }
}

// ----------------------------------------------------------------- waiting

/// One thing that waits for the person: an owner card or a request that only
/// the person answers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingItem {
    /// Team and card sequence, for the decision that answers it.
    pub id: String,
    pub team: String,
    pub team_name: String,
    pub card: u64,
    pub kind: String,
    pub node: Option<u64>,
    pub text: String,
    pub state: String,
    /// The person the card is for, from the record's actor.
    pub by: String,
    pub created_ms: i64,
}

/// `hgs team waiting`: the person's open cards, bounded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WaitingView {
    pub items: Vec<WaitingItem>,
    pub total: u64,
    pub more: bool,
}

fn waiting_item(item: &Value) -> Option<WaitingItem> {
    let team = name(item, "team");
    let card = item
        .get("card")
        .and_then(Value::as_u64)
        .or_else(|| item.get("seq").and_then(Value::as_u64))?;
    let id = match item.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => cut(id, NAME_CHARS),
        _ => format!("{team}:{card}"),
    };
    Some(WaitingItem {
        id,
        team,
        team_name: name(item, "team_name"),
        card,
        kind: name(item, "kind"),
        node: item.get("node").and_then(Value::as_u64),
        text: text(item, "text", TEXT_CHARS),
        state: name(item, "state"),
        by: item
            .get("actor")
            .map(|actor| name(actor, "person"))
            .unwrap_or_default(),
        created_ms: item.get("created_ms").and_then(Value::as_i64).unwrap_or(0),
    })
}

/// The reply's open cards, at most [`PAGE`].
pub fn waiting(reply: &Value) -> WaitingView {
    let all = array(reply, "items");
    let items: Vec<WaitingItem> = all
        .iter()
        .filter(|item| name(item, "state") == "open")
        .filter_map(waiting_item)
        .collect();
    WaitingView {
        total: if reply.get("total").is_some() {
            count(reply, "total")
        } else {
            items.len() as u64
        },
        more: items.len() > PAGE || given_more(reply),
        items: items.into_iter().take(PAGE).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared fixture: a running queue team of a lead, two developers and
    /// a reviewer, with six nodes in different states.
    fn fixture() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/team-plan/fixtures/team-queue.json"
        )))
        .expect("the shared team fixture parses")
    }

    fn group<'a>(view: &'a TeamView, state: &str) -> &'a BoardGroup {
        view.groups
            .iter()
            .find(|group| group.state == state)
            .expect("the board carries every group")
    }

    #[test]
    fn the_fixture_lists_one_team() {
        let list = list(&fixture());
        assert!(!list.truncated);
        assert!(!list.more);
        let team = &list.teams[0];
        assert_eq!(team.id, "00000000-0000-4000-8000-00000000beef");
        assert_eq!(team.name, "Fixes");
        assert_eq!(team.project, "example-project");
        assert_eq!(team.status, "running");
        assert_eq!(team.lead, "lead");
        assert_eq!(team.revision, 42);
        assert_eq!(
            team.members
                .iter()
                .map(|member| member.id.as_str())
                .collect::<Vec<_>>(),
            ["lead", "m1", "m2", "m3"]
        );
        assert!(team.members[0].lead, "the lead is marked");
        assert!(!team.members[1].lead);
        assert_eq!(
            team.members[2].session,
            Some(SessionRef {
                machine: "00000000-0000-4000-8000-0000000000aa".into(),
                session_id: "launch_id:00000000-0000-4000-8000-000000000032".into(),
            })
        );
        // Stage node 5 belongs to card 2, so the counts are the cards'.
        assert_eq!(
            team.counts,
            BoardCounts {
                tasks: 5,
                waiting: 1,
                pending: 1,
                in_progress: 1,
                reported: 1,
                accepted: 1,
                attention: 0,
            }
        );
    }

    #[test]
    fn the_fixture_projects_the_team() {
        let view = team(&fixture(), &fixture());
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
        assert_eq!(view.bar.revision, 42);
        assert_eq!(view.revision, 42);
        assert!(!view.truncated);
        assert!(!view.more);
        assert_eq!(view.members.len(), 4);
        assert_eq!(view.members[0].role, "lead");
        assert_eq!(view.members[1].role, "developer");
        assert_eq!(
            view.members[1]
                .session
                .as_ref()
                .map(|s| s.session_id.as_str()),
            Some("launch_id:00000000-0000-4000-8000-000000000031")
        );

        // Every group, in their order.
        assert_eq!(
            view.groups
                .iter()
                .map(|group| group.state.as_str())
                .collect::<Vec<_>>(),
            GROUPS
        );

        let accepted = group(&view, "accepted");
        assert_eq!(accepted.cards.len(), 1);
        assert_eq!(accepted.cards[0].node, 1);
        assert_eq!(accepted.cards[0].title, "Fix the empty-state crash");
        assert_eq!(
            accepted.cards[0].task,
            Some(TaskRefView {
                project: "example-project".into(),
                uuid: "00000000-0000-4000-8000-000000000001".into(),
            })
        );
        assert!(accepted.cards[0].integrated);
        assert_eq!(accepted.cards[0].decided_by, "lead");

        // Card 2 reported, and its reviewer stage is pending on m1.
        let reported = group(&view, "reported");
        assert_eq!(reported.cards.len(), 1);
        let card = &reported.cards[0];
        assert_eq!(card.node, 2);
        assert_eq!(card.implementer, "m1");
        assert_eq!(
            card.stages,
            [StageCard {
                node: 5,
                index: 0,
                role: "reviewer".into(),
                state: "pending".into(),
                member: "m1".into(),
            }]
        );
        assert_eq!(group(&view, "in_progress").cards[0].node, 3);
        assert_eq!(group(&view, "pending").cards[0].node, 4);
        assert_eq!(group(&view, "waiting").cards[0].node, 6);
        assert!(!group(&view, "waiting").cards[0].admitted);
        assert!(group(&view, "draft").cards.is_empty());
        assert!(
            view.groups
                .iter()
                .flat_map(|group| &group.cards)
                .all(|card| card.node != 5),
            "a stage is not a card of its own"
        );
    }

    #[test]
    fn a_card_shows_a_stage_in_each_state() {
        let mut nodes = vec![serde_json::json!({
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
            nodes.push(serde_json::json!({
                "id": at as u64 + 2, "state": state, "kind": "stage",
                "stage_of": 1, "stage_index": at as u64, "role": "reviewer",
            }));
        }
        let queue = serde_json::json!({"nodes": nodes});
        let view = team(&serde_json::json!({"team": {"id": "t"}}), &queue);
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
        let view = team(
            &serde_json::json!({"team": {"id": "t", "name": "Empty"}}),
            &serde_json::json!({"nodes": []}),
        );
        assert_eq!(view.groups.len(), GROUPS.len());
        assert!(view.groups.iter().all(|group| group.cards.is_empty()));
        assert!(!view.truncated);
        assert!(!view.more);
        let list = list(&serde_json::json!({"team": {"id": "t"}, "members": [], "nodes": []}));
        assert_eq!(list.teams.len(), 1);
        assert_eq!(list.teams[0].counts.tasks, 0);
    }

    #[test]
    fn the_board_keeps_one_page_and_says_so() {
        let nodes: Vec<Value> = (1..=250)
            .map(|id| {
                serde_json::json!({
                    "id": id, "state": "pending", "title": format!("Task {id}"),
                    "rank": format!("a{id:03}"),
                })
            })
            .collect();
        let queue = serde_json::json!({"page": {"rows": nodes, "next": {"after": 200}}});
        let view = team(&serde_json::json!({"team": {"id": "t"}}), &queue);
        let cards = view.groups.iter().flat_map(|group| &group.cards).count();
        assert_eq!(cards, PAGE, "the view keeps one page");
        assert!(view.truncated);
        assert!(view.more, "the reply's next page is visible to the view");
    }

    #[test]
    fn the_list_reads_the_counts_the_reply_gives() {
        let list = list(&serde_json::json!({
            "items": [{
                "id": "t", "name": "T", "status": "running", "lead": "lead",
                "counts": {"tasks": 9, "pending": 3, "accepted": 6, "attention": 2},
                "members": [{"id": "lead", "role": "lead", "state": "active"}],
                "nodes": [{"id": 1, "state": "pending"}],
            }],
            "next": {"after": 1},
        }));
        assert_eq!(
            list.teams[0].counts,
            BoardCounts {
                tasks: 9,
                pending: 3,
                accepted: 6,
                attention: 2,
                ..BoardCounts::default()
            }
        );
        assert!(list.teams[0].members[0].lead);
        assert!(list.more);
        assert!(!list.truncated);
    }

    #[test]
    fn long_texts_are_cut() {
        let long = "x".repeat(TEXT_CHARS + 100);
        let view = team(
            &serde_json::json!({"team": {"id": "t", "goal": long}}),
            &serde_json::json!({"nodes": [{"id": 1, "title": long, "state": "pending"}]}),
        );
        assert_eq!(view.bar.goal.chars().count(), TEXT_CHARS);
        assert_eq!(
            view.groups
                .iter()
                .flat_map(|group| &group.cards)
                .next()
                .unwrap()
                .title
                .chars()
                .count(),
            LINE_CHARS
        );
    }

    #[test]
    fn a_stage_without_its_card_stays_a_card() {
        let queue = serde_json::json!({"nodes": [
            {"id": 7, "state": "reported", "kind": "stage", "stage_of": 99,
             "stage_index": 0, "role": "reviewer", "title": "Review"},
        ]});
        let view = team(&serde_json::json!({"team": {"id": "t"}}), &queue);
        let card = &group(&view, "reported").cards[0];
        assert_eq!(card.node, 7);
        assert_eq!(card.title, "Review");
        assert!(
            card.stages.is_empty(),
            "the stage checks a card this page does not carry"
        );
    }

    #[test]
    fn waiting_lists_open_cards_only() {
        let view = waiting(&serde_json::json!({
            "items": [
                {"team": "t1", "team_name": "One", "seq": 3, "kind": "unknown_launch",
                 "text": "A member did not start", "state": "open", "created_ms": 12,
                 "actor": {"kind": "service", "person": "local"}},
                {"team": "t1", "seq": 4, "kind": "conflict", "text": "Conflict",
                 "state": "decided"},
                {"team": "t2", "seq": 9, "kind": "no_account_available",
                 "text": "No account", "state": "open"},
            ],
            "total": 5,
            "next": {"after": 9},
        }));
        assert_eq!(view.items.len(), 2);
        assert_eq!(view.total, 5);
        assert!(view.more);
        assert_eq!(view.items[0].id, "t1:3");
        assert_eq!(view.items[0].team_name, "One");
        assert_eq!(view.items[0].card, 3);
        assert_eq!(view.items[0].by, "local");
        assert_eq!(view.items[0].created_ms, 12);
        assert_eq!(view.items[1].id, "t2:9");
    }

    #[test]
    fn waiting_keeps_one_page() {
        let items: Vec<Value> = (1..=250)
            .map(|seq| serde_json::json!({"team": "t", "seq": seq, "state": "open"}))
            .collect();
        let view = waiting(&serde_json::json!({"items": items}));
        assert_eq!(view.items.len(), PAGE);
        assert_eq!(view.total, 250, "no total given: the open cards");
        assert!(view.more);
    }
}
