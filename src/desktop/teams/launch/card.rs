use super::*;

impl Runtime {
    pub(super) fn card(&mut self, store: &mut Store, row: &mut Row, now: i64) -> Result<()> {
        if row.card_done {
            return Ok(());
        }
        let team = store
            .team(&row.team)?
            .ok_or_else(|| invalid("Team disappeared"))?;
        if row.card_request.is_none() {
            let problem = row
                .problem
                .as_ref()
                .ok_or_else(|| invalid("Blocked launch has no reason"))?;
            let detail: String = problem.message.chars().take(4096).collect();
            let text = format!(
                "Member {}: {}. {} {}",
                row.member,
                problem.code,
                detail,
                problem_refs(problem)
            );
            row.card_request = Some(Request {
                request_id: uuid::Uuid::new_v4().to_string(),
                team: row.team.clone(),
                expected_revision: team.revision,
                operation: Operation::OpenCard {
                    kind: if problem.code == code::DELIVERY_UNCERTAIN {
                        "launch_uncertain"
                    } else {
                        "launch_blocked"
                    }
                    .into(),
                    text,
                    node: None,
                },
            });
            self.save(row, now)?;
        }
        let request = row.card_request.as_ref().expect("reserved above");
        let actor = Actor::Service {
            person: row.person.clone(),
            team: row.team.clone(),
        };
        match ops::apply(store, &actor, request, now) {
            Ok(_) => row.card_done = true,
            Err(p) if p.code == code::STALE_REVISION => row.card_request = None,
            Err(p) => return Err(p),
        }
        self.save(row, now)
    }
}

fn problem_refs(problem: &Problem) -> String {
    if problem.code == code::NO_ACCOUNT_AVAILABLE {
        if let Some((seconds, time)) = problem
            .refs
            .first()
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0).map(|t| (s, t)))
        {
            return format!(
                "Next account window resets at {} ({seconds}).",
                time.to_rfc3339()
            );
        }
    }
    problem.refs.join(", ")
}
