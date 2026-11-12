use super::*;

impl Runtime {
    pub(super) fn card(&mut self, store: &mut Store, row: &mut Row, now: i64) -> Result<()> {
        let mut team = store
            .team(&row.team)?
            .ok_or_else(|| invalid("Team disappeared"))?;
        if row.card_done {
            // An open card is kept only while it lists what something would
            // read now: when the row's state has changed that, the card is
            // closed and the current one opened. A list never changes in place.
            let replaced = match row.card_id {
                Some(id) => store
                    .card(&row.team, id)?
                    .is_some_and(|c| c.state == "open" && c.decisions != decisions(row)),
                None => false,
            };
            if !replaced {
                return Ok(());
            }
            let card = row.card_id.expect("read above");
            apply(
                store,
                &team,
                Operation::CloseCard {
                    card,
                    reason: "The launch's state changed what this card can take.".into(),
                },
                now,
            )?;
            row.card_id = None;
            row.card_request = None;
            row.card_done = false;
            self.save(row, now)?;
            team = store
                .team(&row.team)?
                .ok_or_else(|| invalid("Team disappeared"))?;
        }
        if row.card_request.is_none() {
            let problem = row
                .problem
                .as_ref()
                .ok_or_else(|| invalid("Blocked launch has no reason"))?;
            let detail = cut(&problem.message, 2048);
            let timing = if row.state.waits_on_backoff() {
                if problem.code == code::NO_ACCOUNT_AVAILABLE && !problem.refs.is_empty() {
                    " Automatic retry follows that account window reset. Choosing Retry checks sooner, on the next service pass (60-second safety interval)."
                } else {
                    " Automatic retry uses a backoff from one to fifteen minutes. Choosing Retry checks sooner, on the next service pass (60-second safety interval)."
                }
            } else {
                ""
            };
            let subject = if row.member == crate::desktop::teams::ledger::BUDGET {
                "Team member creation".into()
            } else {
                format!("Member {}", row.member)
            };
            let text = format!(
                "{}: {}. {} {}{}",
                subject,
                problem.code,
                detail,
                problem_refs(problem),
                timing
            );
            row.card_request = Some(Request {
                request_id: uuid::Uuid::new_v4().to_string(),
                team: row.team.clone(),
                expected_revision: team.revision,
                operation: Operation::OpenCard {
                    kind: if row.member == crate::desktop::teams::ledger::BUDGET {
                        "launch_capacity"
                    } else if problem.code == code::DELIVERY_UNCERTAIN {
                        "launch_uncertain"
                    } else {
                        "launch_blocked"
                    }
                    .into(),
                    text: cut(&text, 3072).into(),
                    node: None,
                    decisions: Some(decisions(row)),
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
            Ok(reply) => {
                row.card_done = true;
                row.card_id = reply.item;
            }
            Err(p) if p.code == code::STALE_REVISION => row.card_request = None,
            Err(p) => return Err(p),
        }
        self.save(row, now)
    }
}

/// The decisions a launch card lists: retry, which the budget row reads on
/// every pass and a member row while it waits on backoff; otherwise nothing
/// reads a decision, and the card takes acknowledged alone.
fn decisions(row: &Row) -> Vec<String> {
    if row.member == crate::desktop::teams::ledger::BUDGET || row.state.waits_on_backoff() {
        vec!["retry".to_owned()]
    } else {
        vec!["acknowledged".to_owned()]
    }
}

fn problem_refs(problem: &Problem) -> String {
    if crate::desktop::teams::ledger::is_budget(problem) {
        return String::new();
    }
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

pub(super) fn cut(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
