use super::*;
use crate::teams::{ops::HandoutOutcome, store::EffectIntent};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Delivery {
    node: u32,
    member: String,
    id: String,
    kind: String,
    intent: Option<Request>,
    outcome: Option<HandoutOutcome>,
    receipt: Option<Request>,
    gated: bool,
    #[serde(default)]
    not_started: bool,
    #[serde(default)]
    nonstarts: u32,
    /// The refusal is the terminal gate's: the member's terminal holds
    /// input (zerus-63rru.79). An older build reads a plain refusal.
    #[serde(default)]
    waiting: bool,
    /// Which dialog the gate knew, when one did (zerus-63rru.83).
    #[serde(default)]
    dialog: Option<String>,
    /// The stamp of the member's row when that refusal was observed.
    #[serde(default)]
    stamp: Option<String>,
}
impl Runtime {
    pub(super) fn send(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        assignment: (&Node, &Member),
        kind: &str,
        now: i64,
    ) -> Result<()> {
        let (node, member) = assignment;
        let id = uuid::Uuid::new_v4().to_string();
        let op = if kind == "reopen" {
            Operation::ReopenIntent {
                node: node.id,
                member: member.id.clone(),
                effect_request_id: id.clone(),
            }
        } else {
            Operation::HandoutIntent {
                node: node.id,
                member: member.id.clone(),
                effect_request_id: id.clone(),
            }
        };
        let (actor, intent) = request(store, &team.id, op)?;
        let mut d = Delivery {
            node: node.id,
            member: member.id.clone(),
            id: id.clone(),
            kind: kind.into(),
            intent: Some(intent),
            outcome: None,
            receipt: None,
            gated: false,
            not_started: false,
            nonstarts: 0,
            waiting: false,
            dialog: None,
            stamp: None,
        };
        let key = format!("delivery:{id}");
        self.ledger.reserve(
            &team.id,
            &key,
            serde_json::to_value(&d).map_err(crate::teams::store::storage)?,
        )?;
        boundary("reserved");
        // This frame may attempt an original. A durably known non-start
        // permits a later fresh admission; an unknown original never does.
        let intent = d.intent.as_ref().expect("reserved");
        if let Err(error) = ops::apply(store, &actor, intent, now) {
            if store.request_refused(&team.id, &intent.request_id)? {
                if let Some(row) = self.ledger.get(&team.id, &key)? {
                    self.ledger.remove(&row)?;
                }
            }
            return Err(error);
        }
        boundary("intent");
        d.intent = None;
        self.invoke_delivery(store, ports, team, &key, d, (node, member), now)
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke_delivery(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        key: &str,
        mut d: Delivery,
        assignment: (&Node, &Member),
        now: i64,
    ) -> Result<()> {
        if d.not_started {
            // Consume the proof durably before attempting a new original. If
            // this result is lost, recovery is uncertain and cannot send.
            d.not_started = false;
            self.put(&team.id, key, &d)?;
            boundary("retry_armed");
        }
        let result = ports.deliver(team, assignment.0, assignment.1, &d.id);
        boundary("effect");
        match result {
            ports::DeliveryResult::Stale => {
                self.defer_delivery(store, ports, team, (key, assignment.0), d, now)
            }
            ports::DeliveryResult::Outcome(outcome) => {
                d.outcome = Some(outcome);
                self.put(&team.id, key, &d)?;
                boundary("outcome");
                self.settle(store, team, key, d, now)
            }
            // Refused before any input because the member's terminal holds
            // input: recorded as a refusal, with that cause and the member's
            // row as it is now, both kept before anything settles.
            ports::DeliveryResult::TerminalWaiting { reason, dialog } => {
                d.stamp = ports.row_stamp(team, assignment.1).ok().flatten();
                d.waiting = true;
                d.dialog = dialog;
                d.outcome = Some(HandoutOutcome::Refused { reason });
                self.put(&team.id, key, &d)?;
                boundary("outcome");
                self.settle(store, team, key, d, now)
            }
            ports::DeliveryResult::NotStarted { reason } => {
                d.not_started = true;
                d.nonstarts = d.nonstarts.saturating_add(1);
                if d.nonstarts >= 5 {
                    // Five non-starts reach the retry table's fifteen-minute
                    // tier (0, 1, 3, 7, 15 minutes). Free the reservation so
                    // the kernel can choose another member on a later pass.
                    d.outcome = Some(HandoutOutcome::Refused {
                        reason: reason.clone(),
                    });
                }
                self.put(&team.id, key, &d)?;
                boundary("not_started");
                if d.outcome.is_some() {
                    self.settle(store, team, key, d, now)
                } else {
                    self.failure(store, team, key, "", &problem(&reason), now)
                }
            }
        }
    }
    fn defer_delivery(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        target: (&str, &Node),
        mut d: Delivery,
        now: i64,
    ) -> Result<()> {
        // The CLI ran, but proved no delivery started. Persist that proof
        // before observing; an uncertain original never gets it.
        let (key, node) = target;
        d.not_started = true;
        self.put(&team.id, key, &d)?;
        let task = node
            .task
            .as_ref()
            .ok_or_else(|| problem("Stale delivery has no task"))?;
        let observed = ports.observe(store, task);
        // Even a failed read must not leave the delivery itself ungated.
        self.defer(store, team, key, "", now)?;
        observed
    }
    pub(super) fn recover(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let mut budget = 4;
        for e in self
            .open_effects(store, &team.id)?
            .into_iter()
            .filter(|e| e.outcome.is_none())
        {
            let key = format!("delivery:{}", e.request_id);
            if self.ledger.get(&team.id, &key)?.is_none() {
                self.put(&team.id, &key, &Delivery::interrupted(e))?;
            }
            self.recover_row(store, ports, team, &key, now, &mut budget)?;
        }
        let after = self
            .load::<String>(&team.id, "delivery_cursor")?
            .unwrap_or_default();
        let rows = self.ledger.page(&team.id, &after)?;
        let next = if rows.len() == 200 {
            rows.last().expect("full page").key.clone()
        } else {
            String::new()
        };
        for row in rows {
            if row.key.starts_with("delivery:") {
                self.recover_row(store, ports, team, &row.key, now, &mut budget)?;
            }
        }
        self.put(&team.id, "delivery_cursor", &next)?;
        Ok(())
    }
    fn recover_row(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        key: &str,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        if !self.due(store, team, key, "", now)? {
            return Ok(());
        }
        if let Err(error) = self.recover_delivery(store, ports, team, key, now, budget) {
            self.failure(store, team, key, "", &error, now)?;
        }
        Ok(())
    }
    fn recover_delivery(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        key: &str,
        now: i64,
        budget: &mut u8,
    ) -> Result<()> {
        let Some(row) = self.ledger.get(&team.id, key)? else {
            return Ok(());
        };
        let mut d: Delivery =
            serde_json::from_value(row.data.clone()).map_err(crate::teams::store::storage)?;
        if let Some(intent) = &d.intent {
            let actor = Actor::Service {
                person: team.owner.clone(),
                team: team.id.clone(),
            };
            match ops::apply(store, &actor, intent, now) {
                Ok(_) => d.intent = None,
                Err(error) => {
                    if store.request_refused(&team.id, &intent.request_id)? {
                        self.recovered(store, team, key, now)?;
                        self.ledger.remove(&row)?;
                        return Ok(());
                    }
                    return Err(error);
                }
            }
        }
        if d.outcome.is_none() && !store.effect_reserved(&team.id, &d.id)? {
            // The store settled this effect before any outcome reached the
            // row: a removal's hand-back (zerus-63rru.77), a reopen or an
            // override. No admission lets it out again, so the row ends now:
            // nothing is delivered, replayed or recorded, no failure stays.
            self.recovered(store, team, key, now)?;
            self.ledger.remove(&row)?;
            return Ok(());
        }
        if d.not_started && d.outcome.is_none() {
            if *budget == 0 || ports.stopped() || team.status != TeamStatus::Running {
                return Ok(());
            }
            let node = store
                .node(&team.id, d.node)?
                .ok_or_else(|| problem("Deferred delivery node disappeared"))?;
            let member = store
                .member(&team.id, &d.member)?
                .ok_or_else(|| problem("Deferred delivery member disappeared"))?;
            *budget -= 1;
            return self.invoke_delivery(store, ports, team, key, d, (&node, &member), now);
        }
        if d.outcome.is_none() {
            d.outcome=Some(HandoutOutcome::Uncertain{reason:"The service restarted or lost its original result; this delivery will not be sent again.".into()});
            self.put(&team.id, key, &d)?;
        }
        self.settle(store, team, key, d, now)
    }
    fn settle(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        mut d: Delivery,
        now: i64,
    ) -> Result<()> {
        if !d.gated {
            // A refusal of an effect the store has already settled (a
            // removal's hand-back, which told the lead) is no failure of the
            // node's hand-out (zerus-63rru.77), and neither is a refusal of
            // the terminal gate (zerus-63rru.79).
            let refused = match &d.outcome {
                Some(HandoutOutcome::Refused { reason })
                    if !d.waiting && store.effect_reserved(&team.id, &d.id)? =>
                {
                    Some(reason)
                }
                _ => None,
            };
            if let Some(reason) = refused {
                let revision = store
                    .task_observation(&team.id, d.node)?
                    .map_or(String::new(), |o| o.revision);
                self.failure(
                    store,
                    team,
                    &format!("handout:{}", d.node),
                    &revision,
                    &problem(reason),
                    now,
                )?;
            }
            // A refusal of the terminal gate is not a failed hand-out: the
            // member's terminal is not free (zerus-63rru.79).
            if d.waiting {
                self.terminal_refused(
                    &team.id,
                    &d.member,
                    d.stamp.clone(),
                    d.dialog.clone(),
                    now,
                )?;
            }
            // A submitted delivery: the member's terminal took input.
            if matches!(d.outcome, Some(HandoutOutcome::Submitted { .. })) {
                self.terminal_free(&team.id, &d.member)?;
            }
            // A hand-back is settled by the store at once; nothing retries it.
            if matches!(
                d.outcome,
                Some(HandoutOutcome::Submitted { .. } | HandoutOutcome::HandedBack { .. })
            ) {
                self.recovered(store, team, &format!("handout:{}", d.node), now)?;
            }
            d.gated = true;
            self.put(&team.id, key, &d)?;
        }
        if d.receipt.is_none() {
            let outcome = d.outcome.clone().expect("durable outcome");
            let op = if d.kind == "reopen" {
                Operation::ReopenOutcome {
                    effect_request_id: d.id.clone(),
                    outcome,
                }
            } else {
                Operation::HandoutOutcome {
                    effect_request_id: d.id.clone(),
                    outcome,
                }
            };
            d.receipt = Some(request(store, &team.id, op)?.1);
            self.put(&team.id, key, &d)?;
        }
        let actor = Actor::Service {
            person: team.owner.clone(),
            team: team.id.clone(),
        };
        match ops::apply(store, &actor, d.receipt.as_ref().expect("reserved"), now) {
            Ok(_) => {
                boundary("recorded");
                if matches!(team.status, TeamStatus::Running | TeamStatus::Paused) {
                    self.recovered(store, team, key, now)?;
                }
                if let Some(row) = self.ledger.get(&team.id, key)? {
                    self.ledger.remove(&row)?;
                }
            }
            Err(e) if e.code == code::STALE_REVISION => {
                d.receipt = None;
                self.put(&team.id, key, &d)?;
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    /// Settle only durable proof that no child started, before terminal
    /// cancellation. Unknown originals remain evidence and are never retried.
    pub(super) fn terminal_deliveries(
        &mut self,
        store: &mut Store,
        team: &Team,
        now: i64,
    ) -> Result<bool> {
        if !matches!(team.status, TeamStatus::Stopped | TeamStatus::Archived) {
            return Ok(true);
        }
        let after = self
            .load::<String>(&team.id, "terminal_delivery_cursor")?
            .unwrap_or_default();
        let rows = self.ledger.prefix_page(&team.id, "delivery:", &after)?;
        let next = if rows.len() == 200 {
            rows.last().unwrap().key.clone()
        } else {
            String::new()
        };
        for row in rows {
            let mut d: Delivery =
                serde_json::from_value(row.data).map_err(crate::teams::store::storage)?;
            if !d.not_started {
                continue;
            }
            if d.outcome.is_none() {
                d.outcome = Some(HandoutOutcome::Refused {
                    reason: "Delivery did not start before the team ended".into(),
                });
                d.gated = true;
                self.put(&team.id, &row.key, &d)?;
            }
            self.settle(store, team, &row.key, d, now)?;
        }
        self.put(&team.id, "terminal_delivery_cursor", &next)?;
        Ok(next.is_empty())
    }
    pub(super) fn open_effects(&self, store: &Store, team: &str) -> Result<Vec<EffectIntent>> {
        let mut out = Vec::new();
        let mut cursor = None;
        for _ in 0..4 {
            let page = store.open_effects(team, 200, cursor.as_ref())?;
            out.extend(page.rows);
            cursor = page.next;
            if cursor.is_none() {
                return Ok(out);
            }
        }
        Err(problem("Open effect page bound exceeded"))
    }
}
impl Delivery {
    fn interrupted(e: EffectIntent) -> Self {
        Self {
            node: e.node,
            member: e.member,
            id: e.request_id,
            kind: e.kind,
            intent: None,
            outcome: None,
            receipt: None,
            gated: false,
            not_started: false,
            nonstarts: 0,
            waiting: false,
            dialog: None,
            stamp: None,
        }
    }
}

#[cfg(test)]
thread_local! {pub(super) static CRASH: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };}
fn boundary(name: &str) {
    #[cfg(test)]
    CRASH.with(|c| {
        if c.get() == Some(name) {
            c.set(None);
            panic!("fixture restart at {name}");
        }
    });
    #[cfg(not(test))]
    let _ = name;
}
