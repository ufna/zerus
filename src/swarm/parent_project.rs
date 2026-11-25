//! Project consent attached to the desktop parent setter. The caller holds
//! the session writers while this module verifies and stages catalog changes.
use super::{catalog::Key, session_project::assign_project, store::LockedStore};
use crate::config::Config;
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Choice {
    Keep,
    Move,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviousParent {
    machine: String,
    session_id: String,
    run_id: String,
    conversation_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Consent {
    pub machine_id: String,
    child_id: String,
    child_project: String,
    parent_project: String,
    default_project: String,
    previous: Option<PreviousParent>,
    choice: Choice,
}

impl Consent {
    pub(crate) fn check_previous(&self, previous: Option<&Value>) -> Result<()> {
        let matches = match (&self.previous, previous) {
            (None, None) => true,
            (Some(expected), Some(actual)) => {
                actual["machine"] == expected.machine
                    && actual["session_id"].as_str().unwrap_or_default() == expected.session_id
                    && actual["run_id"].as_str().unwrap_or_default() == expected.run_id
                    && actual["conversation_id"].as_str().unwrap_or_default()
                        == expected.conversation_id
            }
            _ => false,
        };
        ensure!(
            matches,
            "The previous parent changed. Open Set parent again."
        );
        Ok(())
    }

    pub(crate) fn prepare(
        &self,
        config: &Config,
        child: &Value,
        parent: &Value,
    ) -> Result<Pending> {
        self.prepare_locked(LockedStore::try_load_scoped(config)?, child, parent)
    }

    fn prepare_locked(
        &self,
        mut store: LockedStore,
        child: &Value,
        parent: &Value,
    ) -> Result<Pending> {
        let stale = "The session projects changed or could not be verified. Open Set parent again.";
        ensure!(
            store.data.imported && store.data.node_id == self.machine_id,
            stale
        );
        ensure!(
            child["session_id"] == self.child_id,
            "The child session changed. Open Set parent again."
        );
        let catalog = &store.data.catalog;
        let machine = catalog.machine(&store.data.node_id);
        let physical_heads = catalog.effective_heads();
        let primary = store
            .data
            .id_memberships
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(stale))?;
        let resolved = primary.resolved(catalog)?;
        let live = |id: &str| {
            let key = Key::project(id, "alive");
            physical_heads
                .get(&key)
                .and_then(|ops| super::catalog::Catalog::selected(&key, ops))
                .is_some_and(|op| op.value == true)
        };
        ensure!(live(&self.default_project), stale);
        let membership = |record: &Value, expected: &str| -> Result<Key> {
            let id = record["session_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(stale))?;
            let name = record["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(stale))?;
            let key = Key::Session {
                machine: machine.clone(),
                session: name.into(),
            };
            let project = if let Some(by_id) = resolved.get(&(machine.clone(), id.to_owned())) {
                ensure!(!by_id.conflict, stale);
                by_id.value.as_str()
            } else if let Some(ops) = physical_heads.get(&key) {
                let picked = super::catalog::Catalog::selected(&key, ops);
                ensure!(
                    picked.is_none_or(|op| ops.iter().all(|other| other.value == op.value)),
                    stale
                );
                picked.and_then(|op| op.value.as_str())
            } else {
                None
            };
            ensure!(
                live(expected)
                    && project
                        .filter(|id| live(id))
                        .unwrap_or(&self.default_project)
                        == expected,
                stale
            );
            Ok(key)
        };
        let child_key = membership(child, &self.child_project)?;
        membership(parent, &self.parent_project)?;
        ensure!(self.child_project != self.parent_project, stale);
        if self.choice == Choice::Keep {
            return Ok(Pending {
                store,
                choice: Choice::Keep,
            });
        }
        assign_project(
            &mut store.data,
            &child_key,
            child["name"].as_str().expect("validated name"),
            Some(&self.child_id),
            &self.parent_project,
        )?;
        Ok(Pending {
            store,
            choice: Choice::Move,
        })
    }
}

/// Keep the catalog locked until the parent write returns. If that write is
/// unconfirmed, dropping this value writes no project placement.
pub(crate) struct Pending {
    store: LockedStore,
    choice: Choice,
}

impl Pending {
    pub(crate) fn finish(mut self) -> &'static str {
        match self.choice {
            Choice::Keep => "kept",
            Choice::Move => match self.store.save_catalog_only() {
                Ok(()) => "moved",
                Err(error) => {
                    eprintln!("hgs: project move could not be confirmed: {error}");
                    "uncertain"
                }
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn projects(config: &Config) -> String {
        let mut store = LockedStore::load_bootstrap(config).unwrap();
        store.data.import_missing(&serde_json::json!({"projects": [
            {"id":"child-project","name":"Child project","color":"#123456","sessions":[],"folders":[]},
            {"id":"parent-project","name":"Parent project","color":"#654321","sessions":[],"folders":[]}
        ]})).unwrap();
        store.save().unwrap();
        store.data.node_id.clone()
    }

    pub(crate) fn place(config: &Config, session: &Value, project: &str) {
        let mut store = LockedStore::load_scoped(config).unwrap();
        let name = session["name"].as_str().unwrap();
        let key = Key::Session {
            machine: store.data.catalog.machine(&store.data.node_id),
            session: name.into(),
        };
        assign_project(
            &mut store.data,
            &key,
            name,
            session["session_id"].as_str(),
            project,
        )
        .unwrap();
        store.save_catalog_only().unwrap();
    }

    pub(crate) fn membership(config: &Config, session: &Value) -> String {
        let store = LockedStore::load_scoped(config).unwrap();
        store
            .data
            .id_memberships
            .as_ref()
            .unwrap()
            .resolve(
                &store.data.catalog,
                &store.data.node_id,
                session["session_id"].as_str().unwrap(),
            )
            .unwrap()
            .unwrap()
            .value
            .as_str()
            .unwrap()
            .into()
    }

    pub(crate) fn fail_catalog<T>(apply: impl FnOnce() -> T) -> T {
        super::super::store::FAIL_CATALOG.with(|fail| {
            fail.set(true);
            let outcome = apply();
            fail.set(false);
            outcome
        })
    }
}
