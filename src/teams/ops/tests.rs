use super::*;
use zerus_team_plan::model::TeamFixture;

const TEAM: &str = "00000000-0000-4000-8000-00000000beef";
fn fixture() -> TeamFixture {
    serde_json::from_str(include_str!(
        "../../../crates/team-plan/fixtures/team-queue.json"
    ))
    .unwrap()
}
fn person() -> Actor {
    Actor::Person {
        person: "local".into(),
        origin: PersonOrigin::DesktopUi,
    }
}
fn service() -> Actor {
    Actor::Service {
        person: "local".into(),
        team: TEAM.into(),
    }
}
fn request(rev: u64, operation: Operation) -> Request {
    Request {
        request_id: uuid::Uuid::new_v4().to_string(),
        team: TEAM.into(),
        expected_revision: rev,
        operation,
    }
}
fn create() -> Operation {
    let t = fixture().team;
    Operation::Create {
        project: t.project,
        name: t.name,
        goal: t.goal,
        home: t.home,
        bundle: Box::new(t.bundle),
        integration_branch: t.integration_branch,
        base_commit: t.base_commit,
    }
}
fn observation(revision: u64, stage: TaskStage, rank: &str) -> TaskObservation {
    TaskObservation {
        revision: revision.to_string(),
        previous_revision: if revision > 1 {
            (revision - 1).to_string()
        } else {
            String::new()
        },
        stage,
        cancelled: false,
        rank: rank.into(),
    }
}
struct Harness {
    _temp: tempfile::TempDir,
    store: Store,
}
impl Harness {
    fn empty() -> Self {
        let temp = tempfile::tempdir().unwrap();
        crate::platform::private_dir(&temp.path().join("teams")).unwrap();
        std::fs::write(temp.path().join("teams/preview"), "on").unwrap();
        let store = Store::open(temp.path()).unwrap();
        Self { _temp: temp, store }
    }
    fn new() -> Self {
        let mut h = Self::empty();
        h.run(&person(), create()).unwrap();
        h
    }
    fn run(&mut self, actor: &Actor, op: Operation) -> Result<Reply, Problem> {
        let rev = self.store.team(TEAM)?.map_or(0, |t| t.revision);
        apply(&mut self.store, actor, &request(rev, op), 1234)
    }
    fn add(&mut self, actor: &Actor, n: u32) -> u32 {
        self.run(
            actor,
            Operation::AddTask {
                task: TaskRef {
                    project: "example-project".into(),
                    uuid: format!("00000000-0000-4000-8000-{n:012x}"),
                },
                title: format!("Task {n}"),
                observation: observation(1, TaskStage::Todo, &format!("a{n:08}")),
                role: None,
                pipeline: None,
            },
        )
        .unwrap()
        .node
        .unwrap()
    }
}
#[test]
fn create_team_is_transactional_and_idempotent() {
    let mut h = Harness::empty();
    let r = request(0, create());
    let first = apply(&mut h.store, &person(), &r, 1).unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(apply(&mut h.store, &person(), &r, 2).unwrap(), first);
    assert_eq!(h.store.members(TEAM, 200, None).unwrap().rows.len(), 4);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), 1);
}
#[test]
fn duplicate_task_even_with_another_request_has_one_node() {
    let mut h = Harness::new();
    let a = h.add(&person(), 1);
    let b = h.add(&person(), 1);
    assert_eq!(a, b);
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
}
#[test]
fn request_wire_refuses_unknown_fields() {
    let mut v = serde_json::to_value(request(0, Operation::Pause)).unwrap();
    v["unexpected"] = true.into();
    assert!(serde_json::from_value::<Request>(v).is_err());
}

impl Harness {
    fn bind(&mut self, id: &str) -> Actor {
        let m = fixture().members.into_iter().find(|m| m.id == id).unwrap();
        let session = m.session.unwrap();
        self.run(
            &service(),
            Operation::SetMemberSession {
                member: id.into(),
                session: session.clone(),
                account: m.account,
                worktree: m.worktree,
                ownership_nonce: m.ownership_nonce,
            },
        )
        .unwrap();
        Actor::Session {
            person: "local".into(),
            team: TEAM.into(),
            member: id.into(),
            session,
        }
    }
    fn node(&self, id: u32) -> Node {
        self.store.node(TEAM, id).unwrap().unwrap()
    }
    fn handout(&mut self, node: u32) -> String {
        let effect_request_id = uuid::Uuid::new_v4().to_string();
        self.run(
            &service(),
            Operation::HandoutIntent {
                node,
                member: "m1".into(),
                effect_request_id: effect_request_id.clone(),
            },
        )
        .unwrap();
        effect_request_id
    }
    fn observe(&mut self, node: u32, revision: u64, stage: TaskStage) {
        let rank = self.node(node).rank;
        let mut o = observation(revision, stage, &rank);
        o.previous_revision = self
            .store
            .task_observation(TEAM, node)
            .unwrap()
            .unwrap()
            .revision;
        self.run(
            &service(),
            Operation::ObserveTask {
                node,
                observation: o,
            },
        )
        .unwrap();
    }
    fn reported(&mut self) -> (u32, Actor, Actor) {
        let lead = self.bind("lead");
        let member = self.bind("m1");
        let node = self.add(&person(), 1);
        self.handout(node);
        self.observe(node, 2, TaskStage::InProgress);
        self.run(
            &member,
            Operation::RecordReport {
                node,
                generation: 1,
                observation: observation(3, TaskStage::InReview, &self.node(node).rank),
                text: "Implemented and checked".into(),
                commit: "0123456789abcdef0123456789abcdef01234567".into(),
            },
        )
        .unwrap();
        (node, lead, member)
    }
}
fn other() -> Actor {
    Actor::Other {
        person: "local".into(),
        session: SessionKey {
            machine: "00000000-0000-4000-8000-0000000000bb".into(),
            session_id: "launch_id:00000000-0000-4000-8000-000000000011".into(),
        },
    }
}
fn code<T: std::fmt::Debug>(result: Result<T, Problem>, expected: &str) {
    assert_eq!(result.unwrap_err().code, expected);
}
#[test]
fn stale_revision_refusal_and_success_are_both_replayed() {
    let mut h = Harness::new();
    let r = request(0, Operation::Pause);
    code(apply(&mut h.store, &person(), &r, 1), code::STALE_REVISION);
    h.run(&person(), Operation::Pause).unwrap();
    code(apply(&mut h.store, &person(), &r, 2), code::STALE_REVISION);
    let rev = h.store.team(TEAM).unwrap().unwrap().revision;
    let r = request(rev, Operation::Resume);
    let first = apply(&mut h.store, &person(), &r, 1).unwrap();
    assert_eq!(apply(&mut h.store, &person(), &r, 2).unwrap(), first);
    let mut changed = r.clone();
    changed.operation = Operation::Pause;
    code(apply(&mut h.store, &person(), &changed, 2), code::DUPLICATE);
}
#[test]
fn other_agents_wait_for_admission_and_lead_routing() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.routing = Routing::Lead;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    let node = h.add(&other(), 1);
    assert_eq!(h.node(node).state, NodeState::Waiting);
    h.run(&lead, Operation::Admit { node }).unwrap();
    assert_eq!(h.node(node).state, NodeState::Waiting);
    h.run(
        &lead,
        Operation::Route {
            node,
            role: "developer".into(),
            pipeline: "reviewed".into(),
            deps: vec![],
            writes: vec!["client/src/example".into()],
        },
    )
    .unwrap();
    assert_eq!(h.node(node).state, NodeState::Pending);
    code(h.run(&lead, Operation::Admit { node }), code::INVALID_STATE);
}
#[test]
fn exact_session_binding_is_rechecked_for_every_operation() {
    let mut h = Harness::new();
    let mut lead = h.bind("lead");
    if let Actor::Session { session, .. } = &mut lead {
        session.session_id = "launch_id:00000000-0000-4000-8000-000000000099".into();
    }
    code(h.run(&lead, Operation::Pause), code::NOT_ALLOWED);
    let lead = Actor::Session {
        person: "local".into(),
        team: TEAM.into(),
        member: "lead".into(),
        session: fixture().members[0].session.clone().unwrap(),
    };
    h.run(&lead, Operation::Pause).unwrap();
    h.run(&lead, Operation::Resume).unwrap();
}
#[test]
fn uncertain_handout_blocks_replay_until_new_generation() {
    let mut h = Harness::new();
    h.bind("m1");
    let lead = h.bind("lead");
    let node = h.add(&person(), 1);
    let effect = h.handout(node);
    h.run(
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: effect.clone(),
            outcome: HandoutOutcome::Uncertain {
                reason: "No receipt after restart".into(),
            },
        },
    )
    .unwrap();
    let retry = Operation::HandoutIntent {
        node,
        member: "m1".into(),
        effect_request_id: uuid::Uuid::new_v4().to_string(),
    };
    code(h.run(&service(), retry.clone()), code::INVALID_STATE);
    assert_eq!(
        h.store.inbox(TEAM, 200, None).unwrap().rows[0].kind,
        "delivery_uncertain"
    );
    code(
        h.run(
            &service(),
            Operation::HandoutOutcome {
                effect_request_id: effect,
                outcome: HandoutOutcome::Submitted {
                    receipt: "late".into(),
                },
            },
        ),
        code::INVALID_STATE,
    );
    h.run(
        &lead,
        Operation::Reassign {
            node,
            member: "m1".into(),
            reason: "Checked the old attempt; use a new attempt".into(),
        },
    )
    .unwrap();
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(2));
    assert!(h.node(node).hold);
    h.run(&lead, Operation::Release { node }).unwrap();
    h.run(&service(), retry).unwrap();
    assert_eq!(
        h.store.effects(TEAM, 200, None).unwrap().rows[0].state,
        "superseded"
    );
}
#[test]
fn refusal_releases_member_and_reservation_survives_reopen() {
    let mut h = Harness::new();
    h.bind("m1");
    let first = h.add(&person(), 1);
    let second = h.add(&person(), 2);
    let effect = h.handout(first);
    let retry = Operation::HandoutIntent {
        node: second,
        member: "m1".into(),
        effect_request_id: uuid::Uuid::new_v4().to_string(),
    };
    let reopened = Store::open(h._temp.path()).unwrap();
    h.store = reopened;
    code(h.run(&service(), retry.clone()), code::INVALID_STATE);
    h.run(
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: effect,
            outcome: HandoutOutcome::Refused {
                reason: "Writer definitely refused delivery".into(),
            },
        },
    )
    .unwrap();
    h.run(&service(), retry).unwrap();
}
#[test]
fn submitted_and_cutting_over_outcomes_are_recorded_once() {
    for outcome in [
        HandoutOutcome::Submitted {
            receipt: "receipt-example".into(),
        },
        HandoutOutcome::CuttingOver {
            reason: "Writer is cutting over".into(),
        },
    ] {
        let mut h = Harness::new();
        h.bind("m1");
        let node = h.add(&person(), 1);
        let effect = h.handout(node);
        let op = Operation::HandoutOutcome {
            effect_request_id: effect,
            outcome,
        };
        h.run(&service(), op.clone()).unwrap();
        code(h.run(&service(), op), code::INVALID_STATE);
        assert_eq!(
            h.node(node).state,
            NodeState::Pending,
            "Delivery is not a task-state observation"
        );
    }
}
#[test]
fn acceptance_waits_for_task_writer_and_archives_only_observed_done() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    h.run(
        &lead,
        Operation::Accept {
            node,
            integration_commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
        },
    )
    .unwrap();
    assert_eq!(h.node(node).state, NodeState::Reported);
    assert_eq!(h.node(node).decided_by, Some(Decider::Lead));
    h.observe(node, 4, TaskStage::Done);
    assert_eq!(h.node(node).state, NodeState::Accepted);
    assert!(h.store.queue(TEAM, 200, None).unwrap().rows.is_empty());
    let db = h.store.db.as_ref().unwrap();
    let data: String = db
        .query_row(
            "SELECT data FROM node_archive WHERE team=?1 AND id=?2",
            rusqlite::params![TEAM, node],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!serde_json::from_str::<serde_json::Value>(&data)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("state"));
}
#[test]
fn reports_are_bound_to_the_current_attempt_and_assignee() {
    let mut h = Harness::new();
    let (node, lead, member) = h.reported();
    let report = Operation::RecordReport {
        node,
        generation: 1,
        observation: observation(3, TaskStage::InReview, &h.node(node).rank),
        text: "Report".into(),
        commit: "commit-example".into(),
    };
    code(h.run(&member, report.clone()), code::INVALID_STATE);
    h.run(
        &lead,
        Operation::Return {
            node,
            reason: "Fix the missing edge case".into(),
        },
    )
    .unwrap();
    code(h.run(&member, report), code::INVALID_STATE);
    assert_eq!(h.store.generation(TEAM, node).unwrap(), Some(2));
    assert_eq!(h.node(node).round, 2);
}
#[test]
fn ask_reply_and_question_cap_are_per_attempt() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let m = h.bind("m1");
    let node = h.add(&person(), 1);
    h.handout(node);
    h.observe(node, 2, TaskStage::InProgress);
    let mut first = 0;
    for n in 0..3 {
        let r = h
            .run(
                &m,
                Operation::Ask {
                    node,
                    generation: 1,
                    text: format!("Question {n}"),
                },
            )
            .unwrap();
        if n == 0 {
            first = r.item.unwrap();
        }
    }
    code(
        h.run(
            &m,
            Operation::Ask {
                node,
                generation: 1,
                text: "Overflow".into(),
            },
        ),
        code::OVER_CAP,
    );
    h.run(
        &lead,
        Operation::Reply {
            item: first,
            text: "Use the documented behavior".into(),
        },
    )
    .unwrap();
    h.run(
        &m,
        Operation::Ask {
            node,
            generation: 1,
            text: "Replacement question".into(),
        },
    )
    .unwrap();
    code(
        h.run(
            &lead,
            Operation::Reply {
                item: first,
                text: "Repeated".into(),
            },
        ),
        code::INVALID_STATE,
    );
    let answers = h.store.inbox(TEAM, 200, None).unwrap();
    assert_eq!(
        answers.rows[0].reply_actor.as_ref().unwrap()["person"],
        "local"
    );
}
#[test]
fn owner_cards_record_proposer_and_person_decision() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let card = h
        .run(
            &lead,
            Operation::OpenCard {
                kind: "proposal".into(),
                text: "Add another member?".into(),
                node: None,
            },
        )
        .unwrap()
        .item
        .unwrap();
    code(
        h.run(
            &lead,
            Operation::DecideCard {
                card,
                decision: "Yes".into(),
            },
        ),
        code::NOT_ALLOWED,
    );
    h.run(
        &person(),
        Operation::DecideCard {
            card,
            decision: "Keep the approved roster".into(),
        },
    )
    .unwrap();
    code(
        h.run(
            &person(),
            Operation::DecideCard {
                card,
                decision: "Again".into(),
            },
        ),
        code::INVALID_STATE,
    );
    let card = &h.store.cards(TEAM, 200, None).unwrap().rows[0];
    assert_eq!(card.actor["person"], "local");
    assert_eq!(card.decided_actor.as_ref().unwrap()["person"], "local");
}
#[test]
fn lifecycle_and_person_stopped_member_are_guarded() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.run(
        &lead,
        Operation::PauseMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    h.run(
        &lead,
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    h.run(
        &person(),
        Operation::StopMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    code(
        h.run(
            &lead,
            Operation::ResumeMember {
                member: "m1".into(),
            },
        ),
        code::INVALID_STATE,
    );
    h.run(
        &person(),
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    code(
        h.run(
            &person(),
            Operation::ResumeMember {
                member: "m1".into(),
            },
        ),
        code::INVALID_STATE,
    );
    let node = h.add(&person(), 1);
    code(h.run(&person(), Operation::Archive), code::INVALID_STATE);
    h.run(&lead, Operation::Pause).unwrap();
    code(h.run(&lead, Operation::Pause), code::INVALID_STATE);
    h.run(&lead, Operation::Resume).unwrap();
    code(h.run(&lead, Operation::Stop), code::NOT_ALLOWED);
    h.run(&person(), Operation::Stop).unwrap();
    assert!(h.node(node).hold);
    assert_eq!(h.node(node).state, NodeState::Pending);
    h.run(&person(), Operation::Archive).unwrap();
    assert_eq!(
        h.store.team(TEAM).unwrap().unwrap().status,
        TeamStatus::Archived
    );
    assert_eq!(
        h.store
            .journal(TEAM, 200, None)
            .unwrap()
            .rows
            .last()
            .unwrap()
            .kind,
        "archive"
    );
}
#[test]
fn delegation_revocation_blocks_lead_accept_and_reply() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.delegations.accept_results = false;
    settings.delegations.answer_members = false;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    code(
        h.run(
            &lead,
            Operation::Accept {
                node,
                integration_commit: Some("commit-example".into()),
            },
        ),
        code::NOT_ALLOWED,
    );
    code(
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
fn node_state_tracks_observations_without_accepting_stale_snapshots() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.observe(node, 4, TaskStage::InProgress);
    code(
        h.run(
            &service(),
            Operation::ObserveTask {
                node,
                observation: observation(3, TaskStage::Todo, &h.node(node).rank),
            },
        ),
        code::STALE_REVISION,
    );
    assert_eq!(h.node(node).state, NodeState::InProgress);
}
#[test]
fn failed_operation_rolls_back_revision_and_journal() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    let before = h.store.team(TEAM).unwrap().unwrap();
    let journal = h.store.journal(TEAM, 200, None).unwrap();
    code(
        h.run(
            &person(),
            Operation::Route {
                node,
                role: "missing".into(),
                pipeline: "reviewed".into(),
                deps: vec![],
                writes: vec![],
            },
        ),
        code::UNKNOWN_ROLE,
    );
    assert_eq!(h.store.team(TEAM).unwrap().unwrap(), before);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap(), journal);
}
#[test]
fn each_operation_has_an_authority_refusal() {
    let mut h = Harness::new();
    let team = h.store.team(TEAM).unwrap().unwrap();
    let session = fixture().members[1].session.clone().unwrap();
    let dummy = vec![
        create(),
        Operation::SetMemberSession {
            member: "m1".into(),
            session,
            account: "example".into(),
            worktree: "/workspace/example".into(),
            ownership_nonce: "example".into(),
        },
        Operation::SetMemberState {
            member: "m1".into(),
            state: MemberState::Active,
        },
        Operation::Pause,
        Operation::Resume,
        Operation::Stop,
        Operation::Archive,
        Operation::PauseMember {
            member: "m1".into(),
        },
        Operation::ResumeMember {
            member: "m1".into(),
        },
        Operation::StopMember {
            member: "m1".into(),
        },
        Operation::ArchiveMember {
            member: "m1".into(),
        },
        Operation::AddTask {
            task: TaskRef {
                project: "example-project".into(),
                uuid: "00000000-0000-4000-8000-000000000001".into(),
            },
            title: "Example".into(),
            observation: observation(1, TaskStage::Todo, "a"),
            role: None,
            pipeline: None,
        },
        Operation::Admit { node: 1 },
        Operation::Route {
            node: 1,
            role: "developer".into(),
            pipeline: "reviewed".into(),
            deps: vec![],
            writes: vec![],
        },
        Operation::Hold { node: 1 },
        Operation::Release { node: 1 },
        Operation::Cancel {
            node: 1,
            reason: "Example".into(),
        },
        Operation::HandoutIntent {
            node: 1,
            member: "m1".into(),
            effect_request_id: uuid::Uuid::new_v4().to_string(),
        },
        Operation::HandoutOutcome {
            effect_request_id: uuid::Uuid::new_v4().to_string(),
            outcome: HandoutOutcome::Uncertain {
                reason: "Example".into(),
            },
        },
        Operation::ObserveTask {
            node: 1,
            observation: observation(2, TaskStage::Done, "a"),
        },
        Operation::RecordReport {
            node: 1,
            generation: 1,
            observation: observation(2, TaskStage::InReview, "a"),
            text: "Example".into(),
            commit: "commit-example".into(),
        },
        Operation::Accept {
            node: 1,
            integration_commit: Some("commit-example".into()),
        },
        Operation::Return {
            node: 1,
            reason: "Example".into(),
        },
        Operation::Reassign {
            node: 1,
            member: "m1".into(),
            reason: "Example".into(),
        },
        Operation::Ask {
            node: 1,
            generation: 1,
            text: "Example".into(),
        },
        Operation::Reply {
            item: 1,
            text: "Example".into(),
        },
        Operation::OpenCard {
            kind: "example".into(),
            text: "Example".into(),
            node: None,
        },
        Operation::DecideCard {
            card: 1,
            decision: "Example".into(),
        },
        Operation::ChangeSettings {
            settings: team.settings.clone(),
        },
    ];
    let wrong_person = Actor::Person {
        person: "someone-else".into(),
        origin: PersonOrigin::DesktopUi,
    };
    for op in dummy {
        code(authority(&wrong_person, &op, &team), code::NOT_ALLOWED);
        if !matches!(op, Operation::AddTask { .. }) {
            code(authority(&other(), &op, &team), code::NOT_ALLOWED);
        }
    }
    code(h.run(&other(), Operation::Pause), code::NOT_ALLOWED);
}

#[test]
fn queue_and_journal_pages_exclude_rows_added_after_first_page() {
    let mut h = Harness::new();
    for n in 1..=5 {
        h.add(&person(), n);
    }
    let first = h.store.queue(TEAM, 2, None).unwrap();
    assert_eq!(
        first.rows.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    let journal = h.store.journal(TEAM, 2, None).unwrap();
    let old_journal_count = h.store.journal(TEAM, 200, None).unwrap().rows.len();
    h.add(&person(), 6);
    let mut ids = first.rows.iter().map(|n| n.id).collect::<Vec<_>>();
    let mut cursor = first.next;
    while let Some(c) = cursor {
        let page = h.store.queue(TEAM, 2, Some(&c)).unwrap();
        ids.extend(page.rows.iter().map(|n| n.id));
        cursor = page.next;
    }
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    let mut count = journal.rows.len();
    let mut cursor = journal.next;
    while let Some(c) = cursor {
        let page = h.store.journal(TEAM, 2, Some(&c)).unwrap();
        count += page.rows.len();
        cursor = page.next;
    }
    assert_eq!(count, old_journal_count);
    code(h.store.queue(TEAM, 201, None), code::OVER_CAP);
    code(h.store.members(TEAM, 0, None), code::OVER_CAP);
    let cursor = h.store.queue(TEAM, 1, None).unwrap().next.unwrap();
    code(h.store.cards(TEAM, 2, Some(&cursor)), code::INVALID_STATE);
}
#[test]
fn one_thousand_decided_nodes_leave_live_queue_unchanged() {
    let mut h = Harness::new();
    let keep = h.add(&person(), 1);
    let original = h.store.queue(TEAM, 200, None).unwrap().rows;
    for n in 2..=1001 {
        let id = h.add(&person(), n);
        let mut snapshot = observation(2, TaskStage::Todo, &h.node(id).rank);
        snapshot.cancelled = true;
        h.run(
            &service(),
            Operation::ObserveTask {
                node: id,
                observation: snapshot,
            },
        )
        .unwrap();
    }
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows, original);
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows[0].id, keep);
    let archived: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM node_archive WHERE team=?1",
            [TEAM],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived, 1000);
    h.add(&person(), 1002);
}
#[test]
fn newer_schema_is_refused_without_changing_version() {
    let h = Harness::new();
    h.store
        .db
        .as_ref()
        .unwrap()
        .pragma_update(None, "user_version", 2)
        .unwrap();
    code(Store::open(h._temp.path()), code::INVALID_STATE);
    let version: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
}
#[test]
fn hold_release_cancel_and_member_state_have_success_and_refusal() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.run(
        &service(),
        Operation::SetMemberState {
            member: "m1".into(),
            state: MemberState::Paused,
        },
    )
    .unwrap();
    code(
        h.run(
            &service(),
            Operation::SetMemberState {
                member: "m1".into(),
                state: MemberState::Paused,
            },
        ),
        code::INVALID_STATE,
    );
    h.run(
        &service(),
        Operation::SetMemberState {
            member: "m1".into(),
            state: MemberState::Active,
        },
    )
    .unwrap();
    let node = h.add(&person(), 1);
    code(
        h.run(&lead, Operation::Release { node }),
        code::INVALID_STATE,
    );
    h.run(&lead, Operation::Hold { node }).unwrap();
    code(h.run(&lead, Operation::Hold { node }), code::INVALID_STATE);
    h.run(&lead, Operation::Release { node }).unwrap();
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    )
    .unwrap();
    assert_eq!(h.node(node).state, NodeState::Pending);
    assert!(h.node(node).hold);
    let mut snapshot = observation(2, TaskStage::Todo, &h.node(node).rank);
    snapshot.cancelled = true;
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: snapshot,
        },
    )
    .unwrap();
    code(
        h.run(
            &lead,
            Operation::Cancel {
                node,
                reason: "Again".into(),
            },
        ),
        code::INVALID_STATE,
    );
    assert_eq!(h.node(node).state, NodeState::Cancelled);
}
#[test]
fn first_unauthorized_create_never_creates_a_database() {
    let mut h = Harness::empty();
    code(h.run(&other(), create()), code::NOT_ALLOWED);
    assert!(!h._temp.path().join("teams/teams-v1.sqlite3").exists());
}
#[test]
fn same_task_request_returns_the_first_reply_without_second_journal_entry() {
    let mut h = Harness::new();
    let r = request(
        1,
        Operation::AddTask {
            task: TaskRef {
                project: "example-project".into(),
                uuid: "00000000-0000-4000-8000-000000000001".into(),
            },
            title: "Example".into(),
            observation: observation(1, TaskStage::Todo, "a"),
            role: None,
            pipeline: None,
        },
    );
    let first = apply(&mut h.store, &person(), &r, 1).unwrap();
    assert_eq!(apply(&mut h.store, &person(), &r, 2).unwrap(), first);
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), 2);
}
#[test]
fn journal_is_capped_and_archived_entries_keep_paging_order() {
    let mut h = Harness::new();
    let db = h.store.db.as_mut().unwrap();
    let tx = db.transaction().unwrap();
    for n in 0..=store::LIVE_JOURNAL_CAP {
        tx.execute("INSERT INTO journal(team,at_ms,actor,kind,request_id,text) VALUES(?1,0,?2,'fixture',?3,'Example')",rusqlite::params![TEAM,store::encode(&person()).unwrap(),format!("fixture-{n}")]).unwrap();
    }
    tx.commit().unwrap();
    h.run(&person(), Operation::Pause).unwrap();
    let live: u64 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM journal WHERE team=?1", [TEAM], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(live, store::LIVE_JOURNAL_CAP);
    let page = h.store.journal(TEAM, 3, None).unwrap();
    assert_eq!(
        page.rows.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}
#[test]
fn disabling_preview_revokes_an_already_open_handle() {
    let mut h = Harness::new();
    std::fs::remove_file(h._temp.path().join("teams/preview")).unwrap();
    code(h.store.team(TEAM), code::TEAMS_INACTIVE);
    code(h.run(&person(), Operation::Pause), code::TEAMS_INACTIVE);
}
#[test]
fn person_task_override_restores_archived_node_and_preserves_its_identity() {
    let mut h = Harness::new();
    let id = h.add(&person(), 1);
    h.observe(id, 2, TaskStage::Done);
    assert!(h.store.queue(TEAM, 200, None).unwrap().rows.is_empty());
    h.observe(id, 3, TaskStage::Todo);
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows[0].id, id);
    assert_eq!(h.add(&person(), 1), id);
}

#[test]
fn accept_without_integration_has_no_commit_receipt() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.integration = Integration::None;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    h.run(
        &lead,
        Operation::Accept {
            node,
            integration_commit: None,
        },
    )
    .unwrap();
    assert!(!h.node(node).integrated);
}
#[test]
fn accept_with_merge_delegation_off_has_no_commit_receipt() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.delegations.merge_accepted = false;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    h.run(
        &lead,
        Operation::Accept {
            node,
            integration_commit: None,
        },
    )
    .unwrap();
    assert!(!h.node(node).integrated);
}
#[test]
fn accepting_stage_has_no_integration_receipt() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    let mut stage = h.node(node);
    stage.kind = NodeKind::Stage;
    stage.stage_of = Some(2);
    let payload = rows::node_payload(&stage).unwrap();
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE nodes SET data=?3 WHERE team=?1 AND id=?2",
            rusqlite::params![TEAM, node, payload],
        )
        .unwrap();
    h.run(
        &lead,
        Operation::Accept {
            node,
            integration_commit: None,
        },
    )
    .unwrap();
    assert!(!h.node(node).integrated);
}
#[test]
fn accepting_implementation_requires_receipt_when_merging_is_enabled() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    code(
        h.run(
            &lead,
            Operation::Accept {
                node,
                integration_commit: None,
            },
        ),
        code::INVALID_STATE,
    );
}

#[test]
fn decided_nodes_are_available_through_bounded_state_pages() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.observe(node, 2, TaskStage::Done);
    assert_eq!(
        h.store
            .nodes_by_state(TEAM, NodeState::Accepted, 1, None)
            .unwrap()
            .rows[0]
            .id,
        node
    );
}
#[test]
fn cap_counts_only_live_nodes_and_refuses_without_partial_insertion() {
    let mut h = Harness::new();
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.caps.max_open_tasks = 1;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    let first = h.add(&person(), 1);
    let r = request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::AddTask {
            task: TaskRef {
                project: "example-project".into(),
                uuid: "00000000-0000-4000-8000-000000000002".into(),
            },
            title: "Example".into(),
            observation: observation(1, TaskStage::Todo, "b"),
            role: None,
            pipeline: None,
        },
    );
    code(apply(&mut h.store, &person(), &r, 1), code::OVER_CAP);
    h.observe(first, 2, TaskStage::Done);
    assert_eq!(h.add(&person(), 2), 2);
}
#[test]
fn each_operation_refuses_invalid_state_or_binding() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let member = h.bind("m1");
    let node = h.add(&person(), 1);
    let bad = vec![
        Operation::Resume,
        Operation::Archive,
        Operation::ResumeMember {
            member: "m1".into(),
        },
        Operation::PauseMember {
            member: "m2".into(),
        },
        Operation::StopMember {
            member: "m2".into(),
        },
        Operation::SetMemberSession {
            member: "m1".into(),
            session: fixture().members[1].session.clone().unwrap(),
            account: "example".into(),
            worktree: "/workspace/example".into(),
            ownership_nonce: "example".into(),
        },
        Operation::Accept {
            node,
            integration_commit: Some("commit-example".into()),
        },
        Operation::Return {
            node,
            reason: "Example".into(),
        },
        Operation::Release { node },
        Operation::Admit { node },
        Operation::HandoutOutcome {
            effect_request_id: uuid::Uuid::new_v4().to_string(),
            outcome: HandoutOutcome::Refused {
                reason: "Example".into(),
            },
        },
        Operation::RecordReport {
            node,
            generation: 1,
            observation: observation(2, TaskStage::InReview, &h.node(node).rank),
            text: "Example".into(),
            commit: "example".into(),
        },
        Operation::Ask {
            node,
            generation: 1,
            text: "Example".into(),
        },
        Operation::Reply {
            item: 1000,
            text: "Example".into(),
        },
        Operation::DecideCard {
            card: 1000,
            decision: "Example".into(),
        },
    ];
    for op in bad {
        let r = h.run(&person(), op.clone());
        assert!(r.is_err(), "{op:?} unexpectedly succeeded");
    }
    let rev = h.store.team(TEAM).unwrap().unwrap().revision;
    code(
        apply(&mut h.store, &person(), &request(rev, create()), 1),
        code::INVALID_STATE,
    );
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.phase_acceptance = Acceptance::Pipeline;
    code(
        h.run(&person(), Operation::ChangeSettings { settings }),
        code::INVALID_STATE,
    );
    h.run(&lead, Operation::Pause).unwrap();
    code(
        h.run(
            &service(),
            Operation::HandoutIntent {
                node,
                member: "m1".into(),
                effect_request_id: uuid::Uuid::new_v4().to_string(),
            },
        ),
        code::INVALID_STATE,
    );
    h.run(&lead, Operation::Resume).unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    code(
        h.run(
            &lead,
            Operation::Reassign {
                node,
                member: "m1".into(),
                reason: "Example".into(),
            },
        ),
        code::INVALID_STATE,
    );
    code(
        h.run(
            &member,
            Operation::Ask {
                node,
                generation: 1,
                text: "Example".into(),
            },
        ),
        code::NOT_ALLOWED,
    );
    h.run(&person(), Operation::Stop).unwrap();
    code(h.run(&person(), Operation::Stop), code::INVALID_STATE);
    code(
        h.run(
            &person(),
            Operation::OpenCard {
                kind: "proposal".into(),
                text: "Example".into(),
                node: None,
            },
        ),
        code::INVALID_STATE,
    );
}
#[cfg(unix)]
#[test]
fn private_store_refuses_links_and_keeps_other_connection_locks() {
    use std::os::unix::fs::symlink;
    let h = Harness::new();
    let path = h._temp.path().join("teams/teams-v1.sqlite3");
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE;")
        .unwrap();
    assert!(crate::sqlite_file::foreign_lock_refused(&path));
    let reopened = Store::open(h._temp.path()).unwrap();
    assert!(crate::sqlite_file::foreign_lock_refused(&path));
    drop(reopened);
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("ROLLBACK;")
        .unwrap();
    drop(h.store);
    let moved = h._temp.path().join("teams/saved.sqlite3");
    std::fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    code(Store::open(h._temp.path()), code::INVALID_STATE);
}

#[test]
fn opaque_task_tokens_use_projection_cas_instead_of_sort_order() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    let mut o = observation(2, TaskStage::InProgress, &h.node(node).rank);
    o.revision = "ffffffff".into();
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o,
        },
    )
    .unwrap();
    let o = TaskObservation {
        revision: "00000000".into(),
        previous_revision: "ffffffff".into(),
        stage: TaskStage::InReview,
        cancelled: false,
        rank: h.node(node).rank,
    };
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o,
        },
    )
    .unwrap();
    assert_eq!(h.node(node).state, NodeState::Reported);
}
#[test]
fn an_empty_store_handle_can_refresh_after_another_writer_creates_the_first_team() {
    let mut h = Harness::empty();
    let mut writer = Store::open(h._temp.path()).unwrap();
    apply(&mut writer, &person(), &request(0, create()), 1).unwrap();
    h.store.refresh().unwrap();
    assert!(h.store.team(TEAM).unwrap().is_some());
}
#[test]
fn reopening_archived_tasks_cannot_exceed_the_live_set_cap() {
    let mut h = Harness::new();
    let old = h.add(&person(), 1);
    h.observe(old, 2, TaskStage::Done);
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.caps.max_open_tasks = 1;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    h.add(&person(), 2);
    code(
        h.run(
            &service(),
            Operation::ObserveTask {
                node: old,
                observation: observation(3, TaskStage::Todo, &h.node(old).rank),
            },
        ),
        code::OVER_CAP,
    );
    assert_eq!(h.node(old).state, NodeState::Accepted);
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap().rows.len(), 1);
}
#[test]
fn journal_references_identify_the_changed_node() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.run(&person(), Operation::Hold { node }).unwrap();
    let journal = h.store.journal(TEAM, 200, None).unwrap();
    assert!(journal
        .rows
        .last()
        .unwrap()
        .refs
        .contains(&format!("node:{node}")));
}

#[test]
fn late_task_observations_keep_an_archived_teams_journal_archived() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.run(&person(), Operation::Stop).unwrap();
    h.run(&person(), Operation::Archive).unwrap();
    let mut o = observation(2, TaskStage::Todo, &h.node(node).rank);
    o.cancelled = true;
    h.run(
        &service(),
        Operation::ObserveTask {
            node,
            observation: o,
        },
    )
    .unwrap();
    let count: u64 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM journal WHERE team=?1", [TEAM], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        h.store
            .journal(TEAM, 200, None)
            .unwrap()
            .rows
            .last()
            .unwrap()
            .kind,
        "observe_task"
    );
}

#[test]
fn exact_receipt_replay_stays_read_only_after_delegation_is_revoked() {
    let mut h = Harness::new();
    let (node, lead, _) = h.reported();
    let rev = h.store.team(TEAM).unwrap().unwrap().revision;
    let r = request(
        rev,
        Operation::Accept {
            node,
            integration_commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
        },
    );
    let first = apply(&mut h.store, &lead, &r, 1).unwrap();
    let mut settings = h.store.team(TEAM).unwrap().unwrap().settings;
    settings.delegations.accept_results = false;
    h.run(&person(), Operation::ChangeSettings { settings })
        .unwrap();
    assert_eq!(apply(&mut h.store, &lead, &r, 2).unwrap(), first);
    code(h.run(&lead, r.operation), code::NOT_ALLOWED);
}
