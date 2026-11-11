use super::*;
mod outcome;
use ports::{MergePlan, MergeResult};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct MergeRow {
    plan: MergePlan,
    outcome: Option<MergeResult>,
    fact_id: Option<String>,
    fact_revision: Option<u64>,
    #[serde(default)]
    epoch: String,
    #[serde(default)]
    reserved_ms: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    padding: String,
}
impl Runtime {
    pub(super) fn integrations(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        self.recover_merges(store, ports, team, now)?;
        let mut cursor = None;
        let mut budget = 4;
        for _ in 0..4 {
            let page = store.integration_accepts(&team.id, 200, cursor.as_ref())?;
            for d in page.rows {
                if ports.stopped() {
                    return Ok(());
                }
                if budget == 0
                    || self.ledger.merge_unread(&team.id)?
                    || self
                        .ledger
                        .get(&team.id, &format!("merge:{}", d.seq))?
                        .is_some()
                {
                    continue;
                }
                let key = format!("integration:{}", d.node);
                let stamp = format!("{}:{}", d.seq, d.generation);
                if !self.due(store, team, &key, &stamp, now)? {
                    continue;
                }
                if let Err(error) = self.integrate(store, ports, team, &d, now, &mut budget) {
                    self.failure(store, team, &key, &stamp, &error, now)?;
                }
            }
            cursor = page.next;
            if cursor.is_none() {
                return Ok(());
            }
        }
        Err(problem("Integration page bound exceeded"))
    }
    pub(super) fn recover_merges(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let after = self
            .load::<String>(&team.id, "merge_cursor")?
            .unwrap_or_default();
        let rows = self.ledger.prefix_page(&team.id, "merge:", &after)?;
        let next = if rows.len() == 200 {
            rows.last().expect("full page").key.clone()
        } else {
            String::new()
        };
        for row in rows {
            if ports.stopped() {
                return Ok(());
            }
            if let Err(error) = self.settle_merge(store, ports, team, &row.key, now) {
                let data: MergeRow =
                    serde_json::from_value(row.data).map_err(crate::teams::store::storage)?;
                self.force_failure(
                    store,
                    team,
                    &format!("integration:{}", data.plan.node),
                    &format!("{}:{}", data.plan.decision, data.plan.generation),
                    &error,
                    now,
                )?;
            }
        }
        self.put(&team.id, "merge_cursor", &next)?;
        Ok(())
    }
    fn integrate(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        d: &crate::teams::store::IntegrationAccept,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        let Some(report) = store.report(&team.id, d.node, d.generation)? else {
            return Ok(());
        };
        let plan = MergePlan {
            team: team.id.clone(),
            owner: team.owner.clone(),
            machine: team.home.clone(),
            project: team.project.clone(),
            branch: team.integration_branch.clone(),
            base: team.base_commit.clone(),
            repo: ports.repository(team)?,
            node: d.node,
            generation: d.generation,
            decision: d.seq,
            member: report.member,
            commit: report.commit,
            id: uuid::Uuid::new_v4().to_string(),
        };
        if !ports.descends(&plan)? {
            return Err(problem(
                "Reported commit does not descend from the assigned base",
            ));
        }
        let key = format!("merge:{}", d.seq);
        let mut row = MergeRow {
            plan,
            outcome: None,
            fact_id: None,
            fact_revision: None,
            epoch: self.epoch.clone(),
            reserved_ms: now,
            padding: String::new(),
        };
        outcome::reserve(&mut row)?;
        self.ledger.reserve(
            &team.id,
            &key,
            serde_json::to_value(&row).map_err(crate::teams::store::storage)?,
        )?;
        boundary("reserved");
        *budget -= 1;
        let reply = ports.merge(&row.plan);
        outcome::save(&mut row, reply)?;
        boundary("effect");
        self.put(&team.id, &key, &row)?;
        boundary("outcome");
        self.settle_merge(store, ports, team, &key, now)
    }
    fn settle_merge(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        key: &str,
        now: i64,
    ) -> Result<()> {
        let Some(raw) = self.ledger.get(&team.id, key)? else {
            return Ok(());
        };
        let mut row: MergeRow =
            serde_json::from_value(raw.data).map_err(crate::teams::store::storage)?;
        let gate = format!("integration:{}", row.plan.node);
        let stamp = format!("{}:{}", row.plan.decision, row.plan.generation);
        if store
            .integration_fact(&team.id, row.plan.node)?
            .is_some_and(|f| f.decision == row.plan.decision)
        {
            return self.merge_settled(store, team, key, &gate, now);
        }
        if let Some(MergeResult::Unavailable { reason }) = &row.outcome {
            self.recorded_failure(
                store,
                team,
                (&gate, &stamp),
                &row.plan.id,
                &problem(reason),
                now,
            )?;
            if let Some(raw) = self.ledger.get(&team.id, key)? {
                self.ledger.remove(&raw)?;
            }
            return Ok(());
        }
        if !self.due(store, team, &gate, &stamp, now)? {
            return Ok(());
        }
        if row.outcome.is_none() || matches!(row.outcome, Some(MergeResult::Unknown { .. })) {
            match ports.contains(&row.plan) {
                Ok(true)=>{row.outcome=Some(MergeResult::Merged{commit:row.plan.commit.clone()});self.put(&team.id,key,&row)?;},
                _=>return self.force_failure(store,team,&gate,&stamp,&problem("Integration result is uncertain. Checks only read ancestry; this merge will not be started again."),now),
            }
        }
        self.record_merge(store, team, key, &gate, &mut row, now)
    }
    fn record_merge(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        gate: &str,
        row: &mut MergeRow,
        now: i64,
    ) -> Result<()> {
        // A prior fact in this batch can stale another saved envelope. Only
        // fact writes are retried here, with a new ID after a saved refusal.
        for _ in 0..2 {
            if row.fact_id.is_none() {
                row.fact_id = Some(uuid::Uuid::new_v4().to_string());
                row.fact_revision = Some(
                    store
                        .team(&team.id)?
                        .ok_or_else(|| problem("Team disappeared"))?
                        .revision,
                );
                self.put(&team.id, key, row)?;
            }
            let operation = match row.outcome.as_ref().expect("known outcome") {
                MergeResult::Merged { commit } => Operation::RecordIntegration {
                    node: row.plan.node,
                    decision: row.plan.decision,
                    commit: commit.clone(),
                },
                MergeResult::Conflict { files } => Operation::RecordIntegrationConflict {
                    node: row.plan.node,
                    decision: row.plan.decision,
                    conflicts: files.clone(),
                },
                _ => return Err(problem("Integration has no settled result")),
            };
            let request = Request {
                request_id: row.fact_id.clone().expect("reserved"),
                team: team.id.clone(),
                expected_revision: row.fact_revision.expect("reserved"),
                operation,
            };
            let actor = Actor::Service {
                person: row.plan.owner.clone(),
                team: team.id.clone(),
            };
            match ops::apply(store, &actor, &request, now) {
                Ok(_) => {
                    boundary("recorded");
                    return self.merge_settled(store, team, key, gate, now);
                }
                Err(error) if error.code == code::STALE_REVISION => {
                    row.fact_id = None;
                    row.fact_revision = None;
                    self.put(&team.id, key, row)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    fn merge_settled(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        gate: &str,
        now: i64,
    ) -> Result<()> {
        self.recovered(store, team, gate, now)?;
        if let Some(row) = self.ledger.get(&team.id, key)? {
            self.ledger.remove(&row)?;
        }
        Ok(())
    }
}
#[cfg(test)]
thread_local! {pub(super) static CRASH:std::cell::Cell<Option<&'static str>>=const {std::cell::Cell::new(None)};}
fn boundary(name: &str) {
    #[cfg(test)]
    CRASH.with(|c| {
        if c.get() == Some(name) {
            c.set(None);
            panic!("fixture restart at merge {name}");
        }
    });
    #[cfg(not(test))]
    let _ = name;
}
