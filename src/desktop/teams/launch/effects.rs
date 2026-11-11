use super::*;

impl Runtime {
    pub(super) fn worktree(
        &mut self,
        effects: &mut impl Effects,
        row: &mut Row,
        now: i64,
    ) -> Result<bool> {
        let plan = row
            .plan
            .as_ref()
            .ok_or_else(|| invalid("Launch plan is missing"))?;
        match effects.worktree(plan, &row.original) {
            Effect::Submitted(tree)
                if !tree.path.is_empty() && !tree.ownership_nonce.is_empty() =>
            {
                row.worktree = Some(tree);
                row.state = State::WorktreeDone;
            }
            Effect::Submitted(_) => {
                self.block(row, uncertain("Worktree ownership is unconfirmed"), now)?;
                return Ok(true);
            }
            Effect::NotStarted => {
                row.state = State::WorktreeReady;
                self.admitted = None;
            }
            Effect::Refused(p) => {
                self.block(row, p, now)?;
                return Ok(true);
            }
            Effect::Unknown(p) => {
                self.block(row, as_uncertain(p), now)?;
                return Ok(true);
            }
        }
        self.save(row, now)?;
        Ok(row.state != State::WorktreeReady)
    }
    pub(super) fn launch(
        &mut self,
        effects: &mut impl Effects,
        row: &mut Row,
        now: i64,
    ) -> Result<bool> {
        let plan = row
            .plan
            .as_ref()
            .ok_or_else(|| invalid("Launch plan is missing"))?;
        let tree = row
            .worktree
            .as_ref()
            .ok_or_else(|| invalid("Worktree is missing"))?;
        match effects.launch(plan, tree, &row.original) {
            Effect::Submitted(()) => row.state = State::Submitted,
            Effect::NotStarted => {
                row.state = State::LaunchReady;
                self.admitted = None;
            }
            Effect::Refused(p) => {
                self.block(row, p, now)?;
                return Ok(true);
            }
            Effect::Unknown(p) => {
                self.block(row, as_uncertain(p), now)?;
                return Ok(true);
            }
        }
        self.save(row, now)?;
        Ok(row.state != State::LaunchReady)
    }
    pub(super) fn bind(
        &mut self,
        store: &mut Store,
        effects: &mut impl Effects,
        team: &Team,
        row: &mut Row,
        now: i64,
    ) -> Result<bool> {
        let plan = row
            .plan
            .as_ref()
            .ok_or_else(|| invalid("Launch plan is missing"))?;
        let observed = effects.observe(plan)?;
        let Some(observed) = observed.filter(|s| {
            s.launch_id == plan.launch_id
                && s.machine == plan.machine
                && s.agent == plan.agent
                && s.account == plan.account
                && Some(&s.worktree) == row.worktree.as_ref()
        }) else {
            self.block(
                row,
                uncertain("No exact session binding appeared for the original launch ID"),
                now,
            )?;
            return Ok(true);
        };
        let key = SessionKey {
            machine: plan.machine.clone(),
            session_id: format!("launch_id:{}", plan.launch_id),
        };
        let member = store
            .member(&team.id, &row.member)?
            .ok_or_else(|| invalid("Member disappeared"))?;
        if member.session.as_ref() != Some(&key) {
            let tree = row
                .worktree
                .as_ref()
                .ok_or_else(|| invalid("Worktree is missing"))?;
            apply(
                store,
                team,
                Operation::SetMemberSession {
                    member: row.member.clone(),
                    session: key,
                    account: plan.account.clone(),
                    worktree: tree.path.clone(),
                    ownership_nonce: tree.ownership_nonce.clone(),
                },
                now,
            )?;
        }
        row.observed = Some(observed);
        row.state = State::Bound;
        self.save(row, now)?;
        self.admitted = None;
        Ok(true)
    }
    pub(super) fn parent(
        &mut self,
        effects: &mut impl Effects,
        row: &mut Row,
        now: i64,
    ) -> Result<bool> {
        let plan = row
            .plan
            .as_ref()
            .ok_or_else(|| invalid("Launch plan is missing"))?;
        if plan.parent.is_none() {
            row.state = State::Done;
        } else if row.state == State::Bound {
            row.state = State::ParentIntent;
            row.original = uuid::Uuid::new_v4().to_string();
            row.epoch = self.epoch.clone();
            self.admitted = Some((row.team.clone(), row.original.clone()));
        } else {
            row.parent_result = Some(if self.owns(row) {
                effects.parent(
                    plan,
                    row.observed
                        .as_ref()
                        .ok_or_else(|| invalid("Bound observation is missing"))?,
                    &row.original,
                )
            } else {
                "unknown_after_restart".into()
            });
            row.state = State::Done;
            self.admitted = None;
        }
        self.save(row, now)?;
        Ok(true)
    }
}
