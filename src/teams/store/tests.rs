use super::*;

#[test]
fn missing_store_is_lazy_and_preview_is_required() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(Store::open(temp.path()).unwrap_err().code, "teams_inactive");
    assert!(!temp.path().join("teams").exists());
    std::fs::create_dir(temp.path().join("teams")).unwrap();
    std::fs::write(temp.path().join("teams/preview"), "on").unwrap();
    let store = Store::open(temp.path()).unwrap();
    assert!(store.team("missing").unwrap().is_none());
    assert!(!temp.path().join("teams/teams-v1.sqlite3").exists());
}

use crate::teams::ops::{
    self,
    tests::{create, person, refused, request, route, task, Harness, TEAM},
    Operation,
};

#[test]
fn empty_published_store_recovers_schema_initialization() {
    let mut h = Harness::empty();
    let path = h.temp.path().join("teams/teams-v1.sqlite3");
    #[cfg(unix)]
    crate::sqlite_file::create(&path).unwrap();
    #[cfg(windows)]
    drop(
        crate::platform::private_open(std::fs::OpenOptions::new().create_new(true).write(true))
            .open(&path)
            .unwrap(),
    );
    h.store = Store::open(h.temp.path()).unwrap();
    h.run(&person(), create()).unwrap();
    assert_eq!(h.store.team(TEAM).unwrap().unwrap().revision, 1);
}
#[test]
fn newer_schema_is_refused_without_modifying_its_version() {
    let h = Harness::new();
    h.store
        .db
        .as_ref()
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    refused(Store::open(h.temp.path()), code::INVALID_STATE);
    let version: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION + 1);
}
#[test]
fn version_one_store_migrates_without_losing_the_team() {
    let mut h = Harness::empty();
    let path = h.temp.path().join("teams/teams-v1.sqlite3");
    #[cfg(unix)]
    crate::sqlite_file::create(&path).unwrap();
    #[cfg(windows)]
    drop(
        crate::platform::private_open(std::fs::OpenOptions::new().create_new(true).write(true))
            .open(&path)
            .unwrap(),
    );
    let db = Connection::open(&path).unwrap();
    db.execute_batch(include_str!("schema-v1.sql")).unwrap();
    let t = ops::tests::fixture().team;
    db.execute("INSERT INTO teams(id,project,name,goal,home,owner,status,lead,settings,bundle,integration_branch,base_commit,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",rusqlite::params![t.id,t.project,t.name,t.goal,t.home,t.owner,encode(&t.status).unwrap(),t.lead,encode(&t.settings).unwrap(),encode(&t.bundle).unwrap(),t.integration_branch,t.base_commit,t.revision]).unwrap();
    drop(db);
    h.store = Store::open(h.temp.path()).unwrap();
    assert_eq!(h.store.team(TEAM).unwrap(), Some(t));
    assert_eq!(
        h.store
            .db
            .as_ref()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
    h.run(&person(), Operation::Stop).unwrap();
}
#[test]
fn an_empty_handle_can_refresh_after_another_writer_creates_a_team() {
    let mut h = Harness::empty();
    let mut writer = Store::open(h.temp.path()).unwrap();
    ops::apply(&mut writer, &person(), &request(0, create()), 1).unwrap();
    h.store.refresh().unwrap();
    assert!(h.store.team(TEAM).unwrap().is_some());
}
#[test]
fn queue_and_journal_pages_exclude_additions_after_the_first_page() {
    let mut h = Harness::new();
    for n in 1..=5 {
        h.add(&person(), n);
    }
    let first = h.store.queue(TEAM, 2, None).unwrap();
    let journal = h.store.journal(TEAM, 2, None).unwrap();
    let old_count = h.store.journal(TEAM, 200, None).unwrap().rows.len();
    h.add(&person(), 6);
    let mut ids = first.rows.iter().map(|n| n.id).collect::<Vec<_>>();
    let mut cursor = first.next;
    while let Some(c) = cursor {
        let p = h.store.queue(TEAM, 2, Some(&c)).unwrap();
        ids.extend(p.rows.iter().map(|n| n.id));
        cursor = p.next;
    }
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    let mut count = journal.rows.len();
    let mut cursor = journal.next;
    while let Some(c) = cursor {
        let p = h.store.journal(TEAM, 2, Some(&c)).unwrap();
        count += p.rows.len();
        cursor = p.next;
    }
    assert_eq!(count, old_count);
}
#[test]
fn node_rows_and_dependencies_share_one_snapshot_during_a_concurrent_route() {
    let mut h = Harness::new();
    for n in 1..=3 {
        h.add(&person(), n);
    }
    let mut old = route(1);
    if let Operation::Route { deps, writes, .. } = &mut old {
        deps.push(Dep {
            node: 2,
            kind: DepKind::Hard,
        });
        writes.push("old.rs".into());
    }
    h.run(&person(), old).unwrap();
    let mut writer = Store::open(h.temp.path()).unwrap();
    let p = h
        .store
        .node_page_with(TEAM, None, 200, None, || {
            let mut new = route(1);
            if let Operation::Route { deps, writes, .. } = &mut new {
                deps.push(Dep {
                    node: 3,
                    kind: DepKind::Hard,
                });
                writes.push("new.rs".into());
            }
            let rev = writer.team(TEAM).unwrap().unwrap().revision;
            ops::apply(&mut writer, &person(), &request(rev, new), 1).unwrap();
        })
        .unwrap();
    assert_eq!(p.rows[0].writes, vec!["old.rs"]);
    assert_eq!(p.rows[0].deps[0].node, 2);
    assert_eq!(h.node(1).writes, vec!["new.rs"]);
    assert_eq!(h.node(1).deps[0].node, 3);
}
#[test]
fn every_list_rejects_over_limit_and_foreign_cursors() {
    let mut h = Harness::new();
    h.add(&person(), 1);
    refused(h.store.queue(TEAM, 201, None), code::OVER_CAP);
    refused(h.store.members(TEAM, 0, None), code::OVER_CAP);
    let c = h.store.queue(TEAM, 1, None).unwrap().next.unwrap();
    refused(h.store.cards(TEAM, 1, Some(&c)), code::INVALID_STATE);
    refused(h.store.inbox(TEAM, 201, None), code::OVER_CAP);
    refused(h.store.effects(TEAM, 201, None), code::OVER_CAP);
    refused(h.store.journal(TEAM, 201, None), code::OVER_CAP);
}
#[test]
fn a_thousand_decided_nodes_leave_the_live_page_unchanged() {
    let mut h = Harness::new();
    h.add(&person(), 1);
    let before = h.store.queue(TEAM, 200, None).unwrap();
    for n in 2..=1001 {
        let node = h.add(&person(), n);
        h.cancel_fact(node);
    }
    assert_eq!(h.store.queue(TEAM, 200, None).unwrap(), before);
    assert_eq!(
        h.store
            .nodes_by_state(TEAM, NodeState::Cancelled, 200, None)
            .unwrap()
            .rows
            .len(),
        200
    );
    h.run(
        &person(),
        Operation::AddTask {
            task: task(1002),
            role: None,
            pipeline: None,
        },
    )
    .unwrap();
}
#[test]
fn journal_is_capped_and_archived_rows_keep_sequence_order() {
    let mut h = Harness::new();
    let tx = h.store.db.as_mut().unwrap().transaction().unwrap();
    for n in 0..=LIVE_JOURNAL_CAP {
        tx.execute("INSERT INTO journal(team,at_ms,actor,kind,request_id,text) VALUES(?1,0,?2,'fixture',?3,'Example')",rusqlite::params![TEAM,encode(&person()).unwrap(),format!("fixture-{n}")]).unwrap();
    }
    tx.commit().unwrap();
    h.run(&person(), Operation::Pause).unwrap();
    let count: u64 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM journal WHERE team=?1", [TEAM], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, LIVE_JOURNAL_CAP);
    assert_eq!(
        h.store
            .journal(TEAM, 3, None)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}
#[test]
fn late_observations_keep_archived_journals_archived_and_record_node_references() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    h.run(&person(), Operation::Stop).unwrap();
    h.run(&person(), Operation::Archive).unwrap();
    h.cancel_fact(node);
    let count: u32 = h
        .store
        .db
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM journal WHERE team=?1", [TEAM], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    let rows = h.store.journal(TEAM, 200, None).unwrap().rows;
    assert!(rows.last().unwrap().refs.contains(&format!("node:{node}")));
}
#[cfg(unix)]
#[test]
fn opening_another_connection_keeps_sqlite_locks_and_refuses_symlinks() {
    let h = Harness::new();
    let path = h.temp.path().join("teams/teams-v1.sqlite3");
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE;")
        .unwrap();
    assert!(crate::sqlite_file::foreign_lock_refused(&path));
    let reopened = Store::open(h.temp.path()).unwrap();
    assert!(crate::sqlite_file::foreign_lock_refused(&path));
    drop(reopened);
    h.store
        .db
        .as_ref()
        .unwrap()
        .execute_batch("ROLLBACK;")
        .unwrap();
    drop(h.store);
    let moved = h.temp.path().join("teams/saved.sqlite3");
    std::fs::rename(&path, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &path).unwrap();
    refused(Store::open(h.temp.path()), code::INVALID_STATE);
}
