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
