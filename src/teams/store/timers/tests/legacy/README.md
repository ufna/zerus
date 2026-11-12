# Legacy hold stores

SQL dumps of stores that the old commits wrote with their own operations, for
the step to layout 11 (`../legacy_holds.rs`). Each dump ends with the store's
`user_version`, which a dump does not carry.

| Layout | Commit | Cases |
|---|---|---|
| 8 | 40046fa3f0c0a21371ea2e618e9e963c4adb5646 | holds, pipeline_active, pipeline_person |
| 9 | 452bec1fe71057cbbe04ff42954dc83bea13373d | holds, pipeline_active, pipeline_person, stranded |
| 10 | c29dc402e05089ee80f7d734d8ea4b1c6502095c | holds, pipeline_active, pipeline_person, removed |

To regenerate one, check the commit out in its own worktree, add the module
below as `src/teams/ops/tests/wp10_legacy.rs` with `#[cfg(test)] mod wp10_legacy;`
at the end of `src/teams/ops/tests/mod.rs`, and run, with a build target of that
worktree's own (a shared target reuses another checkout's test binary):

    WP10_CASE=<case> WP10_KEEP=<new directory on the same filesystem as TMPDIR> \
      cargo +1.99.1 test --locked --bin hgs -- --exact teams::ops::tests::wp10_legacy::wp10_generate_legacy
    { sqlite3 <directory>/teams/teams-v1.sqlite3 .dump; \
      echo "PRAGMA user_version=$(sqlite3 <directory>/teams/teams-v1.sqlite3 'PRAGMA user_version');"; } > layout<N>-<case>.sql

```rust
//! Legacy store generator for the hold-rule regression controls (zerus-63rru.42).
//! Added temporarily to an old commit's ops tests; only public operations run.
use super::*;

fn reported(h: &mut Harness, n: u32) -> u32 {
    let node = h.add(&person(), n);
    h.submit(node, "m1");
    h.report(node, "m1");
    node
}

fn cancel_refused(h: &mut Harness, lead: &Actor, node: u32) {
    h.run(lead, Operation::Cancel { node, reason: "Cancel for the legacy hold".into() }).unwrap();
    let row = h.store.pending_cancels(TEAM, 16, 0).unwrap().into_iter().find(|r| r.node == node).unwrap();
    h.run(&service(), Operation::RecordCancel {
        node, decision: row.decision, state: CancelState::Refused,
        reason: "Still running".into(), attempt: row.attempt, attempt_id: row.attempt_id,
    }).unwrap();
    assert!(h.node(node).hold);
}

fn reopen_with(h: &mut Harness, lead: &Actor, node: u32, outcome: HandoutOutcome) {
    h.run(lead, Operation::Return { node, reason: "Reopen for the legacy hold".into() }).unwrap();
    let id = h.reopen(node, "m1");
    h.reopen_outcome(&id, outcome);
    assert!(h.node(node).hold);
}

fn keep(h: Harness) {
    drop(h.store);
    let destination = std::env::var_os("WP10_KEEP").expect("destination");
    let kept = h.temp.keep();
    std::fs::rename(kept, destination).unwrap();
}

fn stages(h: &Harness, parent: u32) -> Vec<Node> {
    h.store.queue(TEAM, 200, None).unwrap().rows.into_iter()
        .filter(|n| n.kind == NodeKind::Stage && n.stage_of == Some(parent)).collect()
}

#[test]
fn wp10_generate_legacy() {
    let case = std::env::var("WP10_CASE").unwrap();
    match case.as_str() {
        "holds" => {
            let mut h = Harness::new();
            let lead = h.bind("lead");
            h.bind("m1");
            h.bind("m2");
            // 1: the reviewer's sequence. A hand-back, the lead's Release, the person's Hold.
            let node = reported(&mut h, 1);
            cancel_refused(&mut h, &lead, node);
            h.run(&lead, Operation::Release { node }).unwrap();
            h.run(&person(), Operation::Hold { node }).unwrap();
            // 2: the person's Hold, then a hand-back.
            let node = reported(&mut h, 2);
            h.run(&person(), Operation::Hold { node }).unwrap();
            cancel_refused(&mut h, &lead, node);
            // 3: the lead's Hold, then nothing.
            let node = h.add(&person(), 3);
            h.run(&lead, Operation::Hold { node }).unwrap();
            // 4: a hand-back, the lead's Release, the lead's Hold.
            let node = reported(&mut h, 4);
            cancel_refused(&mut h, &lead, node);
            h.run(&lead, Operation::Release { node }).unwrap();
            h.run(&lead, Operation::Hold { node }).unwrap();
            // 5: a refused reopen, placed by its effect.
            let node = reported(&mut h, 5);
            reopen_with(&mut h, &lead, node, HandoutOutcome::Refused { reason: "Not delivered".into() });
            // 6: a reopen cutting over, the lead's Release, the person's Hold.
            let node = reported(&mut h, 6);
            reopen_with(&mut h, &lead, node, HandoutOutcome::CuttingOver { reason: "Replacement starting".into() });
            h.run(&lead, Operation::Release { node }).unwrap();
            h.run(&person(), Operation::Hold { node }).unwrap();
            // 7: the person's Hold, then nothing.
            let node = h.add(&person(), 7);
            h.run(&person(), Operation::Hold { node }).unwrap();
            keep(h);
        }
        "pipeline_active" | "pipeline_person" => {
            let mut h = Harness::empty();
            let mut create = create();
            if let Operation::Create { bundle, .. } = &mut create {
                if case == "pipeline_person" {
                    bundle.pipelines[0].accept = Acceptance::Person;
                }
                bundle.template.default_pipeline = "reviewed".into();
            }
            h.run(&person(), create).unwrap();
            h.bind("lead");
            h.bind("m1");
            h.bind("m3");
            let parent = h.add(&person(), 1);
            h.submit(parent, "m1");
            h.report(parent, "m1");
            if case == "pipeline_person" {
                let stage = stages(&h, parent)[0].id;
                let id = uuid::Uuid::new_v4().to_string();
                h.run(&service(), serde_json::from_value(serde_json::json!({"op":"stage_task_intent","node":stage,"effect_request_id":id})).unwrap()).unwrap();
                h.run(&service(), serde_json::from_value(serde_json::json!({"op":"stage_task_outcome","effect_request_id":id,"outcome":{"status":"created","task":task(1000+stage)}})).unwrap()).unwrap();
                h.observe(stage, TaskStage::Todo);
                h.submit(stage, "m3");
                let reviewer = Actor::Session {
                    person: "local".into(), team: TEAM.into(), member: "m3".into(),
                    session: h.store.member(TEAM, "m3").unwrap().unwrap().session.unwrap(),
                };
                h.run(&reviewer, serde_json::from_value(serde_json::json!({"op":"verdict","node":stage,"generation":1,"pass":true,"text":"Checked the criteria"})).unwrap()).unwrap();
            }
            assert!(h.node(parent).hold);
            keep(h);
        }
        "removed" => {
            let mut h = Harness::new();
            let lead = h.bind("lead");
            h.bind("m1");
            h.bind("m2");
            let node = reported(&mut h, 1);
            h.run(&lead, Operation::Reassign { node, member: "m2".into(), reason: "Member removal hand-back".into() }).unwrap();
            h.run(&person(), Operation::ArchiveMember { member: "m2".into() }).unwrap();
            assert!(h.node(node).hold);
            keep(h);
        }
        "stranded" => {
            let mut h = Harness::new();
            let lead = h.bind("lead");
            h.bind("m1");
            let node = h.add(&person(), 1);
            let _id = h.intent(node, "m1");
            h.observe(node, TaskStage::InReview);
            h.run(&lead, Operation::Return { node, reason: "A Return admitted while the hand-out was reserved".into() }).unwrap();
            assert!(h.node(node).hold);
            keep(h);
        }
        _ => panic!("unknown case"),
    }
}
```
