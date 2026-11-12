use super::*;
impl Runtime {
    /// A standing team does not retain a retry row for each completed task.
    /// The key cursor makes every row reachable even while earlier waits stay.
    pub(super) fn cleanup_metadata(
        &mut self,
        store: &mut Store,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let after = self
            .load::<String>(&team.id, "metadata_cursor")?
            .unwrap_or_default();
        let rows = self.ledger.prefix_page(&team.id, "gate:", &after)?;
        let next = if rows.len() == 200 {
            rows.last().expect("full page").key.clone()
        } else {
            String::new()
        };
        let merging = self.ledger.merge_pending(&team.id)?;
        for row in rows {
            let Some(key) = row.key.strip_prefix("gate:") else {
                continue;
            };
            let Some((kind, value)) = key.split_once(':') else {
                continue;
            };
            if !matches!(
                kind,
                "observe" | "report" | "handout" | "finish" | "finish_refused" | "integration"
            ) {
                continue;
            }
            if kind == "integration" && merging {
                continue;
            }
            let Ok(node) = value.parse::<u32>() else {
                continue;
            };
            if store
                .node(&team.id, node)?
                .is_none_or(|n| matches!(n.state, NodeState::Accepted | NodeState::Cancelled))
            {
                self.recovered(store, team, key, now)?;
            }
        }
        self.put(&team.id, "metadata_cursor", &next)?;
        Ok(())
    }
}

impl Runtime {
    pub(super) fn prune_terminal_rows(&mut self, store: &Store) -> Result<bool> {
        let (id, after) = self
            .maintenance_team
            .clone()
            .expect("selected terminal team");
        let team = store.team(&id)?;
        let Some(team) = team.filter(|t| {
            matches!(
                t.status,
                TeamStatus::Stopped | TeamStatus::Archived | TeamStatus::Finished
            )
        }) else {
            self.maintenance_team = None;
            return Ok(true);
        };
        if team.status != TeamStatus::Finished
            && (!store.pending_cancels(&id, 1, 0)?.is_empty()
                || !store.keyed_tasks(&id, 0, 1)?.is_empty()
                || !store.open_effects(&id, 1, None)?.rows.is_empty())
        {
            // Keep work frontiers while terminal work is pending. Pruning
            // their cursors here could strand rows behind a blocked page.
            self.maintenance_team = None;
            return Ok(true);
        }
        let rows = self.ledger.page(&id, &after)?;
        for row in &rows {
            let evidence = ["delivery:", "merge:"]
                .iter()
                .any(|prefix| row.key.starts_with(prefix));
            if team.status == TeamStatus::Finished || !evidence {
                self.ledger.remove(row)?;
            }
        }
        if rows.len() == 200 {
            self.maintenance_team = Some((id, rows.last().unwrap().key.clone()));
            Ok(false)
        } else {
            self.maintenance_team = None;
            Ok(true)
        }
    }
}
