use super::*;
use zerus_client_core::{task_details, tasks as desktop_tasks, Change, Coalescer, ComputerId};
use zerus_replica::{
    execution::{NativeRequest, Origin, Receipt, ReportKind},
    ReviewAssignment, ReviewOwner, Settlement, TaskTeam,
};

const ONE: &str = "11111111-1111-4111-8111-111111111111";
const TWO: &str = "22222222-2222-4222-8222-222222222222";

fn reserve(task: &mut TaskDoc, id: &str, worker: &SessionKey, settle: bool) {
    let native = NativeRequest {
        machine: worker.machine.clone(),
        name: worker.session.clone(),
        run_id: "run-example".into(),
        conversation_id: "conversation-example".into(),
        digest: "ab".repeat(32),
    };
    task.reserve_attempt(id, Origin::Send, worker, native.clone(), &Actor::Person, 3)
        .unwrap();
    if settle {
        let receipt = Receipt {
            request_id: id.into(),
            name: native.name,
            run_id: native.run_id,
            conversation_id: native.conversation_id,
            first_message: false,
            digest: native.digest,
            submitted: true,
        };
        task.settle_attempt(id, Settlement::Receipt(&receipt), &Actor::Person, 4)
            .unwrap();
    }
}

fn conflict(task: &mut TaskDoc, worker: &SessionKey, unknown: bool) {
    task.floor().unwrap();
    let mut right = TaskDoc::from_bytes(&task.snapshot().unwrap()).unwrap();
    reserve(task, ONE, worker, true);
    reserve(&mut right, TWO, worker, !unknown);
    assert!(
        task.report_attempt(
            ONE,
            "Finished",
            ReportKind::Ready,
            None,
            &Actor::Agent(worker.clone()),
            5,
        )
        .unwrap()
        .moves
    );
    task.import(&right.snapshot().unwrap()).unwrap();
    let execution = task.view().unwrap().execution.unwrap();
    assert!(execution.conflict);
    assert_eq!(!execution.unknown.is_empty(), unknown);
}

#[test]
fn task_review_projection_produces_stream_search_and_inspector_fixtures() {
    let scratch = tempfile::tempdir().unwrap();
    let project = "example-project";
    let store = ProjectStore::open(scratch.path(), project).unwrap();
    let worker = agent();
    let cases = [
        ("Person", Some(ReviewOwner::Person), None),
        ("Technical", Some(ReviewOwner::Technical), None),
        ("Legacy", None, None),
        ("Team", None, None),
        ("Conflict", Some(ReviewOwner::Person), Some(false)),
        ("Unknown", Some(ReviewOwner::Person), Some(true)),
    ];
    let mut details = Vec::new();
    let mut uuids = Vec::new();
    for (n, (name, owner, contested)) in cases.into_iter().enumerate() {
        let mut task = TaskDoc::create(NewTask {
            project,
            id: &format!("T-aaaa0{n}"),
            text: "Check the sample",
            stage: Stage::Todo,
            target: Target::Any,
            author: Actor::Person,
            rank: ["U", "V", "W", "X", "Y", "Z"][n],
            now: 1,
        })
        .unwrap();
        task.set_title(name, &Actor::Person, 2).unwrap();
        if let Some(owner) = owner {
            let assignment = ReviewAssignment::new(owner, "Check the fixture".into()).unwrap();
            task.set_review_assignment(Some(&assignment), &Actor::Person, 2)
                .unwrap();
        }
        if name == "Team" {
            task.set_team(
                Some(&TaskTeam {
                    team: ONE.into(),
                    home: TWO.into(),
                }),
                &Actor::Person,
                2,
            )
            .unwrap();
        }
        if let Some(unknown) = contested {
            conflict(&mut task, &worker, unknown);
        } else {
            let actor = Actor::Agent(worker.clone());
            task.record_start(&actor, 3).unwrap();
            assert!(task.record_report("Finished", &actor, 5).unwrap());
        }
        assert_eq!(task.stage().unwrap(), Stage::Review);
        store.save_legacy_for_test(&mut task).unwrap();
        uuids.push(task.uuid().to_owned());

        let task = store.load(task.uuid()).unwrap();
        let mut header = history::materialize(&task).unwrap().header;
        // These are the same identity and history page fields passed by the
        // bounded task reader. The renderer fixtures do not render the log.
        header["uuid"] = json!(stable_uuid(n));
        let reply = json!({
            "project": {"id": project},
            "task": history::finish(header, Vec::new(), 0, None),
        });
        let checked =
            task_details::checked(reply.as_object().unwrap(), project, &stable_uuid(n)).unwrap();
        details.push(task_details::present(checked, |_, _| Ok(json!([]))).unwrap());
    }

    let indexed = all_pages(&store, Stage::Review, 2);
    let query = Scope::searched(None, "sample", "Example");
    let searched = Index::open(&store)
        .unwrap()
        .page_in(Stage::Review, 50, &Anchor::Start, Some(&query))
        .unwrap();
    assert_eq!(searched.rows, indexed);
    let rows: Vec<_> = uuids
        .iter()
        .enumerate()
        .map(|(n, uuid)| {
            let mut row = indexed
                .iter()
                .find(|row| row["uuid"] == *uuid)
                .unwrap()
                .clone();
            // IDs and change revisions are generated by each temporary store.
            row["uuid"] = json!(stable_uuid(n));
            row.as_object_mut().unwrap().remove("history_revision");
            row
        })
        .collect();
    let counts = json!({"review": rows.len(), "active": 0, "todo": 0, "backlog": 0, "done": 0});
    let project = json!({"id": project, "name": "Example", "accessible": true});
    let raw = json!({
        "tasks": rows, "project": project, "counts": counts, "query": "sample",
        "page": {"schema": 1, "stage": "review", "next": null},
    });
    let search = desktop_tasks::checked_queried_page(
        raw.as_object().unwrap(),
        "example-project",
        "review",
        50,
        None,
        Some("sample"),
    )
    .unwrap();
    let mut stream = Coalescer::new();
    stream.push(Change::Tasks {
        computer: ComputerId::from(""),
        key: "example-project".into(),
        update: desktop_tasks::TasksUpdate::List {
            tasks: rows,
            project,
            damaged: json!([]),
            digest: "review-fixture".into(),
            truncated: false,
            window: Value::Null,
        },
    });
    let stream = stream.take().tasks.remove(0);
    assert_eq!(stream.payload["tasks"], search["tasks"]);
    for (index, detail) in details.iter().enumerate() {
        assert_eq!(
            search["tasks"][index]["execution"],
            details[index]["execution"]
        );
        assert_eq!(
            search["tasks"][index]["review_assignment"],
            detail["review_assignment"]
        );
        assert_eq!(search["tasks"][index]["team"], detail["team"]);
    }
    assert_eq!(details[2]["review_assignment"], Value::Null);
    assert_eq!(details[3]["team"]["team"], ONE);
    assert_eq!(details[4]["execution"]["conflict"], true);
    assert_eq!(details[4]["execution"]["has_unknown"], false);
    assert_eq!(details[5]["execution"]["conflict"], true);
    assert_eq!(details[5]["execution"]["has_unknown"], true);
    assert_eq!(details[5]["unknown_attempts"], json!([TWO]));

    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("client/src/ui/screens/fixtures/task-review-projected.json");
    let output = serde_json::to_string_pretty(&json!({
        "details": details, "stream": stream, "search": search,
    }))
    .unwrap()
        + "\n";
    if std::env::var_os("ZERUS_BLESS").is_some() {
        fs::write(&file, &output).unwrap();
    }
    assert_eq!(fs::read_to_string(file).unwrap(), output);
}

fn stable_uuid(index: usize) -> String {
    format!("00000000-0000-4000-8000-00000000010{index}")
}
