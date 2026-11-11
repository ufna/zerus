//! The writes of `hgs team`. Every one builds the operation envelope the store
//! defines and lets the store decide authority; the CLI only refuses a command
//! whose actor kind is wrong before any request is read.
use super::actor::{self, Caller};
use super::input::{self, Options};
use super::{request_input, storage};
use crate::config::Config;
use crate::teams::catalog;
use crate::teams::ops::{self, Actor, Reply, Request};
use crate::teams::store::Store;
use serde_json::{json, Map, Value};
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

/// The operation a command names. The command line, not the request, chooses
/// the operation; `create` is its own function because it reads the catalog.
pub(super) fn variant(command: &str) -> Option<&'static str> {
    Some(match command {
        "settings" => "change_settings",
        "pause" => "pause",
        "resume" => "resume",
        "stop" => "stop",
        "archive" => "archive",
        "decide" => "decide_card",
        "admit" => "admit",
        "route" => "route",
        "accept" => "accept",
        "return" => "return",
        "reassign" => "reassign",
        "hold" => "hold",
        "release" => "release",
        "cancel" => "cancel",
        "reply" => "reply",
        "ask" => "ask",
        "pause-member" => "pause_member",
        "resume-member" => "resume_member",
        "stop-member" => "stop_member",
        "archive-member" => "archive_member",
        _ => return None,
    })
}

/// One operation command, or `not_found` for a word no package implements.
pub(super) fn run(
    config: &Config,
    caller: &Caller,
    command: &str,
    args: &[String],
    provided: Option<&str>,
) -> Result<Value, Problem> {
    let variant = variant(command).ok_or_else(|| super::unknown(command))?;
    if !args.is_empty() {
        return Err(input::usage(format!(
            "{command} takes only its JSON request on stdin"
        )));
    }
    let mut store = Store::open(&config.state)?;
    let identity = actor::resolve(&store, caller)?;
    if command == "ask" {
        actor::member_session(&identity, "ask")?;
    }
    let request = build(variant, &request_input(provided)?)?;
    let reply = ops::apply(&mut store, &identity.actor, &request, input::now_ms())?;
    reply_view(&identity.actor, &request.team, &reply)
}

/// `hgs team create --template ID`: the template comes from the catalog, the
/// rest of the team from the request. It records the team and launches nothing.
pub(super) fn create(
    config: &Config,
    caller: &Caller,
    args: &[String],
    provided: Option<&str>,
) -> Result<Value, Problem> {
    let options = Options::parse(args, &["--template"], &[])?;
    options.nothing_else("create")?;
    let template = options
        .get("--template")
        .ok_or_else(|| input::usage("create needs --template TEMPLATE"))?;
    let store = Store::open(&config.state)?;
    let identity = actor::resolve(&store, caller)?;
    actor::person_only(&identity, "create")?;
    let mut fields = input::object(&request_input(provided)?)?;
    refuse_operation_word(&fields)?;
    let catalog = catalog::load(&config.dir)?;
    let bundle = zerus_team_plan::bundle(&catalog, template)?;
    fields.insert("op".into(), json!("create"));
    fields.insert(
        "bundle".into(),
        serde_json::to_value(bundle).map_err(storage)?,
    );
    let request: Request = serde_json::from_value(Value::Object(fields))
        .map_err(|error| input::serde_problem(error.to_string()))?;
    let mut store = store;
    let reply = ops::apply(&mut store, &identity.actor, &request, input::now_ms())?;
    reply_view(&identity.actor, &request.team, &reply)
}

/// The request is the operation's own fields plus the envelope; it never names
/// its actor and never chooses the operation.
fn build(variant: &str, text: &str) -> Result<Request, Problem> {
    let mut fields = input::object(text)?;
    refuse_operation_word(&fields)?;
    fields.insert("op".into(), json!(variant));
    serde_json::from_value(Value::Object(fields))
        .map_err(|error| input::serde_problem(error.to_string()))
}

fn refuse_operation_word(fields: &Map<String, Value>) -> Result<(), Problem> {
    if !fields.contains_key("op") {
        return Ok(());
    }
    Err(Problem::new(
        code::UNKNOWN_FIELD,
        "The request has an unknown field: op; the command names the operation.",
    )
    .with_refs(["op"]))
}

/// One reply in the shape every `hgs team` write uses: the store's own reply,
/// the team it was about, and the actor it was recorded for.
fn reply_view(actor: &Actor, team: &str, reply: &Reply) -> Result<Value, Problem> {
    let mut value = serde_json::to_value(reply).map_err(storage)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("actor".into(), actor::view(actor));
        object.insert("team".into(), json!(team));
    }
    Ok(value)
}
