use super::*;
use zerus_team_plan::ready::{self, MemberView, PlanView};
impl Runtime {
    fn members(&self, store: &Store, team: &str) -> Result<Vec<Member>> {
        let mut out = Vec::new();
        let mut cursor = None;
        for _ in 0..2 {
            let page = store.members(team, 200, cursor.as_ref())?;
            out.extend(page.rows);
            cursor = page.next;
            if cursor.is_none() {
                return Ok(out);
            }
        }
        Err(problem("Member page bound exceeded"))
    }
    fn member_free(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        m: &Member,
        now: i64,
    ) -> Result<bool> {
        let key = format!("member_free:{}", m.id);
        if !self.due(store, team, &key, "", now)? {
            return Ok(false);
        }
        match ports.free(team, m) {
            Ok(free) => {
                self.recovered(store, team, &key, now)?;
                Ok(free)
            }
            Err(e) => {
                self.failure(store, team, &key, "", &e, now)?;
                Ok(false)
            }
        }
    }
    fn cooled_readiness(
        &mut self,
        store: &mut Store,
        team: &Team,
        now: i64,
    ) -> Result<ops::Readiness> {
        let mut r = ops::readiness(store, &team.id)?;
        let view = PlanView {
            phases: &r.phases,
            nodes: &r.nodes,
            members: &[],
            lead: &team.lead,
            integration: team.settings.integration,
        };
        let mut cooling = std::collections::BTreeSet::new();
        for id in ready::ready(&view) {
            let revision = store
                .task_observation(&team.id, id)?
                .map_or(String::new(), |o| o.revision);
            if !self.due(store, team, &format!("handout:{id}"), &revision, now)? {
                cooling.insert(id);
            }
        }
        // This narrows the view only. Each selected node is checked again
        // against the store's unchanged kernel view before its intent.
        for n in &mut r.nodes {
            if cooling.contains(&n.id) {
                n.hold = true;
            }
        }
        Ok(r)
    }
    pub(super) fn handouts(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let r = self.cooled_readiness(store, team, now)?;
        let initial = PlanView {
            phases: &r.phases,
            nodes: &r.nodes,
            members: &[],
            lead: &team.lead,
            integration: team.settings.integration,
        };
        if ready::ready(&initial).is_empty() {
            return Ok(());
        }
        let effects = self.open_effects(store, &team.id)?;
        let mut members = Vec::new();
        for m in self.members(store, &team.id)? {
            let free = m.id != team.lead
                && m.state == MemberState::Active
                && m.session.is_some()
                && !self.ledger.pending_member(&team.id, &m.id)?
                && !effects.iter().any(|e| e.member == m.id)
                && self.member_free(store, ports, team, &m, now)?;
            members.push(MemberView {
                id: m.id,
                role: m.role,
                state: m.state,
                free,
            });
        }
        let view = PlanView {
            phases: &r.phases,
            nodes: &r.nodes,
            members: &members,
            lead: &team.lead,
            integration: team.settings.integration,
        };
        for h in ready::handouts(&view).into_iter().take(8) {
            let key = format!("handout:{}", h.node);
            let revision = store
                .task_observation(&team.id, h.node)?
                .map_or(String::new(), |o| o.revision);
            if !self.due(store, team, &key, &revision, now)? {
                continue;
            }
            let result = self.handout(store, ports, team, h, now);
            if let Err(e) = result {
                self.failure(store, team, &key, &revision, &e, now)?;
            }
        }
        Ok(())
    }
    fn handout(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        h: ready::Handout,
        now: i64,
    ) -> Result<()> {
        if self.ledger.pending_node(&team.id, h.node)? {
            return Ok(());
        }
        if !self.due(store, team, &format!("observe:{}", h.node), "", now)? {
            return Ok(());
        }
        let Some(n) = store.node(&team.id, h.node)? else {
            return Ok(());
        };
        let Some(task) = n.task.as_ref() else {
            return Ok(());
        };
        ports.observe(store, task)?;
        let r = ops::readiness(store, &team.id)?;
        let view = PlanView {
            phases: &r.phases,
            nodes: &r.nodes,
            members: &[],
            lead: &team.lead,
            integration: team.settings.integration,
        };
        if !ready::ready(&view).contains(&h.node) {
            return Ok(());
        }
        let Some(node) = store.node(&team.id, h.node)? else {
            return Ok(());
        };
        let Some(m) = store.member(&team.id, &h.member)? else {
            return Ok(());
        };
        if m.state != MemberState::Active || !ports.free(team, &m)? {
            return Ok(());
        }
        self.send(store, ports, team, (&node, &m), "handout", now)
    }
    pub(super) fn reopens(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let mut cursor = None;
        for _ in 0..4 {
            let page = store.pending_reopens(&team.id, 200, cursor.as_ref())?;
            for d in page.rows {
                if ports.stopped() {
                    return Ok(());
                }
                let key = format!("reopen:{}", d.seq);
                if !self.due(store, team, &key, "", now)? {
                    continue;
                }
                if let Err(e) = self.reopen(store, ports, team, &d, now) {
                    self.failure(store, team, &key, "", &e, now)?;
                }
            }
            cursor = page.next;
            if cursor.is_none() {
                return Ok(());
            }
        }
        Err(problem("Reopen page bound exceeded"))
    }
    fn reopen(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        d: &crate::teams::store::Decision,
        now: i64,
    ) -> Result<()> {
        if !store.own_task_current(&team.id, d.node, d.generation)? {
            return Ok(());
        }
        let Some(node) = store.node(&team.id, d.node)? else {
            return Ok(());
        };
        let member = if d.kind == "reassign" {
            node.assignee.clone()
        } else {
            self.open_effects(store, &team.id)?
                .iter()
                .filter(|e| e.node == node.id)
                .max_by_key(|e| e.seq)
                .map(|e| e.member.clone())
                .or(store
                    .report(&team.id, node.id, d.generation)?
                    .map(|r| r.member))
        };
        let Some(m) = member
            .map(|id| store.member(&team.id, &id))
            .transpose()?
            .flatten()
        else {
            return Ok(());
        };
        if m.state != MemberState::Active || !ports.free(team, &m)? {
            return Ok(());
        }
        self.send(store, ports, team, (&node, &m), "reopen", now)?;
        self.recovered(store, team, &format!("reopen:{}", d.seq), now)
    }
}
