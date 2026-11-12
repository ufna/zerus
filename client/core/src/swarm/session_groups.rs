//! Named session groups belong to one project. Group edits never affect
//! folders, tasks or native agents; removal only clears group membership.
use super::{edit, members, project_id, projects};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_GROUPS: usize = 200;
pub const GONE: &str = "This session group is no longer listed. Open the menu again.";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Create {
        id: String,
        name: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
    },
    Reorder {
        id: String,
        before: Option<String>,
    },
    Assign {
        group: Option<String>,
        members: Vec<String>,
    },
    Move {
        group: Option<String>,
        members: Vec<String>,
        before: Option<String>,
    },
}

pub fn collapse_key(project: &str, group: &str) -> String {
    format!("session-group:{project}:{group}")
}

pub fn apply(organization: &mut Value, project: &str, change: &Change) -> Result<(), String> {
    let at = projects(organization)
        .iter()
        .position(|p| project_id(p) == project)
        .ok_or(edit::GONE)?;
    if let Change::Assign {
        group,
        members: selected,
    }
    | Change::Move {
        group,
        members: selected,
        ..
    } = change
    {
        if selected.is_empty() {
            return Err(edit::NOTHING.into());
        }
        if group.as_ref().is_some_and(|id| {
            !organization["projects"][at]["groups"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|g| g["id"] == *id)
        }) {
            return Err(GONE.into());
        }
        let membership = members(organization);
        if matches!(change, Change::Assign { .. })
            && selected
                .iter()
                .any(|member| membership.get(member).is_some_and(|p| p != project))
        {
            return Err("Choose sessions from this project only.".into());
        }
        *organization = edit::apply(
            organization,
            &edit::Intent::Move {
                project: project.into(),
                members: selected.clone(),
                before: match change {
                    Change::Move { before, .. } => before.clone(),
                    _ => None,
                },
            },
        )?;
        let p = &mut organization["projects"][at];
        if !p["session_groups"].is_object() {
            p["session_groups"] = json!({});
        }
        let assignments = p["session_groups"].as_object_mut().unwrap();
        for member in selected {
            if let Some(group) = group {
                assignments.insert(member.clone(), json!(group));
            } else {
                assignments.remove(member);
            }
        }
        return Ok(());
    }
    let p = &mut organization["projects"][at];
    if !p["groups"].is_array() {
        p["groups"] = json!([]);
    }
    let groups = p["groups"].as_array_mut().unwrap();
    match change {
        Change::Create { id, name } => {
            let name = name
                .trim()
                .chars()
                .take(edit::NAME_CHARS)
                .collect::<String>();
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err("Enter a group name.".into());
            }
            if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
                return Err("Invalid session group identity.".into());
            }
            if groups.iter().any(|g| g["id"] == *id) {
                return Err("This session group identity is already listed.".into());
            }
            if groups.len() >= MAX_GROUPS {
                return Err("A project holds at most 200 session groups.".into());
            }
            groups.push(json!({"id": id, "name": name}));
        }
        Change::Rename { id, name } => {
            let name = name
                .trim()
                .chars()
                .take(edit::NAME_CHARS)
                .collect::<String>();
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err("Enter a group name.".into());
            }
            let group = groups.iter_mut().find(|g| g["id"] == *id).ok_or(GONE)?;
            group["name"] = json!(name);
        }
        Change::Delete { id } => {
            if !groups.iter().any(|g| g["id"] == *id) {
                return Err(GONE.into());
            }
            groups.retain(|g| g["id"] != *id);
            if !p["session_groups"].is_object() {
                p["session_groups"] = json!({});
            }
            p["session_groups"]
                .as_object_mut()
                .unwrap()
                .retain(|_, group| group != id);
        }
        Change::Reorder { id, before } => {
            let from = groups.iter().position(|g| g["id"] == *id).ok_or(GONE)?;
            if before.as_deref() == Some(id) {
                return Ok(());
            }
            if before
                .as_ref()
                .is_some_and(|anchor| !groups.iter().any(|g| g["id"] == *anchor))
            {
                return Err(GONE.into());
            }
            let group = groups.remove(from);
            let to = before
                .as_ref()
                .and_then(|id| groups.iter().position(|g| g["id"] == *id))
                .unwrap_or(groups.len());
            groups.insert(to, group);
        }
        Change::Assign { .. } | Change::Move { .. } => unreachable!(),
    }
    if matches!(change, Change::Reorder { .. }) || p.get("group_order").is_some() {
        p["group_order"] = json!(p["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["id"].clone())
            .collect::<Vec<_>>());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const MEMBER: &str = "workstation\ncodex/sample/session";
    #[test]
    fn manual_order_survives_rename_create_and_delete_without_moving_sessions() {
        let mut value = grouped();
        for id in ["inventory", "mutants"] {
            apply(
                &mut value,
                "alpha",
                &Change::Create {
                    id: id.into(),
                    name: id.into(),
                },
            )
            .unwrap();
        }
        apply(
            &mut value,
            "alpha",
            &Change::Reorder {
                id: "mutants".into(),
                before: Some("ui".into()),
            },
        )
        .unwrap();
        assert_eq!(
            value["projects"][0]["group_order"],
            json!(["mutants", "ui", "inventory"])
        );
        assert_eq!(value["projects"][0]["sessions"], json!([MEMBER]));
        assert_eq!(value["projects"][0]["session_groups"][MEMBER], "ui");
        apply(
            &mut value,
            "alpha",
            &Change::Rename {
                id: "mutants".into(),
                name: "AAA".into(),
            },
        )
        .unwrap();
        apply(
            &mut value,
            "alpha",
            &Change::Create {
                id: "new".into(),
                name: "New".into(),
            },
        )
        .unwrap();
        assert_eq!(
            value["projects"][0]["group_order"],
            json!(["mutants", "ui", "inventory", "new"])
        );
        apply(
            &mut value,
            "alpha",
            &Change::Delete {
                id: "inventory".into(),
            },
        )
        .unwrap();
        assert_eq!(
            value["projects"][0]["group_order"],
            json!(["mutants", "ui", "new"])
        );
        let before = value.clone();
        assert!(apply(
            &mut value,
            "alpha",
            &Change::Reorder {
                id: "ui".into(),
                before: Some("".into())
            }
        )
        .is_err());
        assert_eq!(value, before);
    }
    fn org() -> Value {
        json!({"projects":[{"id":"alpha","sessions":[MEMBER]}, {"id":"beta","sessions":[]}]})
    }
    fn grouped() -> Value {
        let mut value = org();
        apply(
            &mut value,
            "alpha",
            &Change::Create {
                id: "ui".into(),
                name: "UI".into(),
            },
        )
        .unwrap();
        apply(
            &mut value,
            "alpha",
            &Change::Assign {
                group: Some("ui".into()),
                members: vec![MEMBER.into()],
            },
        )
        .unwrap();
        value
    }
    #[test]
    fn groups_are_shared_edits_and_deleting_keeps_sessions() {
        let mut value = grouped();
        assert!(edit::changes_shared(&org(), &value));
        apply(
            &mut value,
            "alpha",
            &Change::Rename {
                id: "ui".into(),
                name: " Interface ".into(),
            },
        )
        .unwrap();
        assert_eq!(value["projects"][0]["groups"][0]["name"], "Interface");
        apply(&mut value, "alpha", &Change::Delete { id: "ui".into() }).unwrap();
        assert_eq!(value["projects"][0]["sessions"], json!([MEMBER]));
        assert_eq!(value["projects"][0]["session_groups"], json!({}));
    }
    #[test]
    fn moving_to_another_project_clears_group_but_sort_and_rename_keep_it() {
        let value = grouped();
        let sorted = edit::apply(
            &value,
            &edit::Intent::Sort {
                project: "alpha".into(),
                members: vec![MEMBER.into()],
            },
        )
        .unwrap();
        assert_eq!(sorted, value);
        let renamed = edit::apply(
            &value,
            &edit::Intent::Renamed {
                from: MEMBER.into(),
                to: "workstation\ncodex/sample/new".into(),
            },
        )
        .unwrap();
        assert_eq!(
            renamed["projects"][0]["session_groups"]["workstation\ncodex/sample/new"],
            "ui"
        );
        assert!(renamed["projects"][0]["session_groups"]
            .get(MEMBER)
            .is_none());
        let moved = edit::apply(
            &value,
            &edit::Intent::Move {
                project: "beta".into(),
                members: vec![MEMBER.into()],
                before: None,
            },
        )
        .unwrap();
        assert_eq!(moved["projects"][0]["session_groups"], json!({}));
    }
    #[test]
    fn unknown_groups_and_cross_project_members_are_refused() {
        let mut value = grouped();
        let before = value.clone();
        for (project, group) in [("alpha", "missing"), ("beta", "ui")] {
            assert!(apply(
                &mut value,
                project,
                &Change::Assign {
                    group: Some(group.into()),
                    members: vec![MEMBER.into()]
                }
            )
            .is_err());
            assert_eq!(value, before);
        }
    }
}
