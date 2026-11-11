//! A stored team effect is the permit for one ordinary task attempt.
use super::*;
pub(super) mod completion;
pub(super) mod stage;
use crate::swarm::fence::{Admission, Closed};
use crate::teams::{store::Store, tasks::context};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zerus_replica::execution::{EventKind, Origin};
use zerus_team_plan::model::{Member, MemberState, Node, TaskRef, Team, TeamStatus};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: u32,
    id: String,
    team: String,
    node: u32,
    member: String,
}

impl Input {
    fn validate(&self) -> Result<()> {
        let id =
            |s: &str| uuid::Uuid::parse_str(s).is_ok_and(|id| !id.is_nil() && id.to_string() == s);
        if self.schema != 1
            || !id(&self.id)
            || !id(&self.team)
            || self.node == 0
            || self.member.is_empty()
            || self.member.len() > 256
            || self.member.chars().any(char::is_control)
        {
            return Err(fail("Invalid team delivery request."));
        }
        Ok(())
    }
    fn reply(&self, state: &str, error: &str, receipt: Option<Value>) -> Value {
        let mut out = json!({"schema":1,"id":self.id,"team":self.team,"node":self.node,"member":self.member,"state":state});
        if !error.is_empty() {
            out["error"] = json!(error);
        }
        if let Some(receipt) = receipt {
            out["receipt"] = receipt;
        }
        out
    }
}

const UNCONFIRMED_WRITE: &str = "Unconfirmed team task write:";

struct Permit {
    team: Team,
    node: Node,
    member: Member,
    lead: Member,
    task: TaskRef,
    kind: String,
    reason: String,
}

fn problem(p: zerus_team_plan::Problem) -> Error {
    fail(format!("{}: {}", p.code, p.message))
}

fn scope(store: &Store, body: &Input, here: &str) -> Result<(Team, Node, TaskRef, String)> {
    let team = store
        .team(&body.team)
        .map_err(problem)?
        .ok_or_else(|| fail("Team not found."))?;
    if team.home != here {
        return Err(fail("Team belongs to another computer."));
    }
    let db = store.db.as_ref().ok_or_else(|| fail("No team store."))?;
    let effect: Option<(u32, String, String)> = db
        .query_row(
            "SELECT node,member,kind FROM effects WHERE team=?1 AND request_id=?2",
            params![body.team, body.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(fail)?;
    let (node, member, kind) =
        effect.ok_or_else(|| fail("No recorded delivery effect for this ID."))?;
    if node != body.node || member != body.member || !matches!(kind.as_str(), "handout" | "reopen")
    {
        return Err(fail("Delivery effect binding differs."));
    }
    let node = crate::teams::store::rows::node(db, &body.team, body.node)
        .map_err(problem)?
        .ok_or_else(|| fail("Node not found."))?;
    let task = node
        .task
        .clone()
        .ok_or_else(|| fail("Node has no task document."))?;
    let exact:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM node_identity WHERE team=?1 AND id=?2 AND task_project=?3 AND task_uuid=?4)",
        params![body.team,body.node,task.project,task.uuid],|r|r.get(0)).map_err(fail)?;
    if !exact || task.project != team.project {
        return Err(fail("Node task identity differs."));
    }
    Ok((team, node, task, kind))
}

fn permit(store: &Store, body: &Input, here: &str) -> Result<Permit> {
    let (team, node, task, kind) = scope(store, body, here)?;
    if team.status != TeamStatus::Running {
        return Err(fail("Team is not Running."));
    }
    let db = store.db.as_ref().expect("scoped store");
    let (generation, state): (u32, String) = db
        .query_row(
            "SELECT generation,state FROM effects WHERE team=?1 AND request_id=?2",
            params![body.team, body.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(fail)?;
    if Some(generation) != store.generation(&body.team, body.node).map_err(problem)?
        || !matches!(
            state.as_str(),
            "reserved" | "submitted" | "uncertain" | "cutting_over"
        )
    {
        return Err(fail("Effect is closed or belongs to another generation."));
    }
    let member = store
        .member(&body.team, &body.member)
        .map_err(problem)?
        .ok_or_else(|| fail("Member not found."))?;
    let lead = store
        .member(&body.team, &team.lead)
        .map_err(problem)?
        .ok_or_else(|| fail("Lead not found."))?;
    for m in [&member, &lead] {
        if m.state != MemberState::Active
            || m.session
                .as_ref()
                .is_none_or(|s| s.machine != here || s.session_id.is_empty())
        {
            return Err(fail("Member or lead has no active local session binding."));
        }
    }
    if member.id == lead.id {
        return Err(fail("Lead cannot receive a member hand-out."));
    }
    let owns:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM live_tasks WHERE task_project=?1 AND task_uuid=?2 AND team=?3 AND node=?4)",params![task.project,task.uuid,body.team,body.node],|r|r.get(0)).map_err(fail)?;
    if !owns {
        return Err(fail("Task is not this team's live node."));
    }
    let reason = if kind == "reopen" {
        db.query_row("SELECT text FROM decisions WHERE team=?1 AND node=?2 AND generation=?3 AND kind IN ('return','reassign') AND state='effect_reserved' ORDER BY seq DESC LIMIT 1",params![body.team,body.node,generation],|r|r.get::<_,String>(0)).optional().map_err(fail)?.ok_or_else(||fail("No matching reopen decision."))?
    } else {
        String::new()
    };
    Ok(Permit {
        team,
        node,
        member,
        lead,
        task,
        kind,
        reason,
    })
}

fn session(machines: &Machines, member: &Member) -> Result<SessionKey> {
    let bound = member
        .session
        .as_ref()
        .ok_or_else(|| fail("Session is unbound."))?;
    // Labels come from saved metadata only after selection by exact identity.
    let key = SessionKey {
        machine: bound.machine.clone(),
        session_id: bound.session_id.clone(),
        session: String::new(),
    };
    machines
        .resolve(&key)
        .filter(|s| !s.session.is_empty() && !s.session_id.is_empty())
        .ok_or_else(|| fail("Bound session identity is unavailable."))
}

#[allow(clippy::too_many_arguments)]
fn request(
    settings: &Settings,
    starts: usize,
    task: &TaskDoc,
    name: &str,
    body: &Input,
    permit: &Permit,
    machines: &Machines,
    store: &Store,
) -> Result<(Value, SessionKey, Actor)> {
    let to = session(machines, &permit.member)?;
    let who = Actor::Agent(session(machines, &permit.lead)?);
    let base = if permit.kind == "handout" {
        send_check(task)?;
        attempts::render(settings, starts, task, name, &to, false, &who, &body.id)?
    } else {
        settings
            .check_agent(&who, &to, true, starts)
            .map_err(fail)?;
        let view = task.view().map_err(fail)?;
        if !matches!(view.stage, Stage::Review | Stage::Done) || view.session.as_ref() != Some(&to)
        {
            return Err(fail("Reopen no longer names this task's executor."));
        }
        let dispatch = view
            .execution_id()
            .ok_or_else(|| fail("Task has no dispatch to reopen."))?;
        attempts::reopen_request(name, &view, &body.id, &permit.reason, &[], &to, &dispatch)?
    };
    let text = format!(
        "{}{}",
        base["text"]
            .as_str()
            .ok_or_else(|| fail("Frozen request has no text."))?,
        context::build(store, &permit.team, &permit.node, &permit.member).map_err(problem)?
    );
    let files = base["attachments"]
        .as_array()
        .ok_or_else(|| fail("Frozen request has no attachments."))?;
    let dispatch = if permit.kind == "reopen" {
        task.view()
            .map_err(fail)?
            .execution_id()
            .unwrap_or_default()
    } else {
        String::new()
    };
    let frozen =
        crate::state::prepare_task_reply(&to, &dispatch, &body.id, &text, files).map_err(fail)?;
    if frozen["expected_run_id"] != base["expected_run_id"]
        || frozen["expected_conversation_id"] != base["expected_conversation_id"]
    {
        return Err(fail(dispatch::CHANGED));
    }
    Ok((frozen, to, who))
}

trait Delivery {
    fn deliver(
        &self,
        config: &Config,
        task: &TaskRef,
        body: &Input,
        to: &SessionKey,
        request: &Value,
    ) -> attempts::Delivered;
    fn before_reserve(&self) {}
}
struct Native;
impl Delivery for Native {
    fn deliver(
        &self,
        config: &Config,
        task: &TaskRef,
        body: &Input,
        to: &SessionKey,
        request: &Value,
    ) -> attempts::Delivered {
        attempts::deliver(config, &task.project, &task.uuid, &body.id, to, request)
    }
}

pub(super) fn dispatch(config: &Config, args: &[String]) -> Result<i32> {
    match execute(config, args, &Native) {
        Ok(reply) => {
            println!("{reply}");
            Ok(0)
        }
        Err(error) => {
            println!(
                "{}",
                json!({"schema":1,"state":"refused","error":error.message})
            );
            Ok(1)
        }
    }
}

fn execute(config: &Config, args: &[String], delivery: &dyn Delivery) -> Result<Value> {
    if !crate::teams::enabled(&config.state) {
        return Err(fail("teams_inactive: Agent Teams preview is off."));
    }
    arguments(args, true, true)?;
    let deadline = Instant::now() + attempts::BUDGET;
    let text = stdin_text("team delivery request")?;
    let body: Input = serde_json::from_str(&text).map_err(|error| {
        fail(format!(
            "{}: {error}",
            if error.to_string().contains("unknown field") {
                "unknown_field"
            } else {
                "invalid_request"
            }
        ))
    })?;
    body.validate()?;
    match run(config, args, &body, delivery, deadline) {
        Ok(reply) => Ok(reply),
        Err(error) => Ok(body.reply("refused", &error.message, None)),
    }
}

fn arguments(args: &[String], reference: bool, recover: bool) -> Result<()> {
    let mut at = usize::from(reference);
    if reference && args.first().is_none_or(|s| s.starts_with('-')) {
        return Err(fail("A task reference is required."));
    }
    let mut seen = std::collections::BTreeSet::new();
    while at < args.len() {
        let key = args[at].as_str();
        if !seen.insert(key) {
            return Err(fail("Duplicate team service option."));
        }
        match key {
            "--json" => at += 1,
            "--recover-only" if recover => at += 1,
            "--project" if reference && at + 1 < args.len() => at += 2,
            _ => return Err(fail("Unknown team service option.")),
        }
    }
    if !seen.contains("--json") {
        return Err(fail("Team service commands require --json."));
    }
    Ok(())
}

fn identity(config: &Config) -> Result<String> {
    #[derive(Deserialize)]
    struct Identity {
        node_id: String,
    }
    let file = crate::platform::no_follow(std::fs::OpenOptions::new().read(true))
        .open(config.dir.join("swarm/catalog.json"))
        .map_err(fail)?;
    let identity: Identity = serde_json::from_reader(file.take(32 * 1024 * 1024)).map_err(fail)?;
    if !zerus_replica::ids::is_uuid(&identity.node_id) {
        return Err(fail("Invalid local machine identity."));
    }
    Ok(identity.node_id)
}

fn recover(
    config: &Config,
    body: &Input,
    task: &TaskDoc,
    reference: &TaskRef,
    kind: &str,
    home: &str,
) -> Result<Value> {
    let attempt = task
        .attempt(&body.id)
        .map_err(fail)?
        .ok_or_else(|| fail("No recorded attempt for this delivery ID."))?;
    if attempt.origin
        != if kind == "reopen" {
            Origin::Reopen
        } else {
            Origin::Send
        }
        || attempt.executor.machine != home
    {
        return Err(fail("Attempt does not belong to this team effect."));
    }
    let native = attempt
        .delivery
        .ok_or_else(|| fail("Attempt has no native delivery binding."))?;
    let facts = task
        .attempt_facts(&body.id)
        .map_err(fail)?
        .unwrap_or_default();
    if facts.contains(&EventKind::NotStarted) {
        return Ok(body.reply("refused", "The attempt was not admitted.", None));
    }
    match crate::state::task_reply_receipt_bound(&native.name, &body.id, &native.digest) {
        Ok(crate::state::ReceiptLookup::Found(receipt)) if receipt["status"] == "submitted" => {
            if let Err(error) = attempts::settle(
                config,
                &reference.project,
                &reference.uuid,
                &body.id,
                &receipt,
            ) {
                return Ok(body.reply("uncertain", &error.message, None));
            }
            Ok(body.reply("submitted", "", Some(receipt)))
        }
        _ if facts.contains(&EventKind::Submitted) => Ok(body.reply("submitted", "", None)),
        _ => Ok(body.reply(
            "uncertain",
            "The original delivery is not confirmed; nothing was sent again.",
            None,
        )),
    }
}

fn run(
    config: &Config,
    args: &[String],
    body: &Input,
    delivery: &dyn Delivery,
    deadline: Instant,
) -> Result<Value> {
    let here = identity(config)?;
    let teams_store = Store::open(&config.state).map_err(problem)?;
    let (team, _, reference, kind) = scope(&teams_store, body, &here)?;
    let mut scoped = args.to_vec();
    if option(args, "--project")?.is_none() {
        scoped.extend(["--project".into(), reference.project.clone()]);
    }
    let r = super::reference(config, &scoped, &Actor::Person)?;
    if r.project.0 != reference.project {
        return Err(fail("REF is not the stored effect's project."));
    }
    if !super::store_dir(config, &reference.project).is_dir() {
        return Err(fail("Task store is unavailable."));
    }
    let tasks = super::store_until(config, &reference.project, deadline)?;
    let task = tasks
        .admit_until(deadline)
        .ok_or_else(|| fail(BUSY))?
        .load_complete(&reference.uuid)
        .map_err(fail)?;
    if r.task != reference.uuid && r.task != task.id() {
        return Err(fail("REF is not the stored effect's task."));
    }
    if task.attempt(&body.id).map_err(fail)?.is_some() {
        drop(tasks);
        return recover(config, body, &task, &reference, &kind, &team.home);
    }
    if flag(args, "--recover-only") {
        return Ok(body.reply(
            "uncertain",
            "No attempt was recorded; recovery sends nothing.",
            None,
        ));
    }
    let permit = permit(&teams_store, body, &here)?;
    marker(&task, &body.team, &here)?;
    let state: String = teams_store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT state FROM effects WHERE team=?1 AND request_id=?2",
            params![body.team, body.id],
            |r| r.get(0),
        )
        .map_err(fail)?;
    if state != "reserved" {
        return Ok(body.reply(
            "uncertain",
            "Effect already has an outcome; no delivery was repeated.",
            None,
        ));
    }
    let machines = machines_for(config, here.clone())?;
    let settings = tasks.project().map_err(fail)?.settings();
    let (frozen, to, who) = request(
        &settings,
        0,
        &task,
        &r.project.1,
        body,
        &permit,
        &machines,
        &teams_store,
    )?;
    let bound = task.view().map_err(fail)?;
    drop(tasks);
    drop(teams_store);
    delivery.before_reserve();
    let detail = std::cell::RefCell::new(String::new());
    let admitted = attempts::admit(
        config,
        &reference.project,
        &reference.uuid,
        deadline,
        |admitted, task| {
            if task.attempt(&body.id)?.is_some() {
                return Ok(Err(Closed {
                    node: None,
                    reason: "existing",
                }));
            }
            let result = (|| -> Result<Store> {
                let store = Store::open(&config.state).map_err(problem)?;
                // Hold the team's operation lock through the attempt's save.
                store
                    .db
                    .as_ref()
                    .ok_or_else(|| fail("Team store disappeared."))?
                    .execute_batch("BEGIN IMMEDIATE")
                    .map_err(fail)?;
                let fresh = permit_for_reservation(&store, body, &here, &permit, &bound, task)?;
                let settings = admitted.project().map_err(fail)?.settings();
                let starts =
                    attempts::admitted_starts(admitted, &who, &settings, deadline).map_err(fail)?;
                let (current, current_to, current_actor) = request(
                    &settings,
                    starts,
                    task,
                    &r.project.1,
                    body,
                    &fresh,
                    &machines,
                    &store,
                )?;
                if current != frozen || current_to != to || current_actor != who {
                    return Err(fail(dispatch::CHANGED));
                }
                reserve(task, body, &fresh, &to, &who, &frozen)?;
                Ok(store)
            })();
            match result {
                Ok(_permit_lock) => {
                    // An error once saving starts can leave a durable attempt.
                    // Let the fence turn it into Uncertain; never a refusal.
                    admitted.save(task)?;
                    #[cfg(test)]
                    if std::env::var_os("HGS_TEST_TEAM_DELIVERY_SAVE_ERROR").is_some() {
                        anyhow::bail!("fixture error after the durable task save");
                    }
                    Ok(Ok(()))
                }
                Err(error) => {
                    *detail.borrow_mut() = error.message;
                    Ok(Err(Closed {
                        node: None,
                        reason: "not_sendable",
                    }))
                }
            }
        },
    );
    match admitted {
        Ok(Admission::Reserved { .. }) => Ok(delivered(
            body,
            delivery.deliver(config, &reference, body, &to, &frozen),
        )),
        Ok(Admission::Refused(closed)) if closed.reason == "existing" => {
            let task = super::store(config, &reference.project)?
                .load(&reference.uuid)
                .map_err(fail)?;
            recover(config, body, &task, &reference, &kind, &here)
        }
        Ok(Admission::Refused(closed)) => {
            let (state, error) = attempts::refusal(&closed, &detail.borrow());
            Ok(body.reply(state, &error, None))
        }
        Ok(Admission::Legacy(closed)) => Ok(body.reply(
            "cutting_over",
            &format!(
                "Attempt admission is unavailable ({}); nothing was delivered.",
                closed.reason
            ),
            None,
        )),
        Ok(Admission::Uncertain(error)) => Ok(body.reply("uncertain", &error, None)),
        Err(error) => Ok(body.reply("uncertain", &error.message, None)),
    }
}

fn permit_for_reservation(
    store: &Store,
    body: &Input,
    here: &str,
    old: &Permit,
    bound: &zerus_replica::TaskView,
    task: &TaskDoc,
) -> Result<Permit> {
    let fresh = permit(store, body, here)?;
    marker(task, &body.team, here)?;
    let current = task.view().map_err(fail)?;
    if fresh.team.revision != old.team.revision
        || fresh.task != old.task
        || fresh.kind != old.kind
        || current.stage_at != bound.stage_at
        || current.target != bound.target
        || current.session != bound.session
    {
        return Err(fail(dispatch::CHANGED));
    }
    Ok(fresh)
}

fn marker(task: &TaskDoc, team: &str, here: &str) -> Result<()> {
    if task
        .team()
        .map_err(fail)?
        .as_ref()
        .is_none_or(|key| key.team != team || key.home != here)
        || task.team_invalid()
    {
        return Err(fail("Task no longer has this team's exact home marker."));
    }
    Ok(())
}

/// A bounded metadata read, under the caller's project/admission locks. A bad
/// file or unreadable path is present or an error, never an absent task.
fn task_present(config: &Config, reference: &TaskRef) -> Result<bool> {
    if !zerus_replica::ids::is_uuid(&reference.uuid) {
        return Err(fail("Invalid task identity."));
    }
    let directory = super::store_dir(config, &reference.project).join("tasks");
    for suffix in ["loro", "updates"] {
        match std::fs::symlink_metadata(directory.join(format!("{}.{suffix}", reference.uuid))) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(fail(error)),
        }
    }
    Ok(false)
}

fn reserve(
    task: &mut TaskDoc,
    body: &Input,
    permit: &Permit,
    to: &SessionKey,
    who: &Actor,
    request: &Value,
) -> Result<()> {
    let origin = if permit.kind == "reopen" {
        let view = task.view().map_err(fail)?;
        let note = zerus_replica::TaskDelivery {
            mode: "note".into(),
            state: "note".into(),
            stage: view.stage.as_str().into(),
            stage_at: view.stage_at,
            dispatch_id: view.execution_id().unwrap_or_default(),
            submitted_id: String::new(),
            run_id: String::new(),
            conversation_id: String::new(),
            prompt: String::new(),
            error: String::new(),
        };
        task.reserve_message(&body.id, &permit.reason, &[], &note, None, (who, now()))
            .map_err(fail)?;
        Origin::Reopen
    } else {
        Origin::Send
    };
    task.reserve_attempt(
        &body.id,
        origin,
        to,
        attempts::native(to, request),
        who,
        now(),
    )
    .map_err(fail)?;
    Ok(())
}

fn delivered(body: &Input, result: attempts::Delivered) -> Value {
    match result {
        attempts::Delivered::Submitted(receipt) => body.reply("submitted", "", Some(receipt)),
        attempts::Delivered::NotAdmitted(error) => body.reply("refused", &error, None),
        attempts::Delivered::Uncertain(error) => body.reply("uncertain", &error, None),
    }
}

#[cfg(test)]
mod tests;
