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
        let reopens = self.metadata_reopens(store, team, &rows)?;
        for row in rows {
            self.cleanup_gate(store, team, &row, &reopens, merging, now)?;
        }
        self.put(&team.id, "metadata_cursor", &next)?;
        Ok(())
    }
    fn metadata_reopens(
        &self,
        store: &Store,
        team: &Team,
        rows: &[Row],
    ) -> Result<std::collections::BTreeSet<u64>> {
        let mut pending = std::collections::BTreeSet::new();
        if !rows
            .iter()
            .any(|r| r.key.starts_with("gate:reopen:") || r.key.starts_with("gate:held:reopen:"))
        {
            return Ok(pending);
        }
        let mut cursor = None;
        for _ in 0..4 {
            let page = store.pending_reopens(&team.id, 200, cursor.as_ref())?;
            pending.extend(page.rows.into_iter().map(|d| d.seq));
            cursor = page.next;
            if cursor.is_none() {
                return Ok(pending);
            }
        }
        Err(problem("Reopen cleanup page bound exceeded"))
    }
    fn cleanup_gate(
        &mut self,
        store: &mut Store,
        team: &Team,
        row: &Row,
        reopens: &std::collections::BTreeSet<u64>,
        merging: bool,
        now: i64,
    ) -> Result<()> {
        let Some(key) = row.key.strip_prefix("gate:") else {
            return Ok(());
        };
        if let Some(decision) = key
            .strip_prefix("reopen:")
            .and_then(|s| s.parse::<u64>().ok())
        {
            if !reopens.contains(&decision) {
                self.recovered(store, team, key, now)?;
            }
            return Ok(());
        }
        let held = key.strip_prefix("held:");
        let Some((kind, value)) = held.unwrap_or(key).split_once(':') else {
            return Ok(());
        };
        if if held.is_some() {
            !matches!(kind, "finish" | "merge" | "reopen")
        } else {
            !matches!(
                kind,
                "observe" | "report" | "handout" | "finish" | "finish_refused" | "integration"
            )
        } {
            return Ok(());
        }
        if held.is_none() && kind == "integration" && merging {
            return Ok(());
        }
        let Ok(node) = value.parse::<u32>() else {
            return Ok(());
        };
        if held.is_some() && self.held_retired(store, team, row, kind, node, reopens)? {
            self.recovered(store, team, key, now)?;
            return Ok(());
        }
        if store
            .node(&team.id, node)?
            .is_none_or(|n| matches!(n.state, NodeState::Accepted | NodeState::Cancelled))
        {
            self.recovered(store, team, key, now)?;
        }
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
