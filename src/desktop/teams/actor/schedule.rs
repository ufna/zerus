use super::*;
use crate::teams::store::EffectIntent;
use std::collections::BTreeSet;
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
        mut r: ops::Readiness,
        now: i64,
    ) -> Result<ops::Readiness> {
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
        // One readiness read: uncooled for the unstaffed comparison, then
        // cooled for the hand-outs. Members and open effects are read once,
        // and only when a node is ready.
        let raw = ops::readiness(store, &team.id)?;
        let any_ready = !ready::ready(&PlanView {
            phases: &raw.phases,
            nodes: &raw.nodes,
            members: &[],
            lead: &team.lead,
            integration: team.settings.integration,
        })
        .is_empty();
        let (effects, all_members) = if any_ready {
            (
                self.open_effects(store, &team.id)?,
                self.members(store, &team.id)?,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        self.unstaffed(store, team, &raw, &all_members, &effects, now)?;
        let r = self.cooled_readiness(store, team, raw, now)?;
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
        let mut members = Vec::new();
        for m in all_members {
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
    /// zerus-63rru.45: the kernel's unstaffed set over the uncooled view,
    /// compared with the told episodes by the store's own function; the
    /// store reconciles only when that finds a change and recomputes the
    /// set itself. The request is kept in the ledger before it is applied,
    /// so a restart replays it by its request ID and gets its saved reply. A
    /// race (`unstaffed_unchanged`, or a revision that moved) is settled,
    /// never a gate failure or a card.
    fn unstaffed(
        &mut self,
        store: &mut Store,
        team: &Team,
        raw: &ops::Readiness,
        members: &[Member],
        effects: &[EffectIntent],
        now: i64,
    ) -> Result<()> {
        if let Some(saved) = self.load::<Request>(&team.id, UNSTAFFED)? {
            self.apply_unstaffed(store, team, saved, now)?;
        }
        let open: BTreeSet<u32> = effects.iter().map(|e| e.node).collect();
        #[allow(unused_mut)]
        let mut set = ops::unstaffed_of(team, raw, members, &open);
        #[cfg(test)]
        STALE_VIEW.with(|stale| {
            if let Some(node) = stale.get() {
                set.push(zerus_team_plan::ready::Unstaffed {
                    node,
                    reason: "A stale view.".into(),
                });
            }
        });
        // Stale items (older unended items of a node, left by an earlier
        // layout) are ended a page per pass in their own operation, before
        // the cap is judged on the current episodes.
        if store.stale_unstaffed(&team.id)? && self.due(store, team, UNSTAFFED_STALE, "", now)? {
            self.end_stale_unstaffed(store, team, now)?;
        }
        let told = store.told_unstaffed(&team.id)?;
        let changes = ops::unstaffed_changes(&set, &ops::unstaffed_stages(raw), &told);
        if changes.is_empty() || !self.due(store, team, UNSTAFFED, "", now)? {
            return Ok(());
        }
        let (_, request) = request(store, &team.id, Operation::ReconcileUnstaffed)?;
        self.put(&team.id, UNSTAFFED, &request)?;
        self.apply_unstaffed(store, team, request, now)
    }
    /// One page of stale items ended. A lost result costs nothing: an ended
    /// item is never ended again, and the next pass asks with a new request.
    fn end_stale_unstaffed(&mut self, store: &mut Store, team: &Team, now: i64) -> Result<()> {
        let actor = Actor::Service {
            person: team.owner.clone(),
            team: team.id.clone(),
        };
        let (_, request) = request(store, &team.id, Operation::EndStaleUnstaffed)?;
        match ops::apply(store, &actor, &request, now) {
            Ok(_) => self.recovered(store, team, UNSTAFFED_STALE, now),
            Err(e) if e.code == code::UNSTAFFED_UNCHANGED || e.code == code::STALE_REVISION => {
                Ok(())
            }
            Err(e) => self.failure(store, team, UNSTAFFED_STALE, "", &e, now),
        }
    }
    fn apply_unstaffed(
        &mut self,
        store: &mut Store,
        team: &Team,
        request: Request,
        now: i64,
    ) -> Result<()> {
        let actor = Actor::Service {
            person: team.owner.clone(),
            team: team.id.clone(),
        };
        match ops::apply(store, &actor, &request, now) {
            Ok(_) => {
                unstaffed_boundary("applied");
                self.recovered(store, team, UNSTAFFED, now)?;
            }
            Err(e) if e.code == code::UNSTAFFED_UNCHANGED || e.code == code::STALE_REVISION => {}
            // Not saved: the same request ID is applied again next pass.
            Err(e) if e.code == code::STORAGE_FAILED => {
                return self.failure(store, team, UNSTAFFED, "", &e, now);
            }
            Err(e) => self.failure(store, team, UNSTAFFED, "", &e, now)?,
        }
        if let Some(row) = self.ledger.get(&team.id, UNSTAFFED)? {
            self.ledger.remove(&row)?;
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
        let ready = store.own_task_current(&team.id, d.node, d.generation)?;
        if !self.held_step(
            store,
            team,
            ("reopen", d.node, d.generation, d.seq, d.kind.as_str()),
            ready,
            now,
        )? {
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

/// The ledger key of the service's reconcile of unstaffed nodes.
const UNSTAFFED: &str = "unstaffed";
const UNSTAFFED_STALE: &str = "unstaffed_stale";
#[cfg(test)]
thread_local! {
    /// A test's stale view: the service believes this node is unstaffed.
    pub(super) static STALE_VIEW: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    pub(super) static UNSTAFFED_CRASH: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}
fn unstaffed_boundary(point: &str) {
    #[cfg(test)]
    UNSTAFFED_CRASH.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            panic!("fixture unstaffed restart at {point}");
        }
    });
    #[cfg(not(test))]
    let _ = point;
}
