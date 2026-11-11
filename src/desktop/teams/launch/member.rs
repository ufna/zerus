use super::*;

impl Runtime {
    pub(super) fn member(
        &mut self,
        store: &mut Store,
        effects: &mut impl Effects,
        team: &Team,
        member: &Member,
        parent: Option<SessionKey>,
        now: i64,
    ) -> Result<bool> {
        let existing = self.ledger().get(&team.id, &member.id)?;
        let Some(mut row) = existing else {
            if member.state != MemberState::NotCreated || member.session.is_some() {
                return Ok(false);
            }
            return self.prepare(store, effects, team, member, parent, now);
        };
        if row.state == State::Blocked {
            self.card(store, &mut row, now)?;
            return Ok(false);
        }
        if matches!(row.state, State::Done) {
            return Ok(false);
        }
        if member.state == MemberState::Paused {
            if self.owns(&row) {
                self.admitted = None;
            }
            return Ok(false);
        }
        if let Some(plan) = &row.plan {
            if plan.machine != team.home
                || plan.project != team.project
                || plan.agent != member.agent
                || plan.role != member.role
                || plan.parent != parent
            {
                self.block(&mut row, invalid("Member launch plan changed"), now)?;
                return Ok(true);
            }
        }
        match row.state {
            State::WorktreeIntent | State::WorktreeDone | State::LaunchIntent
                if !self.owns(&row) =>
            {
                self.block(
                    &mut row,
                    uncertain("Member creation was interrupted; the original is never continued"),
                    now,
                )?;
                Ok(true)
            }
            State::WorktreeReady | State::LaunchReady => {
                row.state = if row.state == State::WorktreeReady {
                    State::WorktreeIntent
                } else {
                    State::LaunchIntent
                };
                row.original = uuid::Uuid::new_v4().to_string();
                row.epoch = self.epoch.clone();
                self.save(&mut row, now)?;
                self.admitted = Some((row.team.clone(), row.original));
                Ok(true)
            }
            State::WorktreeIntent => self.worktree(effects, &mut row, now),
            State::WorktreeDone => {
                row.state = State::LaunchIntent;
                row.original = uuid::Uuid::new_v4().to_string();
                self.save(&mut row, now)?;
                self.admitted = Some((row.team.clone(), row.original));
                Ok(true)
            }
            State::LaunchIntent => self.launch(effects, &mut row, now),
            State::Submitted => self.bind(store, effects, team, &mut row, now),
            State::Bound | State::ParentIntent => self.parent(effects, &mut row, now),
            _ => Err(invalid("Invalid member creation state")),
        }
    }
    pub(super) fn prepare(
        &mut self,
        store: &Store,
        effects: &mut impl Effects,
        team: &Team,
        member: &Member,
        parent: Option<SessionKey>,
        now: i64,
    ) -> Result<bool> {
        let (choice, model, effort, instructions) = role(team, member)?;
        let accounts = effects.accounts(&member.agent)?;
        if accounts.len() > 200 {
            return Err(invalid("Account read exceeds its bound"));
        }
        let taken = taken(store, self.ledger(), &team.id)?;
        let parent_session = if parent.is_some() {
            self.ledger()
                .get(&team.id, &team.lead)?
                .and_then(|r| r.observed)
        } else {
            None
        };
        let mut row = Row::new(team, &member.id, &self.epoch, State::WorktreeIntent);
        match pick_account(&member.agent, choice, &accounts, &taken) {
            Ok(account) => {
                row.plan = Some(Plan {
                    id: uuid::Uuid::new_v4().to_string(),
                    team: team.id.clone(),
                    person: team.owner.clone(),
                    machine: team.home.clone(),
                    project: team.project.clone(),
                    member: member.id.clone(),
                    role: member.role.clone(),
                    agent: member.agent.clone(),
                    account,
                    model: model.into(),
                    effort: effort.into(),
                    instructions: instructions.into(),
                    base_commit: team.base_commit.clone(),
                    placement: effects.placement(&team.project, &row.original)?,
                    parent,
                    parent_session,
                    launch_id: row.original.clone(),
                })
            }
            Err(p) => {
                row.state = State::Blocked;
                row.problem = Some(p);
            }
        }
        if self.ledger().insert(&row, now)? && row.state != State::Blocked {
            self.admitted = Some((row.team.clone(), row.original));
        }
        Ok(true)
    }
}

fn role<'a>(
    team: &'a Team,
    member: &Member,
) -> Result<(&'a AccountChoice, &'a str, &'a str, &'a str)> {
    if member.id == team.lead {
        let spec = &team.bundle.template.lead;
        return Ok((&spec.account, &spec.model, &spec.effort, &spec.rules));
    }
    let role = team
        .bundle
        .roles
        .iter()
        .find(|r| r.id == member.role)
        .ok_or_else(|| invalid("Member role is missing"))?;
    Ok((&role.account, &role.model, &role.effort, &role.prompt))
}
fn taken(store: &Store, ledger: &mut Ledger, team: &str) -> Result<BTreeMap<String, u32>> {
    let mut taken = BTreeMap::new();
    let mut cursor = None;
    for _ in 0..2 {
        let page = store.members(team, 200, cursor.as_ref())?;
        for m in page.rows {
            if m.state == MemberState::Removed {
                continue;
            }
            let account = if m.account.is_empty() {
                ledger
                    .get(team, &m.id)?
                    .and_then(|r| r.plan)
                    .map(|p| p.account)
                    .unwrap_or_default()
            } else {
                m.account
            };
            if !account.is_empty() {
                *taken.entry(account).or_default() += 1;
            }
        }
        cursor = page.next;
        if cursor.is_none() {
            return Ok(taken);
        }
    }
    Err(invalid("Member read exceeds its bound"))
}
