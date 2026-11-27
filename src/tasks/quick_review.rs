use super::*;
use serde::Deserialize;
use zerus_replica::quick_review::{Disposition, Status};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Set {
        history_revision: String,
        report_id: String,
        disposition: Disposition,
    },
    Verify {
        history_revision: String,
        report_id: String,
        observation: String,
        evidence_reference: String,
    },
    Accept {
        history_revision: String,
        report_id: String,
    },
}

pub(super) fn read() -> Result<Request> {
    const LIMIT: u64 = 8192;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(fail)?;
    if bytes.len() as u64 > LIMIT {
        return Err(fail("Quick review request exceeds 8192 bytes."));
    }
    let request = serde_json::from_slice::<Request>(&bytes).map_err(fail)?;
    let (revision, report) = request.guard();
    validate_guard(revision, report)?;
    Ok(request)
}

fn validate_guard(revision: &str, report: &str) -> Result<()> {
    if revision.len() != 64
        || !revision
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(fail("Invalid expected task history revision."));
    }
    if report.is_empty() || report.len() > 512 || report.chars().any(char::is_control) {
        return Err(fail("Invalid expected task report ID."));
    }
    Ok(())
}

pub(super) fn raw_report<'a>(task: &TaskDoc, project: &str, report: &'a str) -> Result<&'a str> {
    report
        .strip_prefix(&history::scope(project, task.uuid()))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| fail("The report ID does not belong to this task."))
}

/// Scope the opaque report IDs exactly like paged task history entry IDs.
pub(super) fn scope_reports(view: &mut zerus_replica::TaskView) {
    if view.review_report_id.is_none() && !view.log.iter().any(|entry| entry.quick_review.is_some())
    {
        return;
    }
    let prefix = history::scope(&view.project, &view.uuid);
    if let Some(id) = &mut view.review_report_id {
        id.insert_str(0, &prefix);
    }
    for entry in &mut view.log {
        if let Some(audit) = &mut entry.technical_review {
            audit.report_id.insert_str(0, &prefix);
            if let Some(decision_id) = &mut audit.decision_id {
                decision_id.insert_str(0, &prefix);
            }
            // The retained transport envelope is available only to the
            // local reconciler. History exposes the original request ID.
            audit.notice = None;
        }
        if let Some(decision) = &mut entry.quick_review {
            decision.report_id.insert_str(0, &prefix);
            for id in &mut decision.supersedes {
                id.insert_str(0, &prefix);
            }
        }
    }
}

impl Request {
    fn guard(&self) -> (&str, &str) {
        match self {
            Self::Set {
                history_revision,
                report_id,
                ..
            }
            | Self::Verify { history_revision, report_id, .. }
            | Self::Accept {
                history_revision,
                report_id,
            } => (history_revision, report_id),
        }
    }

    pub(super) fn precheck(&self, task: &TaskDoc, project: &str) -> Result<()> {
        let (revision, report) = self.guard();
        task.check_quick_review(revision, raw_report(task, project, report)?)
            .map_err(fail)?;
        Ok(())
    }

    pub(super) fn assignee_name(&self, machines: &Machines) -> Result<Option<String>> {
        let Self::Set {
            disposition: Disposition::Check { assignee, .. },
            ..
        } = self
        else {
            return Ok(None);
        };
        if assignee.machine != machines.here {
            return Err(fail("Choose a review assignee on this computer. Peer sessions cannot be verified for quick review."));
        }
        if assignee.session_id.is_empty() {
            return Err(fail("The selected review assignee is unavailable."));
        }
        let key = SessionKey {
            machine: assignee.machine.clone(),
            session: String::new(),
            session_id: assignee.session_id.clone(),
        };
        machines
            .resolve(&key)
            .map(|matched| matched.session)
            .filter(|name| {
                !name.trim().is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
            })
            .map(Some)
            .ok_or_else(|| fail("The selected review assignee is unavailable."))
    }

    pub(super) fn apply(
        &self,
        config: &Config,
        task: &mut TaskDoc,
        project: &(String, String),
        actor: &Actor,
        assignee_name: Option<&str>,
    ) -> Result<(&'static str, &str)> {
        let (revision, report) = self.guard();
        let raw = raw_report(task, &project.0, report)?;
        match self {
            Self::Verify { observation, evidence_reference, .. } => {
                super::technical_review::verify(
                    config, task, revision, raw, actor, observation, evidence_reference,
                )?;
                Ok(("verify", report))
            }
            Self::Set { disposition, .. } => {
                task.set_quick_review(revision, raw, disposition, assignee_name, actor, now())
                    .map_err(fail)?;
                Ok(("set", report))
            }
            Self::Accept { .. } => {
                task.accept_quick_review(revision, raw, actor, now())
                    .map_err(fail)?;
                Ok(("accept", report))
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ReopenGuard<'a> {
    pub revision: &'a str,
    pub report: &'a str,
}

impl<'a> ReopenGuard<'a> {
    pub(super) fn parse(args: &'a [String], mode: &str) -> Result<Option<Self>> {
        match (
            option(args, "--expect-history-revision")?,
            option(args, "--expect-report-id")?,
        ) {
            (None, None) => Ok(None),
            (Some(revision), Some(report)) if mode == "reopen" => {
                validate_guard(revision, report)?;
                Ok(Some(Self { revision, report }))
            }
            _ => Err(fail(
                "A reviewed reopen needs both expected history revision and report ID.",
            )),
        }
    }

    pub(super) fn check(self, task: &TaskDoc, project: &str) -> Result<()> {
        let raw = raw_report(task, project, self.report)?;
        let (view, _) = task.check_quick_review(self.revision, raw).map_err(fail)?;
        if view.quick_review_status != Some(Status::Rework) {
            return Err(fail("The current report does not request rework."));
        }
        Ok(())
    }
}
