//! The transition that adds this package to a store: one immediate
//! transaction, a marker written last, rerun whole when the marker is absent.
use super::*;
use crate::teams::store::timers::schema::{AFTER_FENCE, BEFORE_MARK, EXTENSION, TRANSITIONS};
use std::sync::{Arc, Barrier, Mutex};

/// Every object timers.sql creates, so that a store can be returned to its
/// layout from before this package.
fn objects() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for line in include_str!("../../timers.sql").lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let ["CREATE", kind, "IF", "NOT", "EXISTS", name, ..] = words.as_slice() {
            found.push((kind.to_string(), name.trim_end_matches('(').to_string()));
        }
    }
    found
}
fn strip(h: &Harness) {
    for (kind, name) in objects().into_iter().rev() {
        db(h)
            .execute_batch(&format!("DROP {kind} IF EXISTS {name};"))
            .unwrap();
    }
}
fn transitions() -> u32 {
    TRANSITIONS.with(|n| n.get())
}
fn names(h: &Harness) -> Vec<String> {
    column(
        db(h),
        "SELECT name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
        [],
    )
    .unwrap()
}
fn marker(h: &Harness) -> Option<u32> {
    db(h)
        .query_row("SELECT version FROM timers_extension WHERE id=1", [], |r| {
            r.get(0)
        })
        .ok()
}
fn stopped() -> Harness {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    let gone = h.add(&person(), 1);
    h.cancel_fact(gone);
    h.add(&person(), 2);
    h.run(&person(), Operation::Stop).unwrap();
    h
}

#[test]
fn the_strip_list_covers_every_object_of_this_package() {
    let h = Harness::new();
    let full = names(&h);
    strip(&h);
    let ours: Vec<String> = objects().into_iter().map(|(_, n)| n).collect();
    let left = names(&h);
    assert!(left.iter().all(|n| !ours.contains(n)), "{left:?}");
    assert!(full.len() > left.len());
}

#[test]
fn a_store_from_before_this_package_with_an_archived_node_opens() {
    let mut h = stopped();
    let full = names(&h);
    strip(&h);
    let before = transitions();
    reopen(&mut h);
    assert_eq!(transitions(), before + 1);
    assert_eq!(names(&h), full);
    assert_eq!(marker(&h), Some(EXTENSION));
    assert_eq!(h.store.keyed_teams("", 10).unwrap(), vec![TEAM.to_owned()]);
    assert_eq!(h.store.pending_cancels(TEAM, 200, 0).unwrap().len(), 1);
}

#[test]
fn a_stop_committed_while_the_transition_runs_waits_for_it_and_is_captured() {
    let mut h = Harness::new();
    h.add(&person(), 1);
    strip(&h);
    let state = h.temp.path().to_owned();
    let thread = Arc::new(Mutex::new(None));
    let gap = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (slot, seen) = (thread.clone(), gap.clone());
    BEFORE_MARK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            // Inside the open transaction: another writer cannot commit.
            let raw = rusqlite::Connection::open(state.join("teams/teams-v1.sqlite3")).unwrap();
            raw.busy_timeout(std::time::Duration::from_millis(50))
                .unwrap();
            let wrote = raw.execute("UPDATE teams SET name=name", []).is_ok();
            seen.store(wrote, std::sync::atomic::Ordering::Release);
            *slot.lock().unwrap() = Some(std::thread::spawn(move || {
                let mut store = Store::open(&state).unwrap();
                let rev = store.team(TEAM).unwrap().unwrap().revision;
                let stop = request(rev, Operation::Stop);
                crate::teams::ops::apply(&mut store, &person(), &stop, 1234).unwrap();
            }));
            Ok(())
        }));
    });
    reopen(&mut h);
    thread.lock().unwrap().take().unwrap().join().unwrap();
    assert!(
        !gap.load(std::sync::atomic::Ordering::Acquire),
        "no write commits inside the transition"
    );
    assert_eq!(
        h.store.keyed_teams("", 10).unwrap(),
        vec![TEAM.to_owned()],
        "the stop is captured"
    );
    assert_eq!(h.store.pending_cancels(TEAM, 200, 0).unwrap().len(), 1);
}

#[test]
fn every_partial_extension_completes_on_the_next_open() {
    let partial = [
        "CREATE TABLE key_clear_teams (team TEXT PRIMARY KEY, pending INTEGER NOT NULL);",
        "CREATE TABLE key_clear_teams (team TEXT PRIMARY KEY, pending INTEGER NOT NULL); CREATE TABLE key_clear_outbox (team TEXT NOT NULL, node INTEGER NOT NULL, PRIMARY KEY(team,node));",
        "",
    ];
    for step in partial {
        let mut h = stopped();
        let full = names(&h);
        if step.is_empty() {
            db(&h)
                .execute_batch("DROP TABLE timers_extension;")
                .unwrap();
        } else {
            strip(&h);
            db(&h).execute_batch(step).unwrap();
        }
        reopen(&mut h);
        assert_eq!(names(&h), full, "{step}");
        assert_eq!(marker(&h), Some(EXTENSION));
        assert_eq!(
            h.store.keyed_teams("", 10).unwrap(),
            vec![TEAM.to_owned()],
            "{step}"
        );
    }
    let mut h = stopped();
    db(&h)
        .execute("UPDATE timers_extension SET version=0", [])
        .unwrap();
    let before = transitions();
    reopen(&mut h);
    assert_eq!(
        (transitions(), marker(&h)),
        (before + 1, Some(EXTENSION)),
        "an older marker reruns"
    );
}

#[test]
fn a_failed_transition_leaves_nothing_and_runs_again() {
    let mut h = stopped();
    let full = names(&h);
    strip(&h);
    let stripped = names(&h);
    BEFORE_MARK
        .with(|hook| *hook.borrow_mut() = Some(Box::new(|| Err(storage("injected failure")))));
    assert!(Store::open(h.temp.path()).is_err());
    assert_eq!(names(&h), stripped, "nothing of the transition remains");
    reopen(&mut h);
    assert_eq!(names(&h), full);
}

#[test]
fn two_processes_opening_at_once_both_open_and_one_extends() {
    let h = stopped();
    strip(&h);
    let barrier = Arc::new(Barrier::new(2));
    let openers: Vec<_> = (0..2)
        .map(|_| {
            let (state, barrier) = (h.temp.path().to_owned(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                let opened = Store::open(&state).map(|_| ());
                (opened, transitions())
            })
        })
        .collect();
    let results: Vec<_> = openers.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(
        results.iter().all(|(opened, _)| opened.is_ok()),
        "{results:?}"
    );
    assert_eq!(results.iter().map(|(_, n)| n).sum::<u32>(), 1);
    assert_eq!(marker(&h), Some(EXTENSION));
}

#[test]
fn a_second_open_does_no_work_and_takes_no_lock() {
    let mut h = stopped();
    let raw = rusqlite::Connection::open(h.temp.path().join("teams/teams-v1.sqlite3")).unwrap();
    raw.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let before = transitions();
    let started = std::time::Instant::now();
    reopen(&mut h);
    assert_eq!(transitions(), before);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    raw.execute_batch("ROLLBACK;").unwrap();
}

/// The one-time cost at two archive sizes, by SQLite's progress handler.
#[test]
fn the_transition_walks_each_archived_node_once() {
    let mut costs = Vec::new();
    for archived in [1_000u32, 10_000] {
        let mut h = stopped();
        let db = db(&h);
        db.execute("WITH RECURSIVE n(x) AS (VALUES(1000) UNION ALL SELECT x+1 FROM n WHERE x<?2+999) INSERT INTO node_identity SELECT ?1,x,'example-project',printf('00000000-0000-4000-8000-%012x',x) FROM n", rusqlite::params![TEAM, archived]).unwrap();
        let columns = crate::teams::store::rows::NODE_COLUMNS.replacen("team,id,", "", 1);
        db.execute(&format!("INSERT INTO node_archive(team,id,{columns}) SELECT i.team,i.id,{columns} FROM node_identity i, (SELECT {columns} FROM node_archive WHERE team=?1 LIMIT 1) WHERE i.team=?1 AND i.id>=1000"), [TEAM]).unwrap();
        strip(&h);
        let steps = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter = steps.clone();
        let started = std::time::Instant::now();
        let raw = rusqlite::Connection::open(h.temp.path().join("teams/teams-v1.sqlite3")).unwrap();
        raw.progress_handler(
            1000,
            Some(move || {
                counter.fetch_add(1000, std::sync::atomic::Ordering::Relaxed);
                false
            }),
        );
        super::super::schema::extend(&raw).unwrap();
        let elapsed = started.elapsed();
        let steps = steps.load(std::sync::atomic::Ordering::Relaxed);
        println!(
            "archived nodes={archived}: transition VM steps~{steps}, {:?}, {} steps per node",
            elapsed,
            steps / u64::from(archived)
        );
        costs.push(steps / u64::from(archived));
        reopen(&mut h);
    }
    assert!(
        costs[1] <= costs[0] * 2,
        "the walk is linear in the archive: {costs:?}"
    );
}

#[test]
fn a_version_one_outbox_moves_into_the_frontier() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    strip(&h);
    db(&h)
        .execute_batch(
            "CREATE TABLE key_clear_outbox (team TEXT NOT NULL, node INTEGER NOT NULL, PRIMARY KEY(team,node));
             CREATE TRIGGER key_clear_stage_settled_v1 AFTER UPDATE OF state ON stage_tasks BEGIN
              INSERT OR IGNORE INTO key_clear_outbox(team,node) VALUES(NEW.team,NEW.node); END;",
        )
        .unwrap();
    db(&h)
        .execute(
            "INSERT INTO key_clear_outbox VALUES(?1,?2)",
            params![TEAM, node],
        )
        .unwrap();
    reopen(&mut h);
    let stages: Vec<u32> = h
        .store
        .released_stages(TEAM, 200, 0)
        .unwrap()
        .iter()
        .map(|k| k.node)
        .collect();
    assert_eq!(stages, [node]);
    let left = names(&h);
    assert!(
        !left.contains(&"key_clear_outbox".to_owned())
            && !left.contains(&"key_clear_stage_settled_v1".to_owned())
    );
}

#[test]
fn a_store_whose_older_read_index_is_replaced_is_extended_too() {
    let mut h = stopped();
    let full = names(&h);
    strip(&h);
    db(&h)
        .execute_batch(
            "DROP INDEX integration_card;
             CREATE UNIQUE INDEX integration_card ON cards(decision_seq);",
        )
        .unwrap();
    reopen(&mut h);
    assert_eq!(names(&h), full);
    assert_eq!(marker(&h), Some(EXTENSION));
}

#[test]
fn a_marker_newer_than_this_program_refuses_the_open_and_writes_nothing() {
    let h = stopped();
    db(&h)
        .execute("UPDATE timers_extension SET version=?1", [EXTENSION + 1])
        .unwrap();
    let full = names(&h);
    let before = transitions();
    let refused = Store::open(h.temp.path()).unwrap_err();
    assert_eq!(refused.code, code::INVALID_STATE);
    assert!(
        refused
            .message
            .contains("store schema is newer than this version of Zerus"),
        "{}",
        refused.message
    );
    assert_eq!(transitions(), before, "no transition");
    assert_eq!(marker(&h), Some(EXTENSION + 1), "no lowered mark");
    assert_eq!(names(&h), full);
}

type Schema = Vec<(String, String, Option<String>)>;
/// Every schema object's definition, to compare a store byte for byte.
fn schema(h: &Harness) -> Schema {
    schema_of(db(h))
}
fn schema_of(db: &Connection) -> Schema {
    let mut s = db
        .prepare("SELECT type,name,sql FROM sqlite_master ORDER BY type,name")
        .unwrap();
    let rows = s
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
}
fn older_card_index(h: &Harness) {
    db(h).execute_batch("DROP INDEX integration_card; CREATE UNIQUE INDEX integration_card ON cards(decision_seq);").unwrap();
}

#[test]
fn a_store_marked_by_a_newer_program_is_refused_before_any_schema_write() {
    let h = stopped();
    db(&h)
        .execute("UPDATE timers_extension SET version=?1", [EXTENSION + 1])
        .unwrap();
    older_card_index(&h);
    db(&h)
        .execute_batch("DROP TABLE verdicts; DROP INDEX teams_live;")
        .unwrap();
    let before = schema(&h);
    let refused = Store::open(h.temp.path()).unwrap_err();
    assert!(
        refused
            .message
            .contains("store schema is newer than this version of Zerus"),
        "{}",
        refused.message
    );
    assert_eq!(
        schema(&h),
        before,
        "the index, the schema batches and the mark are untouched"
    );
    assert_eq!(marker(&h), Some(EXTENSION + 1));
}

#[test]
fn an_older_store_with_the_older_index_still_upgrades() {
    let mut h = stopped();
    let full = schema(&h);
    strip(&h);
    older_card_index(&h);
    reopen(&mut h);
    assert_eq!(schema(&h), full);
    assert_eq!(marker(&h), Some(EXTENSION));
}

/// Another program's commit right after this open's first, unlocked fence;
/// the schema it leaves is returned through the cell.
fn after_the_fence(h: &Harness, sql: String) -> std::rc::Rc<std::cell::RefCell<Schema>> {
    let file = h.temp.path().join("teams/teams-v1.sqlite3");
    let left = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = left.clone();
    AFTER_FENCE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            let other = Connection::open(file).unwrap();
            other.execute_batch(&sql).unwrap();
            *seen.borrow_mut() = schema_of(&other);
            Ok(())
        }))
    });
    left
}
fn refused_as_newer(h: &Harness) {
    let refused = Store::open(h.temp.path()).unwrap_err();
    assert!(
        refused
            .message
            .contains("store schema is newer than this version of Zerus"),
        "{}",
        refused.message
    );
}

/// The reviewer's interleaving: a newer program commits its mark and its own
/// unique index between this open's first fence and the index conversion.
#[test]
fn a_mark_committed_after_the_first_fence_stops_the_index_conversion() {
    let h = stopped();
    let left = after_the_fence(
        &h,
        format!(
            "BEGIN IMMEDIATE; UPDATE timers_extension SET version={}; DROP INDEX integration_card; CREATE UNIQUE INDEX integration_card ON cards(decision_seq); COMMIT;",
            EXTENSION + 1
        ),
    );
    refused_as_newer(&h);
    assert!(!left.borrow().is_empty(), "the other program committed");
    assert_eq!(schema(&h), *left.borrow(), "byte for byte what it left");
}

/// The same race before the common batches: a newer program that removed
/// objects they would create again.
#[test]
fn a_mark_committed_after_the_first_fence_stops_the_common_batches() {
    let h = stopped();
    let left = after_the_fence(
        &h,
        format!(
            "BEGIN IMMEDIATE; UPDATE timers_extension SET version={}; DROP TABLE verdicts; DROP INDEX teams_live; COMMIT;",
            EXTENSION + 1
        ),
    );
    refused_as_newer(&h);
    assert!(!left.borrow().is_empty(), "the other program committed");
    assert_eq!(schema(&h), *left.borrow(), "byte for byte what it left");
}

#[test]
fn a_common_object_a_store_lacks_is_created_under_the_lock() {
    let mut h = stopped();
    let full = schema(&h);
    db(&h)
        .execute_batch("DROP TABLE verdicts; DROP INDEX teams_live;")
        .unwrap();
    reopen(&mut h);
    assert_eq!(schema(&h), full);
}

/// The open looks only for the objects of `CREATE ... IF NOT EXISTS`
/// statements: every statement of the common batches must be one.
#[test]
fn every_statement_of_the_common_batches_is_seen_by_the_presence_check() {
    for sql in crate::teams::store::COMMON_SCHEMA {
        let statements = sql.matches(';').count();
        assert_eq!(crate::teams::store::created(sql).len(), statements, "{sql}");
    }
}
