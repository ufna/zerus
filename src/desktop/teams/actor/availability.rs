//! Point reads of the fleet rows already committed by Broker. No session cache
//! and no catalog walk: the existing (generation,key) index selects one row.
use super::*;
use crate::{config::Config, desktop::store::Location};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::time::{Duration, Instant};
const SQL:&str="SELECT r.json FROM desktop_catalog_sources s JOIN desktop_catalog_owner o ON o.id=1 AND o.epoch=s.epoch JOIN desktop_catalog_rows r ON r.generation=s.visible AND r.key=?1 WHERE s.computer='' AND s.epoch=?2 AND s.active=1 AND s.complete=1 AND json_extract(s.hello,'$.machine_id')=?3 AND r.bytes<=1048576 LIMIT 1";
pub(super) fn free(config: &Config, epoch: &str, team: &str, member: &Member) -> Result<bool> {
    let Some(session) = member.session.as_ref() else {
        return Ok(false);
    };
    let Some(run) = current_run(config, team, member)? else {
        return Ok(false);
    };
    let db = connection(config)?;
    let row: Option<String> = db
        .query_row(
            SQL,
            params![format!("run:{run}"), epoch, session.machine],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::teams::store::storage)?;
    let Some(row) = row else { return Ok(false) };
    let value: Value = serde_json::from_str(&row).map_err(crate::teams::store::storage)?;
    let identity = if let Some(id) = session.session_id.strip_prefix("launch_id:") {
        value["launch_id"] == id
    } else {
        value["run_id"] == run
    };
    Ok(identity && idle(&value))
}
/// Part C prefers the completed resumed-binding receipt here. That fact
/// chooses a run only; the fleet and task-index checks still prove it free.
pub(super) fn current_run(config: &Config, _team: &str, member: &Member) -> Result<Option<String>> {
    let Some(session) = member.session.as_ref() else {
        return Ok(None);
    };
    let run = if let Some(id) = session.session_id.strip_prefix("run_id:") {
        id.to_owned()
    } else if let Some(id) = session.session_id.strip_prefix("launch_id:") {
        match crate::state::observe_team_launch(
            &config.state,
            id,
            Instant::now() + Duration::from_millis(300),
        ) {
            crate::state::TeamObservation::Record(r)
                if r.launch_id == id && r.cwd == member.worktree && r.agent == member.agent =>
            {
                r.run_id
            }
            _ => return Ok(None),
        }
    } else {
        return Ok(None);
    };
    if run.is_empty() || run.len() > 256 {
        return Ok(None);
    }
    Ok(Some(run))
}
fn connection(config: &Config) -> Result<Connection> {
    let location = Location::new(config).map_err(|_| problem("Fleet location unavailable"))?;
    location
        .check_existing()
        .map_err(|_| problem("Fleet location is not private"))?;
    crate::desktop::store::database_files(&location)
        .map_err(|_| problem("Fleet database is not private"))?;
    let path = location.root.join("state.sqlite3");
    let identity = crate::sqlite_file::private(&path).map_err(crate::teams::store::storage)?;
    let db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(crate::teams::store::storage)?;
    crate::sqlite_file::bind_private(&db, &path, identity).map_err(crate::teams::store::storage)?;
    db.busy_timeout(Duration::from_millis(100))
        .map_err(crate::teams::store::storage)?;
    Ok(db)
}
fn idle(row: &Value) -> bool {
    let view = zerus_client_core::view::row(row);
    view.status == "ready"
        && row["state"] == "running"
        && row["activity"] == "idle"
        && row["phase"] == "idle"
        && row["process_work"]["count"].as_i64() == Some(0)
        && row["subagents_active"].as_i64() == Some(0)
        && row["optional_questions"]["count"].as_i64() == Some(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row() -> Value {
        json!({"name":"codex/example/member","agent":"codex","cmd":"codex","state":"running","process_state":"running","activity":"idle","phase":"idle","run_id":"run","launch_id":"launch","conversation_id":"conversation","conversation_state":"active","process_work":{"count":0},"subagents_active":0,"optional_questions":{"count":0}})
    }
    #[test]
    fn idle_is_required_and_unknown_or_pending_or_background_is_not_free() {
        assert!(idle(&row()));
        for (path, value) in [
            ("activity", json!("busy")),
            ("phase", json!("approval")),
            ("state", json!("stopped")),
            ("process_work", json!({"count":1})),
            ("optional_questions", json!({"count":1})),
            ("subagents_active", json!(1)),
            ("process_work", Value::Null),
        ] {
            let mut raw = row();
            raw[path] = value;
            assert!(!idle(&raw), "{path}");
        }
    }
    #[test]
    fn fleet_query_is_indexed_by_generation_and_key_and_requires_the_current_complete_source() {
        let db = Connection::open_in_memory().unwrap();
        zerus_desktop_catalog::migrate(&db).unwrap();
        let mut s = db.prepare(&format!("EXPLAIN QUERY PLAN {SQL}")).unwrap();
        let details = s
            .query_map(params!["run:run", "epoch", "machine"], |r| {
                r.get::<_, String>(3)
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            details.iter().any(|r| r.contains("generation=? AND key=?")),
            "{details:?}"
        );
        assert!(db
            .query_row(SQL, params!["run:run", "epoch", "machine"], |r| r
                .get::<_, String>(0))
            .optional()
            .unwrap()
            .is_none());
    }
}
#[cfg(test)]
mod record_tests {
    use super::*;
    use serde_json::json;
    fn config(temp: &Path) -> Config {
        Config {
            home: String::new(),
            dir: temp.join("config"),
            state: temp.join("state"),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        }
    }
    #[test]
    fn a_current_complete_fleet_record_is_required_and_the_read_changes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let c = config(temp.path());
        crate::platform::private_dir(&c.dir).unwrap();
        crate::platform::private_dir(&c.state.join("desktop")).unwrap();
        let path = c.state.join("desktop/state.sqlite3");
        crate::sqlite_file::create(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        zerus_desktop_catalog::migrate(&db).unwrap();
        db.execute(
            "UPDATE desktop_catalog_owner SET epoch='epoch' WHERE id=1",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO desktop_catalog_sources(computer,epoch,incarnation,generation,stream,seq,visible,complete,active,hello) VALUES('','epoch',1,1,'stream',1,1,1,1,?1)",[json!({"machine_id":"machine"}).to_string()]).unwrap();
        db.execute(
            "INSERT INTO desktop_catalog_generations(id,computer) VALUES(1,'')",
            [],
        )
        .unwrap();
        let row=json!({"name":"codex/example/member","agent":"codex","cmd":"codex","state":"running","activity":"idle","phase":"idle","run_id":"run","conversation_id":"conversation","conversation_state":"active","process_work":{"count":0},"subagents_active":0,"optional_questions":{"count":0}}).to_string();
        db.execute(
            "INSERT INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,'run:run',?1,?2)",
            params![row, row.len()],
        )
        .unwrap();
        let m = Member {
            id: "m1".into(),
            role: "developer".into(),
            agent: "codex".into(),
            account: String::new(),
            state: MemberState::Active,
            session: Some(SessionKey {
                machine: "machine".into(),
                session_id: "run_id:run".into(),
            }),
            worktree: String::new(),
            ownership_nonce: String::new(),
        };
        let before = std::fs::read(&path).unwrap();
        assert!(free(&c, "epoch", "team", &m).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!free(&c, "old-epoch", "team", &m).unwrap());
        for field in ["active", "complete"] {
            db.execute(&format!("UPDATE desktop_catalog_sources SET {field}=0"), [])
                .unwrap();
            assert!(!free(&c, "epoch", "team", &m).unwrap());
            db.execute(&format!("UPDATE desktop_catalog_sources SET {field}=1"), [])
                .unwrap();
        }
        db.execute("DELETE FROM desktop_catalog_rows", []).unwrap();
        assert!(!free(&c, "epoch", "team", &m).unwrap());
    }
    #[test]
    fn an_ended_launch_without_a_resumed_binding_has_no_current_run() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let c = config(temp.path());
        crate::platform::private_dir(&c.state.join("launches")).unwrap();
        let launch = "00000000-0000-4000-8000-000000000001";
        for (leaf, value) in [
            (
                format!("{launch}.json"),
                json!({"launch_id":launch,"name":"kimi/example/member","started_at":1.0}),
            ),
            (
                format!("{launch}.end.json"),
                json!({"launch_id":launch,"name":"kimi/example/member","code":0,"ended_at":2.0}),
            ),
        ] {
            let path = c.state.join("launches").join(leaf);
            std::fs::write(&path, value.to_string()).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let m = Member {
            id: "m1".into(),
            role: "developer".into(),
            agent: "kimi".into(),
            account: String::new(),
            state: MemberState::Active,
            session: Some(SessionKey {
                machine: "machine".into(),
                session_id: format!("launch_id:{launch}"),
            }),
            worktree: String::new(),
            ownership_nonce: String::new(),
        };
        assert_eq!(current_run(&c, "team", &m).unwrap(), None);
        assert!(!free(&c, "epoch", "team", &m).unwrap());
    }
}
