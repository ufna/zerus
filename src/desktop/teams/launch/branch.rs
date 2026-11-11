use super::*;

impl Runtime {
    pub(super) fn branch(
        &mut self,
        store: &mut Store,
        effects: &mut impl Effects,
        team: &Team,
        now: i64,
    ) -> Result<bool> {
        let Some(mut row) = self.ledger().get(&team.id, BRANCH)? else {
            let mut row = Row::new(team, BRANCH, &self.epoch, State::BranchIntent);
            let placement = effects.placement(&team.project, &row.original)?;
            let plan = BranchPlan::of(team, placement);
            let exists = effects.branch_exists(&plan)?;
            row.state = if exists {
                State::BranchDone
            } else {
                State::BranchIntent
            };
            row.branch = Some(plan);
            if self.ledger().insert(&row, now)? && !exists {
                self.admitted = Some((row.team.clone(), row.original));
            }
            return Ok(exists);
        };
        if row
            .branch
            .as_ref()
            .is_none_or(|p| p != &BranchPlan::of(team, p.placement.clone()))
        {
            self.block(&mut row, invalid("Integration branch plan changed"), now)?;
        }
        match row.state {
            State::BranchDone => Ok(true),
            State::BranchReady => {
                row.state = State::BranchIntent;
                row.original = uuid::Uuid::new_v4().to_string();
                row.epoch = self.epoch.clone();
                self.save(&mut row, now)?;
                self.admitted = Some((row.team.clone(), row.original));
                Ok(false)
            }
            State::Blocked => {
                self.card(store, &mut row, now)?;
                Ok(false)
            }
            State::BranchIntent if self.owns(&row) => {
                match effects.create_branch(row.branch.as_ref().expect("checked above")) {
                    Effect::Submitted(()) => {
                        row.state = State::BranchDone;
                        self.admitted = None;
                    }
                    Effect::NotStarted => {
                        self.admitted = None;
                        row.state = State::BranchReady;
                    }
                    Effect::Refused(p) => {
                        self.block(&mut row, p, now)?;
                        self.card(store, &mut row, now)?;
                        return Ok(false);
                    }
                    Effect::Unknown(p) => {
                        self.block(&mut row, as_uncertain(p), now)?;
                        self.card(store, &mut row, now)?;
                        return Ok(false);
                    }
                }
                self.save(&mut row, now)?;
                Ok(row.state == State::BranchDone)
            }
            _ => {
                self.block(
                    &mut row,
                    uncertain("Integration branch creation was interrupted"),
                    now,
                )?;
                self.card(store, &mut row, now)?;
                Ok(false)
            }
        }
    }
}
