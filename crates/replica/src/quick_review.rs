//! Quick review decisions are append-only and name the report they reviewed.
use crate::{Actor, Error, Result, Stage, TaskDoc, TaskView};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Disposition {
    Ready {
        evidence: String,
    },
    Check {
        criterion: String,
        assignee: Assignee,
    },
    Rework {
        defect: String,
    },
    Parked {
        reopen_when: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignee {
    pub machine: String,
    pub session_id: String,
}

impl Disposition {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ready { .. } => "ready",
            Self::Check { .. } => "check",
            Self::Rework { .. } => "rework",
            Self::Parked { .. } => "parked",
        }
    }

    fn text(&self) -> &str {
        match self {
            Self::Ready { evidence } => evidence,
            Self::Check { criterion, .. } => criterion,
            Self::Rework { defect } => defect,
            Self::Parked { reopen_when } => reopen_when,
        }
    }

    fn validate(&self) -> Result<()> {
        let text = self.text();
        if text.trim().is_empty()
            || text.len() > 1024
            || text
                .chars()
                .any(|c| c != '\n' && c != '\t' && c.is_control())
        {
            return Err(Error::new(
                "Review evidence or criteria must use 1–1024 bytes of text.",
            ));
        }
        if let Self::Check { assignee, .. } = self {
            let valid_id = assignee
                .session_id
                .split_once(':')
                .is_some_and(|(kind, id)| {
                    !id.is_empty()
                        && match kind {
                            "launch_id" => crate::ids::is_uuid(id),
                            "run_id" => true,
                            _ => false,
                        }
                });
            if !crate::ids::is_uuid(&assignee.machine)
                || !valid_id
                || assignee.session_id.len() > 200
                || assignee.session_id.chars().any(char::is_control)
            {
                return Err(Error::new(
                    "A targeted check needs one exact computer and session identity.",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub report_id: String,
    pub disposition: Disposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Team,
    Unresolved,
    Unclassified,
    Ready,
    Check,
    Rework,
    Parked,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Unresolved => "unresolved",
            Self::Unclassified => "unclassified",
            Self::Ready => "ready",
            Self::Check => "check",
            Self::Rework => "rework",
            Self::Parked => "parked",
        }
    }
}

fn eligible_report(view: &TaskView) -> Option<usize> {
    let report = view.review_report()?;
    // A legacy delivery receipt may remain unsettled without a managed
    // execution. It cannot authorize a new decision or a second offer.
    let unfinished = view.log.iter().any(|entry| {
        entry.kind == "message"
            && entry.at >= view.stage_at
            && entry.delivery.as_ref().is_some_and(|delivery| {
                delivery.mode == "reopen"
                    && matches!(delivery.state.as_str(), "reserved" | "uncertain")
            })
    });
    (!unfinished).then_some(report)
}

fn heads<'a>(view: &'a TaskView, ids: &'a [String], report: &str) -> Vec<(&'a str, &'a Decision)> {
    let mut candidates = Vec::new();
    let mut superseded = HashSet::new();
    for (entry, id) in view.log.iter().zip(ids) {
        if let Some(decision) = entry
            .quick_review
            .as_ref()
            .filter(|d| d.report_id == report)
        {
            superseded.extend(decision.supersedes.iter().map(String::as_str));
            candidates.push((id.as_str(), decision));
        }
    }
    candidates.retain(|(id, _)| !superseded.contains(id));
    candidates
}

pub(crate) fn project(view: &mut TaskView, ids: &[String]) {
    if view.stage != Stage::Review {
        return;
    }
    if view.review_lane() == Some("team") {
        view.quick_review_status = Some(Status::Team);
        return;
    }
    let Some(report) = eligible_report(view).and_then(|at| ids.get(at)) else {
        view.quick_review_status = Some(Status::Unresolved);
        return;
    };
    let current = match heads(view, ids, report).as_slice() {
        [] => None,
        [(_, decision)] => Some(decision.disposition.clone()),
        _ => {
            view.quick_review_status = Some(Status::Unresolved);
            return;
        }
    };
    view.quick_review_status = Some(match &current {
        None => Status::Unclassified,
        Some(Disposition::Ready { .. }) => Status::Ready,
        Some(Disposition::Check { .. }) => Status::Check,
        Some(Disposition::Rework { .. }) => Status::Rework,
        Some(Disposition::Parked { .. }) => Status::Parked,
    });
    view.review_report_id = Some(report.clone());
    view.quick_review = current;
}

impl TaskDoc {
    pub fn check_quick_review(
        &self,
        revision: &str,
        report: &str,
    ) -> Result<(TaskView, Vec<String>)> {
        if self.revision() != revision {
            return Err(Error::new(
                "The task changed. Refresh its report before reviewing.",
            ));
        }
        let (view, ids) = self.view_with_history_ids()?;
        if view.review_report_id.as_deref() != Some(report) {
            return Err(Error::new(
                "Only the exact settled ordinary task report can be reviewed.",
            ));
        }
        Ok((view, ids))
    }

    pub fn set_quick_review(
        &mut self,
        revision: &str,
        report: &str,
        disposition: &Disposition,
        actor: &Actor,
        now: i64,
    ) -> Result<()> {
        disposition.validate()?;
        let (view, ids) = self.check_quick_review(revision, report)?;
        let current = heads(&view, &ids, report);
        if view.quick_review.as_ref() == Some(disposition) {
            return Ok(());
        }
        let decision = Decision {
            report_id: report.into(),
            disposition: disposition.clone(),
            supersedes: current.iter().map(|(id, _)| (*id).to_owned()).collect(),
        };
        self.record_quick_review(&decision, actor, now)
    }

    pub fn accept_quick_review(
        &mut self,
        revision: &str,
        report: &str,
        actor: &Actor,
        now: i64,
    ) -> Result<()> {
        let (view, _) = self.check_quick_review(revision, report)?;
        if !matches!(view.quick_review, Some(Disposition::Ready { .. })) {
            return Err(Error::new("The current report is not ready to accept."));
        }
        if let Actor::Agent(executor) = actor {
            if executor.session_id.is_empty() || view.session.as_ref() != Some(executor) {
                return Err(Error::new(
                    "Only the person or this task's verified executor can accept it.",
                ));
            }
        }
        self.set_stage(Stage::Done, actor, now)
    }

    pub(crate) fn quick_review_log(decision: &Decision) -> String {
        format!(
            "{}: {}",
            decision.disposition.kind(),
            decision.disposition.text()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewTask, SessionKey, Target};

    fn reported() -> (TaskDoc, Actor, String) {
        let owner = Actor::Agent(SessionKey {
            machine: "00000000-0000-4000-8000-000000000002".into(),
            session: "codex/original".into(),
            session_id: "run_id:original".into(),
        });
        let mut doc = TaskDoc::create(NewTask {
            project: "example",
            id: "T-abcdef",
            text: "Verify the result",
            stage: Stage::Todo,
            target: Target::Any,
            author: owner.clone(),
            rank: "V",
            now: 1,
        })
        .unwrap();
        doc.record_start(&owner, 2).unwrap();
        assert!(doc.record_report("Observed result", &owner, 3).unwrap());
        let report = doc.view().unwrap().review_report_id.unwrap();
        (doc, owner, report)
    }

    #[test]
    fn conflicting_dispositions_are_visible_in_history_without_authorizing_acceptance() {
        let (mut left, _, report) = reported();
        let mut right = TaskDoc::from_bytes(&left.snapshot().unwrap()).unwrap();
        let revision = left.revision();
        let ready = Disposition::Ready {
            evidence: "Observed the exact result".into(),
        };
        let parked = Disposition::Parked {
            reopen_when: "Find the missing trace".into(),
        };
        left.set_quick_review(&revision, &report, &ready, &Actor::Person, 4)
            .unwrap();
        right
            .set_quick_review(&revision, &report, &parked, &Actor::Person, 5)
            .unwrap();
        left.import(&right.snapshot().unwrap()).unwrap();

        let view = left.view().unwrap();
        assert_eq!(view.quick_review_status, Some(Status::Unresolved));
        assert_eq!(
            view.log.iter().filter(|e| e.quick_review.is_some()).count(),
            2
        );
        assert_eq!(view.review_report_id, None);
        assert!(left
            .accept_quick_review(&left.revision(), &report, &Actor::Person, 6)
            .is_err());
        assert_eq!(left.stage().unwrap(), Stage::Review);
    }

    #[test]
    fn ready_preserves_exact_executor_and_person_authority() {
        let (mut doc, owner, report) = reported();
        let ready = Disposition::Ready {
            evidence: "Observed the exact result".into(),
        };
        doc.set_quick_review(&doc.revision(), &report, &ready, &Actor::Person, 4)
            .unwrap();
        let Actor::Agent(mut other) = owner.clone() else {
            unreachable!()
        };
        other.session_id = "run_id:changed".into();
        let revision = doc.revision();
        assert!(doc
            .accept_quick_review(&revision, &report, &Actor::Agent(other), 5)
            .is_err());
        assert_eq!(doc.revision(), revision);
        doc.accept_quick_review(&revision, &report, &owner, 6)
            .unwrap();
        assert_eq!(doc.stage().unwrap(), Stage::Done);
    }
}
