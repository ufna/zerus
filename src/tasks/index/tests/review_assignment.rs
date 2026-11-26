//! The desktop review fixture comes from the task writer and the indexed
//! page. The document fallback and inspector read the same typed assignment.
use super::*;
use zerus_replica::{ReviewAssignment, ReviewOwner};

#[test]
fn task_review_assignment_reaches_the_index_and_inspector() {
    let scratch = tempfile::tempdir().unwrap();
    let store = ProjectStore::open(scratch.path(), "example").unwrap();
    let worker = Actor::Agent(agent());
    let mut review_uuids = Vec::new();
    for (n, (owner, criterion, cancelled)) in [
        (
            Some(ReviewOwner::Person),
            "Approve the sample output",
            false,
        ),
        (
            Some(ReviewOwner::Technical),
            "Verify the test results",
            false,
        ),
        (None, "", false),
        (Some(ReviewOwner::Person), "Check the sample output", true),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("T-aaaa0{n}");
        let mut task = TaskDoc::create(NewTask {
            project: store.project_id(),
            id: &id,
            text: "Check the sample",
            stage: Stage::Todo,
            target: Target::Any,
            author: worker.clone(),
            rank: ["U", "V", "W", "X"][n],
            now: 1,
        })
        .unwrap();
        let assignment = owner.map(|owner| ReviewAssignment {
            owner,
            criterion: criterion.into(),
        });
        if let Some(assignment) = &assignment {
            task.set_review_assignment(Some(assignment), &worker, 2)
                .unwrap();
        }
        task.record_start(&worker, 3).unwrap();
        assert!(task.set_review_assignment(None, &worker, 4).is_err());
        if cancelled {
            task.record_cancel("Stopped", &Actor::Person, 17).unwrap();
        } else {
            assert!(task.record_report("Finished", &worker, 17).unwrap());
        }
        assert!(task.set_review_assignment(None, &worker, 18).is_err());
        assert_eq!(task.stage().unwrap(), Stage::Review);
        store.save_legacy_for_test(&mut task).unwrap();
        review_uuids.push(task.uuid().to_owned());

        let reloaded = store.load(task.uuid()).unwrap();
        assert_eq!(reloaded.view().unwrap().review_assignment, assignment);
        let details = history::materialize(&reloaded).unwrap().header;
        assert_eq!(details["review_assignment"], json!(assignment));
    }

    let indexed = all_pages(&store, Stage::Review, 2);
    assert_eq!(indexed.len(), 4);
    let names = [
        "Needs a decision",
        "Needs a technical check",
        "Review owner unspecified",
        "Cancelled work",
    ];
    let fixture: Vec<_> = review_uuids
        .iter()
        .enumerate()
        .map(|(n, uuid)| {
            let row = indexed.iter().find(|row| row["uuid"] == *uuid).unwrap();
            // Retain relevant wire fields. Replace only the generated UUID
            // with a stable fixture UUID and add a distinguishable title.
            let mut out = json!({
                "uuid": format!("00000000-0000-4000-8000-00000000000{n}"),
                "id": row["id"], "stage": row["stage"], "summary": row["summary"],
                "title": names[n], "author": row["author"], "writable": row["writable"],
                "cancelled_at": row["cancelled_at"],
            });
            if let Some(assignment) = row.get("review_assignment") {
                out["review_assignment"] = assignment.clone();
            }
            out
        })
        .collect();
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("client/src/ui/screens/fixtures/task-review-assignment.json");
    let output = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
    if std::env::var_os("ZERUS_BLESS").is_some() {
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, &output).unwrap();
    }
    assert_eq!(fs::read_to_string(file).unwrap(), output);

    for uuid in review_uuids {
        let mut task = store.load(&uuid).unwrap();
        task.set_stage(Stage::Done, &Actor::Person, 19).unwrap();
        assert_eq!(task.stage().unwrap(), Stage::Done);
    }
}

#[test]
fn review_owner_command_requires_an_explicit_owner_and_checked_criterion() {
    let args = |parts: &[&str]| parts.iter().map(|part| (*part).into()).collect::<Vec<_>>();
    let command = || args(&["T-aaaa01", "person", "--json"]);
    let assignment =
        super::super::super::review_assignment(&command(), || Ok(" Confirm the sample\n".into()))
            .unwrap()
            .unwrap();
    assert_eq!(assignment.owner, ReviewOwner::Person);
    assert_eq!(assignment.criterion, "Confirm the sample");
    for invalid in ["   ".into(), "one\ntwo".into(), "é".repeat(257)] {
        assert!(super::super::super::review_assignment(&command(), || Ok(invalid)).is_err());
    }
    for invalid in [
        args(&["T-aaaa01", "reviewer"]),
        args(&["T-aaaa01", "person", "--agent", "pretend"]),
        args(&["T-aaaa01", "person", "--json", "--json"]),
    ] {
        assert!(
            super::super::super::review_assignment(&invalid, || panic!("unexpected read")).is_err()
        );
    }
    assert_eq!(
        super::super::super::review_assignment(&args(&["T-aaaa01", "unspecified"]), || panic!(
            "clear reads nothing"
        ))
        .unwrap(),
        None
    );
}


#[test]
fn reported_review_classification_preserves_work_and_filters_the_full_scope() {
    let scratch = tempfile::tempdir().unwrap();
    let store = ProjectStore::open(scratch.path(), "example").unwrap();
    let worker = agent();
    let other = SessionKey { session_id: "run_id:other".into(), ..worker.clone() };
    let person = ReviewAssignment::new(ReviewOwner::Person, "Approve the result".into()).unwrap();
    let technical = ReviewAssignment::new(ReviewOwner::Technical, "Verify the logs".into()).unwrap();
    let mut uuids = Vec::new();

    for (n, (key, choice)) in [
        (&worker, Some(&person)), (&worker, None),
        (&other, Some(&technical)), (&worker, Some(&person)),
    ].into_iter().enumerate() {
        let owner = Actor::Agent(key.clone());
        let title = if n == 3 { "A separate result" } else { "Release evidence" };
        let mut task = TaskDoc::create(NewTask {
            project: store.project_id(), id: &format!("T-aaaa0{n}"), text: title,
            stage: Stage::Todo, target: Target::Any, author: owner.clone(),
            rank: ["U", "V", "W", "X"][n], now: 1,
        }).unwrap();
        task.record_start(&owner, 2).unwrap();
        assert!(task.record_report("Original result", &owner, 3).unwrap());
        let before = task.view().unwrap();
        let seen = task.revision();
        task.classify_review(&seen, choice, &Actor::Person, 4).unwrap();
        let after = task.view().unwrap();
        assert_eq!((after.stage, after.stage_at, &after.uuid), (Stage::Review, before.stage_at, &before.uuid));
        assert_eq!(after.execution, before.execution);
        assert_eq!(after.session, before.session);
        assert_eq!(after.execution_id(), before.execution_id());
        assert_eq!(after.summary, before.summary);
        assert_eq!(&after.log[..before.log.len()], &before.log);
        if choice.is_some() {
            assert!(task.classify_review(&seen, None, &Actor::Person, 5).is_err());
        }
        store.save_legacy_for_test(&mut task).unwrap();
        uuids.push(task.uuid().to_owned());
    }

    let all = Scope::review(None, ReviewLane::All);
    let only = Scope::new(&worker.machine, &worker.session_id);
    let phrase = Scope::searched(Some(only), "Release evidence", "Example");
    let scope = Scope::review(Some(phrase), ReviewLane::Person);
    let person_queue = Scope::review(None, ReviewLane::Person);
    let mut index = Index::open(&store).unwrap();
    assert_eq!(index.review_counts(&all).unwrap(), BTreeMap::from([
        ("person".into(), 2), ("technical".into(), 1), ("triage".into(), 1), ("team".into(), 0),
    ]));
    let filtered = super::super::super::indexed_page_in(
        &mut index, Stage::Review, 1, &Anchor::Start, None, Some(&scope),
    ).unwrap();
    assert_eq!(filtered["review_counts"], json!({"person": 1, "technical": 0, "triage": 1, "team": 0}));
    assert_eq!(filtered["page"]["total"], 1);
    assert_eq!(filtered["page"]["next"], Value::Null);
    assert_eq!(filtered["tasks"][0]["uuid"], uuids[0]);
    let first = index.page_in(Stage::Review, 1, &Anchor::Start, Some(&person_queue)).unwrap();
    let cursor = first.next.unwrap();
    assert!(parse_cursor(&all.cursor_project("example"), Stage::Review, &cursor).is_err());
    let (rank, uuid) = parse_cursor(&person_queue.cursor_project("example"), Stage::Review, &cursor).unwrap();
    let last = index.page_in(Stage::Review, 1, &Anchor::After(rank, uuid), Some(&person_queue)).unwrap();
    assert_eq!(last.rows[0]["uuid"], uuids[3]);
    assert_eq!(last.position, 1);
    assert!(last.previous.is_some());
    assert!(last.next.is_none());
    drop(index);

    let mut reopened = store.load(&uuids[0]).unwrap();
    reopened.set_stage(Stage::Todo, &Actor::Person, 6).unwrap();
    reopened.record_start(&Actor::Agent(worker.clone()), 7).unwrap();
    assert!(reopened.record_report("New result", &Actor::Agent(worker), 8).unwrap());
    let new_report = reopened.view().unwrap();
    assert_eq!(new_report.review_lane(), Some("triage"));
    assert!(new_report.review_editable());
    let revision = reopened.revision();
    reopened.classify_review(&revision, Some(&technical), &Actor::Person, 9).unwrap();
    assert_eq!(reopened.view().unwrap().review_lane(), Some("technical"));
    reopened.set_stage(Stage::Done, &Actor::Person, 10).unwrap();
    assert_eq!(reopened.stage().unwrap(), Stage::Done);
}

#[test]
fn conflicting_legacy_results_stay_in_triage() {
    let worker = Actor::Agent(agent());
    let mut task = TaskDoc::create(NewTask {
        project: "example",
        id: "T-aaaaba",
        text: "Read both observations",
        stage: Stage::Todo,
        target: Target::Any,
        author: worker.clone(),
        rank: "U",
        now: 1,
    }).unwrap();
    task.record_start(&worker, 2).unwrap();
    let mut other = TaskDoc::from_bytes(&task.snapshot().unwrap()).unwrap();
    assert!(task.record_report("Result from the left replica", &worker, 3).unwrap());
    assert!(other.record_report("Result from the right replica", &worker, 4).unwrap());
    task.import(&other.snapshot().unwrap()).unwrap();

    let view = task.view().unwrap();
    assert_eq!(view.stage, Stage::Review);
    assert_eq!(view.log.iter().filter(|entry| entry.kind == "report").count(), 2);
    assert_eq!(view.review_lane(), Some("triage"));
    let revision = task.revision();
    let assignment = ReviewAssignment::new(ReviewOwner::Person, "Pick a result".into()).unwrap();
    assert!(task.classify_review(&revision, Some(&assignment), &Actor::Person, 5).is_err());
    assert_eq!(task.revision(), revision);
}
