//! A member bound to a launch resolves through its durable handle, its
//! session's own ID (zerus-63rru.53), and no longer through the launch's
//! evidence, which goes when the first run ends and after a week. Records are
//! the supervisor's own construction, saved, resumed, renamed, started fresh
//! and archived by the product's functions, and fleet rows are made by the
//! listing's own steps, in a child test process whose state root is private.
use super::super::availability::{current, Current, Unresolved};
use super::super::native::Native;
use super::production::fleet::session_of;
use super::real_rows::{isolated, Production};
use crate::state::listing_fixture as producer;
use crate::teams::ops::tests::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use zerus_team_plan::model::{Member, Team};

const CHILD: &str = "desktop::teams::actor::tests::handles::";
const OTHER_LAUNCH: &str = "00000000-0000-4000-8000-0000000000e1";
const OTHER_SESSION: &str = "00000000-0000-4000-8000-0000000000e2";

struct World {
    h: Harness,
    native: Native,
    records: BTreeMap<String, Value>,
}

fn name(member: &str) -> String {
    format!("claude/fixture/{member}")
}
fn run(member: &str) -> String {
    format!("fixture-{member}")
}
fn launch_of(h: &Harness, member: &str) -> String {
    let m = h.store.member(TEAM, member).unwrap().unwrap();
    m.session.unwrap().session_id["launch_id:".len()..].to_owned()
}

/// A team whose `members` are bound to their launches. Each has the record
/// its supervisor writes, saved by the record writer at its label's path,
/// its launch's evidence as the launch writes it, and its idle fleet row.
fn world(members: &[&str]) -> World {
    let mut h = Harness::new();
    // The state root of the records is the team's own.
    std::env::set_var("HGS_STATE_DIR", h.temp.path());
    for m in members {
        h.bind(m);
    }
    let native = super::production::native(&h);
    let mut records = BTreeMap::new();
    for member in members {
        let m = h.store.member(TEAM, member).unwrap().unwrap();
        let record = adopt(&h, &m, &m.worktree);
        records.insert((*member).to_owned(), record);
    }
    World { h, native, records }
}

/// Replaces the fixture's seeded record and evidence of `m` by the ones the
/// product writes, its session working in `cwd`.
fn adopt(h: &Harness, m: &Member, cwd: &str) -> Value {
    let state = h.temp.path();
    let sid = session_of(&m.id);
    let launch = launch_of(h, &m.id);
    std::fs::remove_file(state.join("sessions").join(format!("{sid}.json"))).unwrap();
    std::fs::remove_file(state.join("launches").join(format!("{launch}.json"))).unwrap();
    let mut record = producer::launched_record(
        &m.agent,
        &name(&m.id),
        &launch,
        &sid,
        &run(&m.id),
        cwd,
        &m.account,
    );
    producer::confirmed(&mut record, &format!("conversation-{}", m.id));
    producer::save(&mut record).unwrap();
    producer::launch_started(&launch, &name(&m.id), &sid).unwrap();
    record
}

fn evidence(w: &World, launch: &str) -> std::path::PathBuf {
    w.h.temp
        .path()
        .join("launches")
        .join(format!("{launch}.json"))
}

/// Commits the idle row the listing makes for `record` under its run.
fn row_for(w: &World, record: &Value) {
    let row = producer::listed_live_row(record.clone()).unwrap();
    let key = format!("run:{}", record["run_id"].as_str().unwrap());
    let json = row.to_string();
    rusqlite::Connection::open(w.native.config.state.join("desktop/state.sqlite3"))
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,?1,?2,?3)",
            rusqlite::params![key, json, json.len()],
        )
        .unwrap();
}

fn team(w: &World) -> Team {
    w.h.store.team(TEAM).unwrap().unwrap()
}
fn member(w: &World, id: &str) -> Member {
    w.h.store.member(TEAM, id).unwrap().unwrap()
}
/// The production free check (it records a missing handle once verified).
fn free(w: &mut World, id: &str) -> bool {
    let (t, m) = (team(w), member(w, id));
    super::super::ports::Ports::free(&mut w.native, &t, &m).unwrap()
}
fn now(w: &World, id: &str) -> Current {
    current(&w.native.config, TEAM, &member(w, id), false).unwrap()
}

/// One actor tick over one new task: what the production ports delivered.
fn hand_out(w: World) -> Vec<String> {
    let mut h = w.h;
    h.add(&person(), 1);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let mut p = Production {
        native: w.native,
        deliveries: vec![],
    };
    a.tick(&mut h.store, &mut p, TEAM, 100, true).unwrap();
    p.deliveries
}
fn handed_out(w: World) {
    let deliveries = hand_out(w);
    assert_eq!(deliveries.len(), 1, "{deliveries:?}");
    assert!(
        deliveries[0].starts_with("m1 -> Outcome(Submitted"),
        "{deliveries:?}"
    );
}

/// The first free read records the handle while the evidence still exists.
fn bound(w: &mut World) {
    assert!(free(w, "m1"));
    let handle = w.h.store.member_handle(TEAM, "m1").unwrap().unwrap();
    assert_eq!(handle.session_id, session_of("m1"));
    assert_eq!(handle.label_hint, name("m1"));
}

/// The first run ends with its conversation confirmed: its supervisor
/// removes the launch's evidence.
fn first_run_ends(w: &World) {
    let launch = launch_of(&w.h, "m1");
    producer::launch_finished(&launch, &name("m1"), &run("m1"));
    assert!(!evidence(w, &launch).exists(), "the evidence stayed");
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn resumed_after_its_first_run_child() {
    let mut w = world(&["m1"]);
    bound(&mut w);
    first_run_ends(&w);
    // A person resumes it: another run of the same session.
    let mut resumed = producer::resumed_record(&w.records["m1"], "run-resumed");
    producer::confirmed(&mut resumed, "conversation-m1");
    producer::save(&mut resumed).unwrap();
    row_for(&w, &resumed);
    assert_eq!(now(&w, "m1"), Current::Run("run-resumed".into()));
    handed_out(w);
}
#[test]
fn a_member_resumed_after_its_first_run_ended_is_free_again() {
    isolated(&format!("{CHILD}resumed_after_its_first_run_child"));
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn eight_days_after_its_launch_child() {
    let mut w = world(&["m1"]);
    bound(&mut w);
    let launch = launch_of(&w.h, "m1");
    producer::age_launch(&launch, 8);
    // A later launch prunes evidence older than a week.
    producer::launch_started(OTHER_LAUNCH, "claude/fixture/other", OTHER_SESSION).unwrap();
    assert!(!evidence(&w, &launch).exists(), "the old evidence stayed");
    assert_eq!(now(&w, "m1"), Current::Run(run("m1")));
    handed_out(w);
}
#[test]
fn a_member_eight_days_after_its_launch_is_free() {
    isolated(&format!("{CHILD}eight_days_after_its_launch_child"));
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn renamed_child() {
    let mut w = world(&["m1"]);
    bound(&mut w);
    first_run_ends(&w);
    producer::rename(&name("m1"), "claude/fixture/m1-renamed").unwrap();
    let mut renamed = w.records["m1"].clone();
    renamed["name"] = json!("claude/fixture/m1-renamed");
    row_for(&w, &renamed);
    assert_eq!(now(&w, "m1"), Current::Run(run("m1")));
    handed_out(w);
}
#[test]
fn a_renamed_member_is_free_under_its_new_name() {
    isolated(&format!("{CHILD}renamed_child"));
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn archived_child() {
    let mut w = world(&["m1"]);
    bound(&mut w);
    producer::archive(&w.records["m1"]).unwrap();
    assert_eq!(now(&w, "m1"), Current::Unresolved(Unresolved::Gone));
    assert!(!free(&mut w, "m1"));
    assert_eq!(hand_out(w), Vec::<String>::new());
}
#[test]
fn an_archived_member_is_not_free_and_is_named_gone() {
    isolated(&format!("{CHILD}archived_child"));
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn started_fresh_child() {
    let mut w = world(&["m1"]);
    bound(&mut w);
    first_run_ends(&w);
    let mut fresh = producer::started_fresh_record(&w.records["m1"], "run-fresh");
    producer::save(&mut fresh).unwrap();
    row_for(&w, &fresh);
    assert_eq!(now(&w, "m1"), Current::Unresolved(Unresolved::StartedFresh));
    // And once its new conversation started.
    producer::confirmed(&mut fresh, "conversation-fresh");
    fresh.as_object_mut().unwrap().remove("fresh_from");
    fresh["session_clear"] = json!({"source":"fresh_start","run_id":"run-fresh",
        "conversation_id":"conversation-fresh","previous_conversation_id":"conversation-m1"});
    producer::save(&mut fresh).unwrap();
    assert_eq!(now(&w, "m1"), Current::Unresolved(Unresolved::StartedFresh));
    assert!(!free(&mut w, "m1"));
    assert_eq!(hand_out(w), Vec::<String>::new());
}
#[test]
fn a_member_started_fresh_is_not_free_and_is_named() {
    isolated(&format!("{CHILD}started_fresh_child"));
}

/// A member bound before handles: none recorded, the evidence gone. The
/// launch ledger's observation at binding gives the handle; a read never
/// writes it, the free check does, once.
#[test]
#[ignore = "run by its isolated wrapper"]
fn backfilled_from_the_ledger_child() {
    use crate::desktop::teams::ledger::{Ledger, Row, State};
    use crate::desktop::teams::ports::{ObservedSession, Worktree};
    let mut w = world(&["m1"]);
    first_run_ends(&w);
    assert_eq!(now(&w, "m1"), Current::Unresolved(Unresolved::NoHandle));
    assert!(!free(&mut w, "m1"));
    let t = team(&w);
    let m = member(&w, "m1");
    let launch = launch_of(&w.h, "m1");
    let observed = |session: &str| ObservedSession {
        machine: t.home.clone(),
        launch_id: launch.clone(),
        agent: m.agent.clone(),
        account: m.account.clone(),
        worktree: Worktree {
            path: m.worktree.clone(),
            ownership_nonce: m.ownership_nonce.clone(),
        },
        parent: None,
        name: name("m1"),
        session_id: session.into(),
        run_id: run("m1"),
        conversation_id: "conversation-m1".into(),
    };
    // A ledger value no session carries is never recorded.
    let mut row = Row::new(&t, "m1", "epoch", State::Done);
    row.observed = Some(observed(OTHER_SESSION));
    let mut ledger = Ledger::open(w.h.temp.path()).unwrap();
    assert!(ledger.insert(&row, 1).unwrap());
    assert_eq!(now(&w, "m1"), Current::Unresolved(Unresolved::Gone));
    assert!(!free(&mut w, "m1"));
    assert_eq!(w.h.store.member_handle(TEAM, "m1").unwrap(), None);
    // The value the launcher saw at binding.
    let mut row = ledger.get(TEAM, "m1").unwrap().unwrap();
    row.observed = Some(observed(&session_of("m1")));
    ledger.save(&mut row, 2).unwrap();
    drop(ledger);
    assert_eq!(now(&w, "m1"), Current::Run(run("m1")));
    assert_eq!(w.h.store.member_handle(TEAM, "m1").unwrap(), None);
    assert!(free(&mut w, "m1"));
    let handle = w.h.store.member_handle(TEAM, "m1").unwrap().unwrap();
    assert_eq!(handle.session_id, session_of("m1"));
    handed_out(w);
}
#[test]
fn a_member_bound_before_handles_takes_its_handle_from_the_launch_ledger() {
    isolated(&format!("{CHILD}backfilled_from_the_ledger_child"));
}
