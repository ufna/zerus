//! A local, owner-enrolled project technical reviewer. This file is not
//! replicated and cannot be installed with generic project settings.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zerus_replica::{
    quick_review::Disposition,
    technical_review::{Action, Audit, Candidate},
};

const FILE: &str = "technical-review-policy.json";
const LIMIT: u64 = 16 * 1024;
const BATCH: usize = 8;
const SUPPORTED: bool = cfg!(any(target_os = "linux", target_os = "macos"));

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
    task_uuid: String,
    history_revision: String,
    report_id: String,
    decision_id: String,
    reviewer_session_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    id: String,
    project: String,
    machine: String,
    enabled: bool,
    at: i64,
    authorized_by: Actor,
    person_request: Option<zerus_replica::PersonRequestRef>,
    reviewers: Vec<SessionKey>,
    legacy: Vec<Legacy>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Configure {
    Status,
    Enable {
        reviewers: Vec<String>,
        #[serde(default)]
        legacy: Vec<Legacy>,
    },
    Disable,
}

fn bounded(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(fail)?;
    if bytes.len() as u64 > LIMIT {
        return Err(fail("Technical review policy exceeds its byte limit."));
    }
    Ok(bytes)
}

fn policy(config: &Config) -> Result<Option<Policy>> {
    let path = config.state.join(FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= LIMIT => m,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => return Err(fail("Technical review policy is unavailable.")),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(fail("Technical review policy is not private."));
        }
    }
    #[cfg(not(unix))]
    let _ = metadata;
    let parsed: Policy =
        serde_json::from_slice(&bounded(File::open(path).map_err(fail)?)?).map_err(fail)?;
    if !zerus_replica::ids::is_uuid(&parsed.id)
        || zerus_replica::command_word(&parsed.project).as_deref() != Some(parsed.project.as_str())
        || !zerus_replica::ids::is_uuid(&parsed.machine)
        || parsed.at <= 0
        || parsed.reviewers.len() > 4
        || parsed.legacy.len() > BATCH
        || !matches!(
            (&parsed.authorized_by, &parsed.person_request),
            (Actor::Person, None) | (Actor::Agent(_), Some(_))
        )
    {
        return Err(fail("Invalid technical review policy."));
    }
    Ok(Some(parsed))
}

fn save(config: &Config, policy: &Policy) -> Result<()> {
    let bytes = serde_json::to_vec(policy).map_err(fail)?;
    if bytes.len() as u64 > LIMIT {
        return Err(fail("Technical review policy exceeds its byte limit."));
    }
    let mut out = tempfile::NamedTempFile::new_in(&config.state).map_err(fail)?;
    out.write_all(&bytes).map_err(fail)?;
    out.as_file().sync_all().map_err(fail)?;
    crate::platform::persist(out, &config.state.join(FILE)).map_err(fail)?;
    crate::platform::sync_dir(&config.state).map_err(fail)
}

fn matches_policy(config: &Config, expected: &Policy) -> Result<bool> {
    Ok(policy(config)?.is_some_and(|current| current.enabled && current.id == expected.id))
}

pub(super) fn configure(config: &Config, args: &[String]) -> Result<Value> {
    if !SUPPORTED {
        return Err(fail(
            "Technical review policy is available on Linux and macOS.",
        ));
    }
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    let ["--project", project, "--json"] = words.as_slice() else {
        return Err(fail(
            "usage: hgs task technical-review-policy --project PROJECT --json",
        ));
    };
    let req: Configure =
        serde_json::from_slice(&bounded(std::io::stdin().lock())?).map_err(fail)?;
    let (project, _) = super::project(config, project)?;
    if zerus_replica::command_word(&project).as_deref() != Some(project.as_str()) {
        return Err(fail(
            "Technical review requires a stable command-safe project ID.",
        ));
    }
    if matches!(req, Configure::Status) {
        return Ok(json!({"policy": policy(config)?.filter(|p| p.project == project)}));
    }
    let machines = machines(config)?;
    let who = actor(&machines.here, &[], false)?;
    let request = person_request_for(
        &who,
        "Only the person or their verified request may enroll technical reviewers",
    )?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(config.state.join("technical-review-policy.lock"))
        .map_err(fail)?;
    let _lock = crate::platform::FileLock::exclusive(lock).map_err(fail)?;
    let previous = policy(config)?;
    if previous
        .as_ref()
        .is_some_and(|p| p.project != project || p.machine != machines.here)
    {
        return Err(fail(
            "This installation already owns technical review policy for another project.",
        ));
    }
    let store = store(config, &project)?;
    let deadline = person_deadline();
    let mut admitted = store
        .admit_until(deadline)
        .ok_or_else(|| fail("The project is busy."))?;
    let (enabled, reviewers, legacy) = match req {
        Configure::Enable { reviewers, legacy } => {
            if reviewers.is_empty() || reviewers.len() > 4 || legacy.len() > BATCH {
                return Err(fail(
                    "Enroll 1–4 local reviewers and at most eight exact legacy decisions.",
                ));
            }
            let mut found = Vec::new();
            for id in reviewers {
                let selected = machines
                    .resolve(&SessionKey {
                        machine: machines.here.clone(),
                        session: String::new(),
                        session_id: id,
                    })
                    .filter(|s| !s.session_id.is_empty() && s.session.starts_with("codex/"))
                    .ok_or_else(|| fail("Enroll an exact local Codex reviewer."))?;
                if found.contains(&selected) || live(&selected).is_none() {
                    return Err(fail("Each reviewer must be distinct and verified live."));
                }
                found.push(selected);
            }
            let mut uuids = std::collections::BTreeSet::new();
            for grant in &legacy {
                if !zerus_replica::ids::is_uuid(&grant.task_uuid) || !uuids.insert(&grant.task_uuid)
                {
                    return Err(fail("Each legacy grant must name one distinct task UUID."));
                }
                let task = admitted.load_complete(&grant.task_uuid).map_err(fail)?;
                let reviewer = found
                    .iter()
                    .find(|r| r.session_id == grant.reviewer_session_id)
                    .ok_or_else(|| fail("The legacy reviewer is not enrolled."))?;
                legacy_audit(&task, grant, reviewer, "")?;
            }
            (true, found, legacy)
        }
        Configure::Disable => (false, Vec::new(), Vec::new()),
        Configure::Status => unreachable!(),
    };
    let _authority = recheck_held(request.as_ref(), &project, deadline)?;
    let policy = Policy {
        id: uuid::Uuid::new_v4().to_string(),
        project,
        machine: machines.here,
        enabled,
        at: now(),
        authorized_by: who,
        person_request: request
            .as_ref()
            .map(crate::caller::PersonRequest::reference),
        reviewers,
        legacy,
    };
    save(config, &policy)?;
    Ok(json!({"policy": policy}))
}

fn live(reviewer: &SessionKey) -> Option<Value> {
    let record = crate::state::reference_details(&reviewer.session, "", "").ok()?;
    (sessions::identity(&record) == reviewer.session_id
        && record["process_state"] == "running"
        && record["run_id"].as_str().is_some_and(|s| !s.is_empty())
        && record["conversation_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty()))
    .then_some(record)
}

fn legacy_audit(
    task: &TaskDoc,
    grant: &Legacy,
    reviewer: &SessionKey,
    policy: &str,
) -> Result<Audit> {
    let report =
        quick_review::raw_report(task, &task.view().map_err(fail)?.project, &grant.report_id)?;
    let decision = quick_review::raw_report(
        task,
        &task.view().map_err(fail)?.project,
        &grant.decision_id,
    )?;
    let eligible = task.technical_candidate().map_err(fail)?.is_some_and(|c| {
        c.report_id == report
            && c.independent(reviewer)
            && matches!(c.decision, Some((id, Actor::Agent(actor), Disposition::Ready { .. }))
                if id == decision && &actor == reviewer)
    });
    if task.revision() != grant.history_revision || !eligible {
        return Err(fail(
            "The legacy grant must match the exact technical Ready report, decision and reviewer.",
        ));
    }
    Ok(Audit {
        policy: policy.into(),
        report_id: report.into(),
        action: Action::LegacyVerified,
        decision_id: Some(decision.into()),
        request_id: None,
        notice: None,
    })
}

pub(super) fn verify(
    config: &Config,
    task: &mut TaskDoc,
    revision: &str,
    report: &str,
    caller: &Actor,
    observation: &str,
    reference: &str,
) -> Result<()> {
    if observation.trim().is_empty() || reference.trim().is_empty() {
        return Err(fail(
            "Explain the observed pass and cite reproducible evidence.",
        ));
    }
    let p = policy(config)?
        .filter(|p| p.enabled && task.view().is_ok_and(|v| v.project == p.project))
        .ok_or_else(|| fail("No technical review policy is enrolled for this project."))?;
    let Actor::Agent(caller) = caller else {
        return Err(fail(
            "This automatic verdict must come from its enrolled reviewer.",
        ));
    };
    let reviewer = p
        .reviewers
        .iter()
        .find(|r| *r == caller)
        .ok_or_else(|| fail("This session is not an enrolled technical reviewer."))?;
    if live(caller).is_none() {
        return Err(fail(
            "The enrolled reviewer no longer has the exact live binding.",
        ));
    }
    let candidate = task
        .technical_candidate()
        .map_err(fail)?
        .ok_or_else(|| fail("No exact technical report is available."))?;
    let decision_id = candidate.decision.map(|d| d.0);
    task.complete_technical_review(
        revision,
        Audit {
            policy: p.id,
            report_id: report.into(),
            action: Action::Verified,
            decision_id,
            request_id: None,
            notice: None,
        },
        reviewer,
        &format!("{}\nEvidence: {}", observation.trim(), reference.trim()),
        now(),
    )
    .map_err(fail)
}

pub(crate) struct Worker {
    config: Config,
    due: Instant,
    cancel: Arc<AtomicBool>,
    running: Option<JoinHandle<Cursor>>,
    cursor: Cursor,
}

#[derive(Default)]
struct Cursor {
    policy: String,
    legacy: usize,
    settled: std::collections::BTreeSet<String>,
    anchor: Option<index::Anchor>,
}

impl Worker {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            due: Instant::now(),
            cancel: Arc::default(),
            running: None,
            cursor: Cursor::default(),
        }
    }

    pub(crate) fn poll(&mut self) {
        if !SUPPORTED
            || self.cancel.load(Ordering::Acquire)
            || !self.reap()
            || Instant::now() < self.due
        {
            return;
        }
        self.due = Instant::now() + Duration::from_secs(3);
        if !self.config.state.join(FILE).is_file() {
            return;
        }
        let config = self.config.clone();
        let cancel = self.cancel.clone();
        let mut cursor = std::mem::take(&mut self.cursor);
        self.running = thread::Builder::new()
            .name("technical-task-review".into())
            .spawn(move || {
                if let Err(error) = pass(&config, &cancel, &mut cursor) {
                    eprintln!("Technical task review: {}", error.message);
                }
                cursor
            })
            .ok();
    }

    fn reap(&mut self) -> bool {
        if self.running.as_ref().is_some_and(|job| !job.is_finished()) {
            return false;
        }
        if let Some(job) = self.running.take() {
            self.cursor = job.join().unwrap_or_default();
        }
        true
    }

    pub(crate) fn stop(&mut self) -> bool {
        self.cancel.store(true, Ordering::Release);
        self.reap()
    }
}

fn open(config: &Config, p: &Policy) -> Result<Option<ProjectStore>> {
    ProjectStore::open_within(
        &config.state.join("replica"),
        &p.project,
        Duration::from_millis(100),
    )
    .map_err(fail)
}

fn write_ready(config: &Config, p: &Policy, task: &str) -> Result<bool> {
    crate::swarm::fence::prepare_write(
        config,
        &crate::swarm::fence::Ssh(config),
        &p.project,
        task,
        person_deadline(),
    )
    .map(|admission| admission.is_ok())
    .map_err(fail)
}

fn reconcile_legacy(config: &Config, p: &Policy, grant: &Legacy) -> Result<bool> {
    let Some(reviewer) = p
        .reviewers
        .iter()
        .find(|r| r.session_id == grant.reviewer_session_id)
    else {
        return Ok(true);
    };
    let Some(store) = open(config, p)? else {
        return Ok(false);
    };
    let task = store.load(&grant.task_uuid).map_err(fail)?;
    if legacy_audit(&task, grant, reviewer, &p.id).is_err() {
        return Ok(true);
    }
    drop(store);
    if !write_ready(config, p, &grant.task_uuid)? {
        return Ok(false);
    }
    let Some(store) = open(config, p)? else {
        return Ok(false);
    };
    if !matches_policy(config, p)? {
        return Ok(false);
    }
    let Some(mut admitted) = store.admit_until(Instant::now() + Duration::from_millis(100)) else {
        return Ok(false);
    };
    let mut task = admitted.load_complete(&grant.task_uuid).map_err(fail)?;
    if let Ok(audit) = legacy_audit(&task, grant, reviewer, &p.id) {
        task.complete_technical_review(&grant.history_revision, audit, reviewer, "", now())
            .map_err(fail)?;
        admitted.save(&mut task).map_err(fail)?;
    }
    Ok(true)
}

fn pass(config: &Config, cancel: &AtomicBool, cursor: &mut Cursor) -> Result<()> {
    let Some(p) = policy(config)?.filter(|p| p.enabled) else {
        return Ok(());
    };
    if crate::swarm::try_local_node_id(config).map_err(fail)? != p.machine {
        return Err(fail(
            "Technical review policy belongs to a different computer.",
        ));
    }
    if cursor.policy != p.id {
        *cursor = Cursor {
            policy: p.id.clone(),
            ..Default::default()
        };
    }
    // Rotate independently of transient legacy failures, which never give a
    // different report authority. A restart reads the same bounded grants.
    for _ in 0..p.legacy.len().min(2) {
        if cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        let grant = &p.legacy[cursor.legacy];
        cursor.legacy = (cursor.legacy + 1) % p.legacy.len();
        if cursor.settled.contains(&grant.task_uuid) {
            continue;
        }
        match reconcile_legacy(config, &p, grant) {
            Ok(true) => {
                cursor.settled.insert(grant.task_uuid.clone());
            }
            Ok(false) => {}
            Err(error) => eprintln!("Technical legacy review left unchanged: {}", error.message),
        }
    }

    let Some(store) = open(config, &p)? else {
        return Ok(());
    };
    let filter = index::Scope::quick_review(
        Some(index::Scope::review(None, index::ReviewLane::Technical)),
        index::QuickReviewLane::Work,
    );
    let Some(mut index) = index::Index::open(&store) else {
        return Ok(());
    };
    let page = index
        .page_in(
            Stage::Review,
            BATCH,
            cursor.anchor.as_ref().unwrap_or(&index::Anchor::Start),
            Some(&filter),
        )
        .map_err(|_| fail("Cannot read the bounded technical review queue."))?;
    cursor.anchor = page.next.and_then(|_| page.rows.last()).and_then(|row| {
        Some(index::Anchor::After(
            row["rank"].as_str()?.into(),
            row["uuid"].as_str()?.into(),
        ))
    });
    drop(index);
    drop(store);
    for row in page.rows {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let Some(uuid) = row["uuid"].as_str() else {
            continue;
        };
        if row["stage_at"].as_i64().is_none_or(|at| at < p.at)
            || !matches!(
                row["quick_review_status"].as_str(),
                Some("check" | "unclassified")
            )
        {
            continue;
        }
        let Some(store) = open(config, &p)? else {
            continue;
        };
        let task = store.load(uuid).map_err(fail)?;
        if !task.technical_candidate().map_err(fail)?.is_some_and(|c| {
            c.decision.is_none()
                || c.audit
                    .iter()
                    .any(|a| a.policy == p.id && a.action == Action::Assigned)
        }) {
            continue;
        }
        drop(store);
        if !write_ready(config, &p, uuid)? {
            continue;
        }
        let Some(store) = open(config, &p)? else {
            continue;
        };
        if cancel.load(Ordering::Acquire) || !matches_policy(config, &p)? {
            break;
        }
        if let Some(notice) = prepare(&store, &p, uuid)? {
            drop(store);
            attempts::pause_at(&notice.task, "technical_review_notice");
            deliver(config, cancel, &p, notice)?;
            break;
        }
    }
    Ok(())
}

struct Notice {
    task: String,
    reviewer: SessionKey,
    audit: Audit,
    wire: Value,
}

fn prepare(store: &ProjectStore, p: &Policy, uuid: &str) -> Result<Option<Notice>> {
    let Some(mut admitted) = store.admit_until(Instant::now() + Duration::from_millis(100)) else {
        return Ok(None);
    };
    let mut task = admitted.load_complete(uuid).map_err(fail)?;
    if task.view().map_err(fail)?.stage_at < p.at {
        return Ok(None);
    }
    let Some(mut candidate) = task.technical_candidate().map_err(fail)? else {
        return Ok(None);
    };
    if candidate.decision.is_none() {
        let Some(reviewer) = p
            .reviewers
            .iter()
            .find(|reviewer| candidate.independent(reviewer))
        else {
            return Ok(None);
        };
        task.route_technical_review(&candidate.report_id, &p.id, reviewer, now())
            .map_err(fail)?;
        admitted.save(&mut task).map_err(fail)?;
        candidate = task
            .technical_candidate()
            .map_err(fail)?
            .ok_or_else(|| fail("Cannot read the routed technical review."))?;
    }
    let Candidate {
        report_id,
        decision,
        audit,
        ..
    } = candidate;
    let Some((decision_id, Actor::Agent(stored), Disposition::Check { .. })) = decision else {
        return Ok(None);
    };
    let Some(reviewer) = p.reviewers.iter().find(|reviewer| **reviewer == stored) else {
        return Ok(None);
    };
    let current = |a: &Audit| a.policy == p.id && a.decision_id.as_ref() == Some(&decision_id);
    if !audit
        .iter()
        .any(|a| current(a) && a.action == Action::Assigned)
    {
        return Ok(None);
    }
    if let Some(last) = audit
        .iter()
        .rev()
        .find(|a| current(a) && a.request_id.is_some())
    {
        if last.action == Action::Reserved {
            let mut settled = last.clone();
            let verified = last.notice.as_deref()
                .and_then(|original| serde_json::from_str::<Value>(original).ok())
                .is_some_and(|original| matches!(
                    crate::state::task_reply_lookup(&reviewer.session, &original),
                    Ok(crate::state::ReceiptLookup::Found(receipt)) if receipt["status"] == "submitted"
                ));
            let message = if verified {
                settled.action = Action::Notified;
                "Original review notice submission verified after restart."
            } else {
                settled.action = Action::Uncertain;
                "Review notice outcome is unknown. Use the assigned Review Check; the notice will not be repeated."
            };
            settled.notice = None;
            task.record_technical_notice(&settled, reviewer, message, now())
                .map_err(fail)?;
            admitted.save(&mut task).map_err(fail)?;
        }
        return Ok(None);
    }
    let mut draft = Audit {
        policy: p.id.clone(),
        report_id,
        action: Action::ReviewerUnavailable,
        decision_id: Some(decision_id),
        request_id: None,
        notice: None,
    };
    let Some(record) = live(reviewer) else {
        if !audit
            .iter()
            .any(|a| a.policy == p.id && a.action == Action::ReviewerUnavailable)
        {
            task.record_technical_notice(
                &draft,
                reviewer,
                "The assigned technical reviewer is unavailable. Check remains assigned.",
                now(),
            )
            .map_err(fail)?;
            admitted.save(&mut task).map_err(fail)?;
        }
        return Ok(None);
    };
    if record["activity"] != "idle" {
        return Ok(None);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let report = format!("{}{}", history::scope(&p.project, uuid), draft.report_id);
    let reference = format!("{}/{}", p.project, task.id());
    // The notice contains only a trusted routing instruction and opaque IDs.
    // The report and its evidence are read using the normal task boundary.
    let text = format!(
        "[Zerus technical review {reference}] An ordinary technical report is assigned to you. Read it with hgs task get {reference} --history-limit 12 --attachments metadata. Review the original request and the exact report {report} against the assigned criterion. If you independently observe that it passes, send op verify to hgs task quick-review {reference} --json with the exact current history_revision, this report_id, observation and evidence_reference. If a check still remains, leave the assignment in place. Record Rework or Parked with the real reason when appropriate. Never use a different report in response to this notice."
    );
    let mut wire = crate::state::prepare_task_reply(
        reviewer,
        "",
        &id,
        &text,
        &[],
        crate::state::Continuation::SameRun,
    )
    .map_err(fail)?;
    if wire["expected_run_id"] != record["run_id"]
        || wire["expected_conversation_id"] != record["conversation_id"]
    {
        return Ok(None);
    }
    wire["only_when_idle"] = json!(true);
    draft.action = Action::Reserved;
    draft.request_id = Some(id);
    draft.notice = Some(wire.to_string());
    task.record_technical_notice(
        &draft,
        reviewer,
        "Technical reviewer notice reserved.",
        now(),
    )
    .map_err(fail)?;
    admitted.save(&mut task).map_err(fail)?;
    Ok(Some(Notice {
        task: uuid.into(),
        reviewer: reviewer.clone(),
        audit: draft,
        wire,
    }))
}

fn deliver(config: &Config, cancel: &AtomicBool, p: &Policy, mut notice: Notice) -> Result<()> {
    // Take the project lock before the input writer can take the native
    // state lock. The callback releases it at the original admission point.
    let Some(store) = ProjectStore::open_within(
        &config.state.join("replica"),
        &p.project,
        Duration::from_millis(100),
    )
    .map_err(fail)?
    else {
        return Ok(());
    };
    let held = std::cell::RefCell::new(Some(store));
    let result =
        crate::state::technical_review_notice(&notice.reviewer.session, &notice.wire, &|| {
            let store = held
                .borrow_mut()
                .take()
                .ok_or("The original review-notice admission was already used.")?;
            if cancel.load(Ordering::Acquire)
                || !matches_policy(config, p).map_err(|e| e.message)?
            {
                return Err("Technical review policy changed before notification.".into());
            }
            let mut admitted = store
                .admit_until(Instant::now() + Duration::from_millis(100))
                .ok_or("The technical task is busy.")?;
            let task = admitted
                .load_complete(&notice.task)
                .map_err(|e| e.to_string())?;
            let candidate = task
                .technical_notice_candidate(&notice.audit, &notice.reviewer)
                .map_err(|e| e.to_string())?;
            if candidate.audit.iter().rev().find(|audit| {
                audit.policy == notice.audit.policy
                    && audit.decision_id == notice.audit.decision_id
                    && audit.request_id.is_some()
            }) != Some(&notice.audit)
            {
                return Err("The original review-notice reservation changed.".into());
            }
            Ok(())
        });
    drop(held);
    let (action, message) = match result {
        crate::state::Submission::Receipt(receipt) if receipt["status"] == "submitted" => {
            (Action::Notified, "The assigned technical reviewer received the guarded notice.")
        }
        crate::state::Submission::NotOffered(_) => (Action::Refused, "Review notice refused before input. The assigned Review Check remains available."),
        _ => (Action::Uncertain, "Review notice delivery is unknown. Use the assigned Review Check; the notice will not be repeated."),
    };
    notice.audit.action = action;
    notice.audit.notice = None;
    let Some(store) = ProjectStore::open_within(
        &config.state.join("replica"),
        &p.project,
        Duration::from_millis(300),
    )
    .map_err(fail)?
    else {
        return Ok(());
    };
    let Some(mut admitted) = store.admit_until(Instant::now() + Duration::from_millis(100)) else {
        return Ok(());
    };
    let mut task = admitted.load_complete(&notice.task).map_err(fail)?;
    if task
        .record_technical_notice(&notice.audit, &notice.reviewer, message, now())
        .is_ok()
    {
        admitted.save(&mut task).map_err(fail)?;
    }
    Ok(())
}
