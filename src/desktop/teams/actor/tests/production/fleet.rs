//! Committed fleet and launch evidence; no native agent or desktop is started.
use super::*;
use rusqlite::params;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
fn write(path: &Path, value: Value) {
    std::fs::write(path, value.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
pub(super) fn seed(h: &Harness, n: &native::Native) {
    std::fs::set_permissions(h.temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    for d in [
        n.config.dir.join("swarm"),
        n.config.state.join("desktop"),
        n.config.state.join("launches"),
        n.config.state.join("sessions"),
    ] {
        crate::platform::private_dir(&d).unwrap();
    }
    let path = n.config.state.join("desktop/state.sqlite3");
    if path.exists() {
        return;
    }
    write(
        &n.config.dir.join("swarm/catalog.json"),
        json!({"node_id":n.machine,"imported":true,"catalog":{"schema":1,"swarm_id":uuid::Uuid::new_v4().to_string(),"operations":{}}}),
    );
    crate::sqlite_file::create(&path).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    zerus_desktop_catalog::migrate(&db).unwrap();
    db.execute(
        "UPDATE desktop_catalog_owner SET epoch=?1 WHERE id=1",
        [&n.epoch],
    )
    .unwrap();
    db.execute(
        "INSERT OR REPLACE INTO desktop_catalog_generations(id,computer) VALUES(1,'')",
        [],
    )
    .unwrap();
    db.execute("INSERT OR REPLACE INTO desktop_catalog_sources(computer,epoch,incarnation,generation,stream,seq,visible,complete,active,hello) VALUES('',?1,1,1,'stream',1,1,1,1,?2)",params![n.epoch,json!({"machine_id":n.machine}).to_string()]).unwrap();
    for m in h.store.members(TEAM, 200, None).unwrap().rows {
        seed_member(n, &db, &m);
    }
}
fn seed_member(n: &native::Native, db: &rusqlite::Connection, m: &Member) {
    let Some(binding) = &m.session else { return };
    let Some(launch) = binding.session_id.strip_prefix("launch_id:") else {
        return;
    };
    let sid = uuid::Uuid::new_v4().to_string();
    let run = format!("fixture-{}", m.id);
    let name = format!("{}/fixture/{}", m.agent, m.id);
    let record = json!({"version":1,"layout":"id","session_id":sid,"name":name,"launch_id":launch,"agent":m.agent,"account_id":m.account,"cwd":m.worktree,"run_id":run,"conversation_id":format!("conversation-{}",m.id)});
    write(
        &n.config.state.join("sessions").join(format!("{sid}.json")),
        record,
    );
    write(
        &n.config
            .state
            .join("launches")
            .join(format!("{launch}.json")),
        json!({"launch_id":launch,"session_id":sid,"name":name,"started_at":1.0}),
    );
    let row=json!({"name":name,"agent":m.agent,"cmd":m.agent,"state":"running","process_state":"running","activity":"idle","phase":"idle","run_id":run,"launch_id":launch,"conversation_id":format!("conversation-{}",m.id),"conversation_state":"active","process_work":{"count":0},"subagents_active":0,"optional_questions":{"count":0}}).to_string();
    db.execute(
        "INSERT OR REPLACE INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,?1,?2,?3)",
        params![format!("run:{run}"), row, row.len()],
    )
    .unwrap();
}

pub(super) fn change(n: &native::Native, mode: &str) {
    let db = rusqlite::Connection::open(n.config.state.join("desktop/state.sqlite3")).unwrap();
    match mode {
        "fleet unknown" => {
            db.execute("DELETE FROM desktop_catalog_rows", []).unwrap();
        }
        "fleet epoch" => {
            db.execute(
                "UPDATE desktop_catalog_owner SET epoch='other' WHERE id=1",
                [],
            )
            .unwrap();
        }
        _ => {
            let (path, value) = match mode {
                "fleet busy" => ("$.activity", json!("busy")),
                "fleet pending" => ("$.optional_questions.count", json!(1)),
                "fleet background" => ("$.process_work.count", json!(1)),
                "fleet run" => ("$.run_id", json!("other-run")),
                _ => panic!("unknown fleet mutation"),
            };
            db.execute(
                "UPDATE desktop_catalog_rows SET json=json_set(json,?1,json(?2))",
                params![path, value.to_string()],
            )
            .unwrap();
        }
    }
}
