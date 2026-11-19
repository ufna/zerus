use super::*;
use crate::teams::ops::tests::{create, person};
use zerus_team_plan::model::{Dep, DepKind};

fn plan(h: &Harness, change: TeamAction) -> wire::Plan {
    wire::Plan {
        team: TEAM.into(),
        request_id: uuid::Uuid::new_v4().to_string(),
        expected_revision: h.store.team(TEAM).unwrap().unwrap().revision,
        action: wire::Action::Team {
            change: Box::new(change),
        },
    }
}

fn review(runtime: &mut Runtime, plan: &wire::Plan) -> wire::Review {
    stage(runtime, plan);
    let queue = Arc::new(Queue::default());
    runtime
        .query(
            &json!({"op":"team_person_action_prepare","scope":runtime.scope,"plan":plan.reference()}),
            queue.clone(),
            1,
        )
        .unwrap();
    let value = take(&queue);
    assert_eq!(value["value"]["ok"], true, "{value}");
    serde_json::from_value(value["value"]["review"].clone()).unwrap()
}

fn execute(runtime: &mut Runtime, intent: wire::Intent) -> Value {
    let queue = Arc::new(Queue::default());
    runtime
        .action(
            Validated::fixture(&json!({"op":"team_person_action","intent":intent})),
            queue.clone(),
            2,
        )
        .unwrap();
    take(&queue)["value"]["reply"].clone()
}

fn route(node: u32, role: &str, pipeline: &str) -> TeamAction {
    TeamAction::Route {
        node,
        role: role.into(),
        pipeline: pipeline.into(),
        keep_task_fields: true,
        deps: vec![],
        writes: vec![],
        assignee: None,
    }
}

#[test]
fn native_placement_route_keeps_dependencies_and_writes_and_only_clears_a_changed_role() {
    let mut h = Harness::empty();
    let mut create = create();
    let ops::Operation::Create { bundle, .. } = &mut create else {
        unreachable!()
    };
    let mut alternate = bundle.pipelines[0].clone();
    alternate.id = "alternate".into();
    bundle.pipelines.push(alternate.clone());
    alternate.id = "analysis".into();
    alternate.implementer = "reviewer".into();
    alternate.stages.clear();
    bundle.pipelines.push(alternate);
    h.run(&person(), create).unwrap();
    let (mut h, mut runtime, _, _owner) = sample_for(h);
    h.bind("m1");
    let prerequisite = h.add(&person(), 1);
    let node = h.add(&person(), 2);
    let dependency = Dep {
        node: prerequisite,
        kind: DepKind::Soft,
    };
    h.run(
        &person(),
        ops::Operation::Route {
            node,
            role: "developer".into(),
            pipeline: "reviewed".into(),
            deps: vec![dependency],
            writes: vec!["src/example.rs".into(), "docs/examples".into()],
            assignee: Some("m1".into()),
        },
    )
    .unwrap();

    let same = plan(&h, route(node, "developer", "alternate"));
    let reviewed = review(&mut runtime, &same);
    assert!(reviewed
        .details
        .iter()
        .any(|(key, text)| key.starts_with("pipeline") && text.contains("alternate")));
    let result = execute(&mut runtime, reviewed.intent);
    assert_eq!(result["ok"], true, "{result}");
    let after = h.node(node);
    assert_eq!(after.pipeline, "alternate");
    assert_eq!(after.assignee.as_deref(), Some("m1"));
    assert_eq!(after.deps, [dependency]);
    assert_eq!(after.writes, ["src/example.rs", "docs/examples"]);

    let changed = plan(&h, route(node, "reviewer", "analysis"));
    let reviewed = review(&mut runtime, &changed);
    let result = execute(&mut runtime, reviewed.intent);
    assert_eq!(result["ok"], true, "{result}");
    let after = h.node(node);
    assert_eq!(after.role, "reviewer");
    assert_eq!(after.pipeline, "analysis");
    assert_eq!(after.assignee, None);
    assert_eq!(after.deps, [dependency]);
    assert_eq!(after.writes, ["src/example.rs", "docs/examples"]);
    let recorded: String = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT content FROM requests WHERE id=?1",
            [&changed.request_id],
            |row| row.get(0),
        )
        .unwrap();
    let recorded: ops::Request = serde_json::from_str(&recorded).unwrap();
    assert_eq!(
        recorded.operation,
        ops::Operation::Route {
            node,
            role: after.role,
            pipeline: after.pipeline,
            deps: after.deps,
            writes: after.writes,
            assignee: None,
        }
    );
}

#[test]
fn native_placement_reassign_refuses_a_stale_revision_and_records_the_reason_on_success() {
    let (mut h, mut runtime, _, _owner) = sample();
    h.bind("m1");
    h.bind("m2");
    let node = h.add(&person(), 1);
    let change = TeamAction::Reassign {
        node,
        member: "m2".into(),
        reason: "The replacement has the needed context.".into(),
    };
    let first = plan(&h, change.clone());
    let reviewed = review(&mut runtime, &first);
    h.run(&person(), ops::Operation::Hold { node }).unwrap();
    let result = execute(&mut runtime, reviewed.intent);
    assert_eq!(result["code"], code::STALE_REVISION, "{result}");
    assert!(h.node(node).assignee.is_none());

    let fresh = plan(&h, change);
    let reviewed = review(&mut runtime, &fresh);
    let result = execute(&mut runtime, reviewed.intent);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(h.node(node).assignee.as_deref(), Some("m2"));
    let decisions = h.store.decisions(TEAM, 1, None).unwrap();
    assert_eq!(decisions.rows[0].kind, "reassign");
    assert_eq!(
        decisions.rows[0].text,
        "The replacement has the needed context."
    );
}
