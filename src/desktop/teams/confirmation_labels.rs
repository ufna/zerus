//! Names for the native dialog, resolved while the consent facts are pinned.
//! The action, revision, request and authority still come from the original.
use crate::{
    config::Config,
    teams::{
        ops::{Operation, Request},
        store,
    },
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use zerus_client_core::desktop::team_confirmation::{Labels, TaskLabel};
use zerus_team_plan::{
    model::{NodeKind, Team},
    problem::code,
    Problem,
};

type Result<T> = std::result::Result<T, Problem>;

pub(super) fn action(
    db: Option<&Connection>,
    config: &Config,
    team: Option<&Team>,
    request: &Request,
    facts: &str,
    machine: &str,
) -> Result<Labels> {
    let mut labels = Labels {
        facts: facts.to_owned(),
        ..Default::default()
    };
    let fields = serde_json::to_value(&request.operation).map_err(store::storage)?;
    let id = &request.team;
    if let Some(db) = db {
        let read_number = |value: &Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
        let mut node = read_number(&fields["node"]);
        if let Operation::Reply { item, .. } = &request.operation {
            node = db
                .query_row(
                    "SELECT node FROM inbox WHERE team=?1 AND seq=?2",
                    params![id, item],
                    |row| row.get::<_, Option<u32>>(0),
                )
                .optional()
                .map_err(store::storage)?
                .flatten();
        }
        if let Operation::DecideCard { card: seq, .. } = &request.operation {
            if let Some(card) = store::rows::card(db, id, *seq)? {
                if let Some(team) = team {
                    return card_labels(db, team, &card, facts);
                }
            }
        }
        labels.task = node.map(|n| task(db, id, n)).transpose()?.flatten();
        labels.phase = read_number(&fields["phase"])
            .or_else(|| read_number(&fields["scope"]["phase"]))
            .map(|n| phase(db, id, n))
            .transpose()?
            .flatten();
        labels.relative_phase = read_number(&fields["relative_to"]["phase"])
            .map(|n| phase(db, id, n))
            .transpose()?
            .flatten();
        if let (Some(team), Some(member)) = (team, fields["member"].as_str()) {
            labels.member = member_role(db, team, member)?;
        }
    }
    match &request.operation {
        Operation::Create {
            bundle,
            project,
            home,
            ..
        }
        | Operation::StartPlanning {
            bundle,
            project,
            home,
            ..
        } => {
            labels.template = Some(bundle.template.name.clone());
            labels.project = crate::tasks::native_team::project_name(config, project).ok();
            if home == machine {
                labels.computer = config.selves.first().cloned();
            }
        }
        _ => {}
    }
    if let Some(team) = team {
        labels.branch = Some(team.integration_branch.clone());
        let (role, pipeline) =
            placement(team, fields["role"].as_str(), fields["pipeline"].as_str());
        labels.role = role;
        labels.pipeline = pipeline;
    }
    Ok(labels)
}

pub(super) fn placement(
    team: &Team,
    role: Option<&str>,
    pipeline: Option<&str>,
) -> (Option<String>, Option<String>) {
    (
        role.and_then(|id| role_name(team, id)),
        pipeline
            .and_then(|id| {
                team.bundle
                    .pipelines
                    .iter()
                    .find(|pipeline| pipeline.id == id)
            })
            .map(|pipeline| pipeline.name.clone()),
    )
}

pub(super) fn card_labels(
    db: &Connection,
    team: &Team,
    card: &store::Card,
    facts: &str,
) -> Result<Labels> {
    let member = card
        .parameters
        .iter()
        .find_map(|(key, value)| (key == "member").then_some(value.as_str()));
    let member = member
        .map(|id| {
            let role = member_role(db, team, id)?;
            let opaque = zerus_client_core::teams::action::canonical_uuid(id);
            Ok::<_, Problem>(match (role, opaque) {
                (Some(role), false) => Some(format!("{role} ({id})")),
                (Some(role), true) => Some(role),
                (None, false) => Some(id.to_owned()),
                (None, true) => None,
            })
        })
        .transpose()?
        .flatten();
    Ok(Labels {
        facts: facts.to_owned(),
        task: card
            .node
            .map(|n| task(db, &team.id, n))
            .transpose()?
            .flatten(),
        member,
        branch: Some(team.integration_branch.clone()),
        card_kind: Some(card.kind.clone()),
        ..Default::default()
    })
}

fn role_name(team: &Team, id: &str) -> Option<String> {
    team.bundle
        .roles
        .iter()
        .find(|role| role.id == id)
        .map(|role| role.name.clone())
}

fn member_role(db: &Connection, team: &Team, id: &str) -> Result<Option<String>> {
    let role = db.query_row(
        "SELECT CASE WHEN length(CAST(data AS BLOB))<=65536 THEN json_extract(data,'$.role') END
         FROM members WHERE team=?1 AND id=?2",
        params![team.id, id],
        |row| row.get::<_, Option<String>>(0),
    ).optional().map_err(store::storage)?.flatten();
    Ok(role.as_deref().and_then(|id| role_name(team, id)))
}

fn phase(db: &Connection, team: &str, n: u32) -> Result<Option<String>> {
    match store::phase_controls::phase(db, team, n) {
        Ok(phase) => Ok(Some(phase.title)),
        Err(error) if error.code == code::NOT_FOUND => Ok(None),
        Err(error) => Err(error),
    }
}

fn task(db: &Connection, team: &str, id: u32) -> Result<Option<TaskLabel>> {
    // The same exact live/archive key as rows::node. Read only the caption
    // and kind and keep the body bounded even for a card's related node.
    let value = db
        .query_row(
            "SELECT CASE WHEN length(CAST(data AS BLOB))<=65536 THEN data END
         FROM nodes WHERE team=?1 AND id=?2 UNION ALL
         SELECT CASE WHEN length(CAST(data AS BLOB))<=65536 THEN data END
         FROM node_archive WHERE team=?1 AND id=?2 LIMIT 1",
            params![team, id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(store::storage)?
        .flatten();
    let Some(value) = value else { return Ok(None) };
    let data: Value = store::decode(&value)?;
    let Some(title) = data["title"].as_str() else {
        return Ok(None);
    };
    Ok(Some(TaskLabel {
        title: title.to_owned(),
        number: id.to_string(),
        stage: data["kind"] == serde_json::json!(NodeKind::Stage),
    }))
}
