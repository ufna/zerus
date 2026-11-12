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
