//! Report-bound technical review facts. Local caller and policy admission
//! happen in the CLI before this module receives an authenticated reviewer.
use crate::quick_review::{self, Assignee, Disposition, Status};
use crate::{Actor, Error, Result, SessionKey, TaskDoc, TaskView};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Assigned,
    ReviewerUnavailable,
    Reserved,
    Notified,
    Refused,
    Uncertain,
    Verified,
    LegacyVerified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audit {
    pub policy: String,
    pub report_id: String,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

pub struct Candidate {
    pub report_id: String,
    pub reporter: Actor,
    pub criterion: String,
    pub decision: Option<(String, Actor, Disposition)>,
    pub audit: Vec<Audit>,
}

fn candidate(view: &TaskView, ids: &[String]) -> Option<Candidate> {
    if view.review_lane() != Some("technical")
        || view.quick_review_status == Some(Status::Unresolved)
    {
        return None;
    }
    let index = quick_review::eligible_report(view)?;
    let report_id = ids.get(index)?;
    let latest_return = view.log.iter().rposition(|entry| {
        entry.kind == "resumed"
            || entry.kind == "stage" && matches!(entry.text.as_str(), "todo" | "backlog")
    });
    // Older review assignments store the owner's decision in the history text.
    // A personal route in this cycle (including before the report) is a veto.
    if view
        .log
        .iter()
        .skip(latest_return.map_or(0, |i| i + 1))
        .any(|entry| {
            entry.kind == "team"
                || entry.kind == "review_assignment" && entry.text.starts_with("person: ")
        })
    {
        return None;
    }
    let decision = match quick_review::heads(view, ids, report_id).as_slice() {
        [] => None,
        [(id, decision)] => {
            let actor = view
                .log
                .get(ids.iter().position(|key| key == id)?)?
                .actor
                .clone();
            Some((id.to_string(), actor, decision.disposition.clone()))
        }
        _ => return None,
    };
    Some(Candidate {
        report_id: report_id.clone(),
        reporter: view.log[index].actor.clone(),
        criterion: view.review_assignment.as_ref()?.criterion.clone(),
        decision,
        audit: view
            .log
            .iter()
            .filter_map(|entry| entry.technical_review.as_ref())
            .filter(|audit| &audit.report_id == report_id)
            .cloned()
            .collect(),
    })
}

impl Candidate {
    pub fn independent(&self, reviewer: &SessionKey) -> bool {
        !reviewer.session_id.is_empty()
            && matches!(&self.reporter, Actor::Agent(reporter)
                if !reporter.session_id.is_empty() && reporter != reviewer)
    }
}

impl TaskDoc {
    pub fn technical_candidate(&self) -> Result<Option<Candidate>> {
        let (view, ids) = self.view_with_history_ids()?;
        Ok(candidate(&view, &ids))
    }

    pub fn route_technical_review(
        &mut self,
        report: &str,
        policy: &str,
        reviewer: &SessionKey,
        now: i64,
    ) -> Result<()> {
        let candidate = self
            .technical_candidate()?
            .filter(|c| c.report_id == report && c.decision.is_none() && c.independent(reviewer))
            .ok_or_else(|| Error::new("The technical report cannot be assigned."))?;
        let check = Disposition::Check {
            criterion: candidate.criterion,
            assignee: Assignee {
                machine: reviewer.machine.clone(),
                session_id: reviewer.session_id.clone(),
            },
        };
        self.set_quick_review(
            &self.revision(),
            report,
            &check,
            Some(&reviewer.session),
            &Actor::Agent(reviewer.clone()),
            now,
        )?;
        let decision_id = self
            .technical_candidate()?
            .and_then(|c| c.decision.map(|d| d.0))
            .ok_or_else(|| Error::new("The assigned technical review could not be read back."))?;
        self.record_technical_review(
            &Audit {
                policy: policy.to_owned(),
                report_id: report.to_owned(),
                action: Action::Assigned,
                decision_id: Some(decision_id),
                request_id: None,
                notice: None,
            },
            reviewer,
            "Technical review assigned by project policy.",
            false,
            now,
        )
    }

    pub fn complete_technical_review(
        &mut self,
        revision: &str,
        mut audit: Audit,
        reviewer: &SessionKey,
        evidence: &str,
        now: i64,
    ) -> Result<()> {
        self.check_quick_review(revision, &audit.report_id)?;
        let c = self
            .technical_candidate()?
            .filter(|c| c.independent(reviewer))
            .ok_or_else(|| Error::new("This report has no independent technical review."))?;
        let (decision, author, disposition) = c
            .decision
            .ok_or_else(|| Error::new("The exact technical review is missing."))?;
        if audit.decision_id.as_deref() != Some(&decision)
            || author != Actor::Agent(reviewer.clone())
        {
            return Err(Error::new("The technical review identity changed."));
        }
        let ready = match (&audit.action, disposition) {
            (
                Action::Verified,
                Disposition::Check {
                    criterion,
                    assignee,
                },
            ) if criterion == c.criterion
                && assignee.machine == reviewer.machine
                && assignee.session_id == reviewer.session_id
                && c.audit.iter().any(|entry| {
                    entry.action == Action::Assigned
                        && entry.policy == audit.policy
                        && entry.decision_id == audit.decision_id
                }) =>
            {
                Disposition::Ready {
                    evidence: evidence.to_owned(),
                }
            }
            (Action::LegacyVerified, ready @ Disposition::Ready { .. }) => ready,
            _ => {
                return Err(Error::new(
                    "The current review outcome cannot auto-complete.",
                ))
            }
        };
        ready.validate()?;
        if audit.action == Action::Verified {
            self.set_quick_review(
                &self.revision(),
                &audit.report_id,
                &ready,
                None,
                &author,
                now,
            )?;
            audit.decision_id = self
                .technical_candidate()?
                .and_then(|c| c.decision.map(|d| d.0));
        }
        let Disposition::Ready { evidence } = ready else {
            unreachable!()
        };
        self.record_technical_review(
            &audit,
            reviewer,
            &format!("Automatically accepted verified technical evidence: {evidence}"),
            true,
            now,
        )
    }

    /// The outbox records a reservation before any provider input. Later
    /// passes may observe that reservation but must never offer it again.
    pub fn technical_notice_candidate(
        &self,
        audit: &Audit,
        reviewer: &SessionKey,
    ) -> Result<Candidate> {
        let candidate = self
            .technical_candidate()?
            .ok_or_else(|| Error::new("The technical report is no longer eligible."))?;
        if candidate.report_id != audit.report_id
            || !candidate.audit.iter().any(|a| a.action == Action::Assigned
                && a.policy == audit.policy && a.decision_id == audit.decision_id)
            || !candidate.decision.as_ref().is_some_and(|(id, actor, kind)| {
                audit.decision_id.as_ref() == Some(id)
                    && actor == &Actor::Agent(reviewer.clone())
                    && matches!(kind, Disposition::Check { criterion, .. } if criterion == &candidate.criterion)
            })
        {
            return Err(Error::new("The assigned technical review changed."));
        }
        Ok(candidate)
    }

    pub fn record_technical_notice(
        &mut self,
        audit: &Audit,
        reviewer: &SessionKey,
        message: &str,
        now: i64,
    ) -> Result<()> {
        self.technical_notice_candidate(audit, reviewer)?;
        self.record_technical_review(audit, reviewer, message, false, now)
    }
}
