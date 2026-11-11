use super::*;
use rusqlite::params;

pub(crate) fn team(db: &Connection, id: &str) -> Result<Option<Team>> {
    db.query_row("SELECT id,project,name,goal,home,owner,status,lead,settings,bundle,integration_branch,base_commit,revision FROM teams WHERE id=?1", [id], |r| Ok(Team {
        id:r.get(0)?,project:r.get(1)?,name:r.get(2)?,goal:r.get(3)?,home:r.get(4)?,owner:r.get(5)?,
        status:sql_json(r,6)?,lead:r.get(7)?,settings:sql_json(r,8)?,bundle:sql_json(r,9)?,
        integration_branch:r.get(10)?,base_commit:r.get(11)?,revision:r.get(12)?,
    })).optional().map_err(storage)
}
pub(crate) fn member(db: &Connection, team: &str, id: &str) -> Result<Option<Member>> {
    db.query_row(
        "SELECT data FROM members WHERE team=?1 AND id=?2",
        params![team, id],
        |r| sql_json(r, 0),
    )
    .optional()
    .map_err(storage)
}
pub(crate) const NODE_COLUMNS: &str = "team,id,data,rank,admitted,routed,hold,task_revision,task_stage,task_cancelled,generation,decided_by,integrated,integration_commit,requested,integration_required";

pub(crate) fn node_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let mut node: Node = sql_json(r, 0)?;
    node.rank = r.get(1)?;
    node.admitted = r.get(2)?;
    node.hold = r.get(3)?;
    let state: String = r.get(4)?;
    node.state = serde_json::from_value(serde_json::Value::String(state)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    node.decided_by = match r.get::<_, Option<String>>(5)? {
        Some(s) => Some(serde_json::from_str(&s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?),
        None => None,
    };
    node.integrated = r.get(6)?;
    Ok(node)
}
pub(crate) const NODE_SELECT: &str = "data,rank,admitted,hold,node_state,decided_by,integrated";

pub(crate) fn deps(db: &Connection, team: &str, id: u32) -> Result<Vec<Dep>> {
    let mut statement=db.prepare("SELECT dependency,kind FROM dependencies WHERE team=?1 AND node=?2 ORDER BY dependency LIMIT 201").map_err(storage)?;
    let deps = statement
        .query_map(params![team, id], |r| {
            Ok(Dep {
                node: r.get(0)?,
                kind: sql_json(r, 1)?,
            })
        })
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if deps.len() > 200 {
        return Err(storage("too many node dependencies"));
    }
    Ok(deps)
}
pub(crate) fn node(db: &Connection, team: &str, id: u32) -> Result<Option<Node>> {
    let sql=format!("SELECT {NODE_SELECT} FROM nodes WHERE team=?1 AND id=?2 UNION ALL SELECT {NODE_SELECT} FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1");
    let mut node = db
        .query_row(&sql, params![team, id], node_row)
        .optional()
        .map_err(storage)?;
    if let Some(node) = node.as_mut() {
        node.deps = deps(db, team, id)?;
    }
    Ok(node)
}

/// The payload holds only node metadata. State, task stage, admission and
/// queue order have independent columns; no serialized NodeState is persisted.
pub(crate) fn node_payload(node: &Node) -> Result<String> {
    let mut value = serde_json::to_value(node).map_err(storage)?;
    if let Some(fields) = value.as_object_mut() {
        for name in [
            "state",
            "rank",
            "admitted",
            "hold",
            "decided_by",
            "integrated",
            "deps",
        ] {
            fields.remove(name);
        }
    }
    encode(&value)
}

pub(crate) fn archive_decided(db: &Connection, team: &str, actor: &str) -> Result<()> {
    let ids = {
        let mut statement=db.prepare("SELECT id FROM nodes WHERE team=?1 AND node_state IN ('accepted','cancelled') ORDER BY id LIMIT 601").map_err(storage)?;
        let mapped = statement
            .query_map([team], |r| r.get::<_, u32>(0))
            .map_err(storage)?;
        mapped
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?
    };
    if ids.len() > 600 {
        return Err(storage("Live node count exceeds its bound."));
    }
    for node in ids {
        settle_decisions(db, team, Some(node), actor, false)?;
    }
    db.execute(
        "UPDATE nodes SET requested=NULL WHERE team=?1 AND node_state IN ('accepted','cancelled')",
        [team],
    )
    .map_err(storage)?;
    db.execute(&format!("INSERT INTO node_archive({NODE_COLUMNS}) SELECT {NODE_COLUMNS} FROM nodes WHERE team=?1 AND node_state IN ('accepted','cancelled')"),[team]).map_err(storage)?;
    db.execute(
        "DELETE FROM nodes WHERE team=?1 AND node_state IN ('accepted','cancelled')",
        [team],
    )
    .map_err(storage)?;
    Ok(())
}

pub(crate) fn settle_decisions(
    db: &Connection,
    team: &str,
    node: Option<u32>,
    actor: &str,
    by_stop: bool,
) -> Result<()> {
    db.execute(&format!("UPDATE decisions SET stopped_ready=(?4 AND kind='accept' AND state=?5),state=?2 WHERE team=?1 AND state<>'{}' AND (?3 IS NULL OR node=?3)",DecisionState::Settled.as_str()),params![team,DecisionState::Settled,node,by_stop,DecisionState::Ready]).map_err(storage)?;
    db.execute("UPDATE cards SET state='closed',decision='stale',decided_actor=?3 WHERE team=?1 AND kind='integration' AND state='open' AND (?2 IS NULL OR node=?2)",params![team,node,actor]).map_err(storage)?;
    Ok(())
}
pub(crate) fn archived_leaf(db: &Connection, team: &str, id: u32) -> Result<Option<Node>> {
    db.query_row(
        &format!("SELECT {NODE_SELECT} FROM node_archive WHERE team=?1 AND id=?2"),
        params![team, id],
        node_row,
    )
    .optional()
    .map_err(storage)
}
