//! Resolve task links from one saved-binding snapshot per command. Labels never
//! establish identity once a task has an immutable session_id.
use super::*;

/// A list resolves just the references in its selected rows. Remote hints
/// need no local records; an unchanged/empty page needs none at all. This
/// never supplies a write command's identity or actor proof.
pub(super) fn for_rows(config: &Config, machines: &mut Machines, rows: &[Value]) -> Result<()> {
    let mut wanted = std::collections::BTreeSet::new();
    for row in rows {
        for reference in [
            &row["target"],
            &row["session"],
            &row["author"],
            &row["target"]["parent"],
            &row["worker"]["parent"],
            &row["execution"]["current"]["executor"],
        ] {
            if let Ok(key) = serde_json::from_value::<SessionKey>(reference.clone()) {
                if key.machine == machines.here {
                    wanted.insert((key.session, key.session_id));
                }
            }
        }
    }
    if !wanted.is_empty() {
        machines.bindings =
            crate::state::task_binding_rows(&config.state, &wanted).map_err(fail)?;
    }
    Ok(())
}

pub(super) fn identity(row: &Value) -> String {
    for key in ["launch_id", "run_id"] {
        if let Some(id) = row[key].as_str().filter(|id| !id.is_empty()) {
            return format!("{key}:{id}");
        }
    }
    String::new()
}

impl Machines {
    pub(super) fn target(
        &self,
        machine: String,
        name: &str,
        args: &[String],
    ) -> Result<SessionKey> {
        let key = SessionKey {
            machine,
            session: name.into(),
            session_id: option(args, "--session-id")?.unwrap_or_default().into(),
        };
        if key.session_id.len() > 200 || key.session_id.chars().any(char::is_control) {
            return Err(fail("invalid task session identity"));
        }
        self.resolve_key(&key, true)
            .ok_or_else(|| fail("the selected session identity is no longer available"))
    }
    /// Unknown remote bindings remain hints for that computer to resolve. A
    /// known local ID with no unique binding refuses rather than using its name.
    pub(super) fn resolve(&self, key: &SessionKey) -> Option<SessionKey> {
        self.resolve_key(key, false)
    }

    fn resolve_key(&self, key: &SessionKey, selecting_current: bool) -> Option<SessionKey> {
        if key.machine != self.here {
            return Some(key.clone());
        }
        let by_name = key.session_id.is_empty();
        let current_name = |row: &Value| row["name"].as_str() == Some(&key.session);
        // An explicit request may select the current holder of a label. An
        // existing name-only reference has no such evidence of intent. A
        // current holder and a different historical holder make it ambiguous.
        let current_only = by_name && selecting_current && self.bindings.iter().any(current_name);
        let matches = |row: &&Value| {
            if !by_name {
                identity(row) == key.session_id
            } else {
                current_name(row)
                    || (!current_only
                        && (row["journal_name"].as_str() == Some(&key.session)
                            || row["launch_label"].as_str() == Some(&key.session)
                            || row["legacy_shadow"]["name"].as_str() == Some(&key.session)))
            }
        };
        let mut found = self.bindings.iter().filter(matches);
        if let Some(row) = found.next() {
            if found.next().is_some() {
                return None;
            }
            return Some(SessionKey {
                machine: key.machine.clone(),
                session: row["name"].as_str()?.into(),
                session_id: identity(row),
            });
        }
        by_name.then(|| key.clone()) // existing name-only tasks remain readable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(root: &std::path::Path) -> Config {
        Config {
            home: root.to_string_lossy().into_owned(),
            dir: root.join("config"),
            state: root.to_path_buf(),
            selves: vec!["local".into()],
            peers: Vec::new(),
            base_peers: Vec::new(),
            peers_from_environment: false,
            machines: BTreeMap::new(),
            tab: false,
            tab_colors: String::new(),
        }
    }

    fn machines(bindings: Vec<Value>) -> Machines {
        Machines {
            here: "local-node".into(),
            selves: vec!["local".into()],
            peers: BTreeMap::from([("remote".into(), "remote-node".into())]),
            bindings,
        }
    }

    #[test]
    fn selected_reference_kinds_keep_full_inventory_resolution_and_ambiguity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for n in 0..12 {
            let mut record = json!({"name":format!("codex/current-{n}"),
                "run_id":format!("run-{n}"),"launch_id":format!("launch-{n}"),
                "launch_label":format!("codex/old-{n}")});
            if n >= 9 {
                record["launch_id"] = json!("ambiguous");
                record["launch_label"] = json!("codex/shared");
            }
            std::fs::write(root.join(format!("{n:064x}.json")), record.to_string()).unwrap();
        }
        let key = |n: usize| {
            json!({"machine":"local-node", "session":format!("codex/old-{n}"),
            "session_id":format!("launch_id:launch-{n}")})
        };
        let mut rows = vec![
            json!({"target":key(0)}),
            json!({"session":key(1)}),
            json!({"author":key(2)}),
            json!({"target":{"kind":"new_session","parent":key(3)}}),
            json!({"worker":{"parent":key(4)}}),
            json!({"execution":{"current":{"executor":key(5)}}}),
            // Legacy alias, exact current name, missing ID, ambiguous ID,
            // ambiguous alias and a remote hint all retain their old rules.
            json!({"target":{"machine":"local-node","session":"codex/old-6"}}),
            json!({"target":{"machine":"local-node","session":"codex/current-7"}}),
            json!({"target":{"machine":"local-node","session":"codex/current-8","session_id":"launch_id:gone"}}),
            json!({"target":{"machine":"local-node","session":"codex/current-9","session_id":"launch_id:ambiguous"}}),
            json!({"target":{"machine":"local-node","session":"codex/shared"}}),
            json!({"target":{"machine":"remote-node","session":"codex/remote","session_id":"launch_id:remote"}}),
        ];
        for (n, row) in rows.iter_mut().enumerate() {
            row["id"] = json!(format!("T-{n:06x}"));
        }
        let before = machines(crate::state::task_bindings_in(root).unwrap());
        let mut selected = machines(Vec::new());
        for_rows(&config(root), &mut selected, &rows).unwrap();
        for row in rows {
            let expected = label_view(row.clone(), "example", false, &[], &before).unwrap();
            let actual = label_view(row, "example", false, &[], &selected).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn remote_only_and_empty_pages_do_not_open_local_binding_storage() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("absent-state");
        let config = config(&missing);
        let mut selected = machines(Vec::new());
        for rows in [
            Vec::new(),
            vec![
                json!({"target":{"machine":"remote-node","session":"codex/remote",
                "session_id":"launch_id:remote"}}),
            ],
        ] {
            for_rows(&config, &mut selected, &rows).unwrap();
            assert!(selected.bindings.is_empty());
            assert!(!missing.exists());
        }
    }
}
