//! The Teams views' reads and actions (M1): one Teams poller of this computer
//! feeding the pump, the intent table of the actions, and the guarded
//! `hgs team <op>` calls of the Team tab.
//!
//! The webview never runs or parses an `hgs team` read: it follows the views it
//! shows (`teams_follow`), tells the core when the tab is hidden
//! (`teams_hidden`) and when the followed project's tasks topic changed
//! (`teams_tasks_changed`), asks for a later page (`teams_page`), and takes the
//! views from the per-frame batches (`batch.teams`, keyed `list`, `waiting` and
//! `team:<id>`). One team is followed at a time; following another ends the
//! first. One reader serves the life of the application, so two readers never
//! run beside each other.
//!
//! An action carries the request id the person's action made once
//! ([`TeamCall`]). The intent table remembers each id: a second call while it
//! is pending sends nothing, a settled intent returns its recorded outcome,
//! and an uncertain one is only sent again by the person's explicit
//! `team_action_again`.
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use tauri::State;
use zerus_client_core::actions::{Exit, Invocation};
use zerus_client_core::run_limited;
use zerus_client_core::teams::action::{
    run_action, Admission, Intents, Pending, TeamCall, TeamOutcome,
};
use zerus_client_core::teams::read::valid_team;
use zerus_client_core::teams::{TeamsFollow, TeamsPoller};

use crate::commands::AppState;

/// The one Teams reader of this computer and the intents whose actions it
/// sent.
#[derive(Default)]
pub struct Teams {
    poller: Mutex<Option<TeamsPoller>>,
    intents: Mutex<Intents>,
}

impl Teams {
    pub fn new() -> Self {
        Self::default()
    }

    fn poller(&self) -> MutexGuard<'_, Option<TeamsPoller>> {
        self.poller.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn intents(&self) -> MutexGuard<'_, Intents> {
        self.intents.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reads every followed view again after an action of our own.
    fn refresh(&self) {
        if let Some(poller) = self.poller().as_ref() {
            poller.refresh();
        }
    }
}

/// Follows the Teams views this window shows: the team list, what waits for
/// the person and at most one team. Every flag false and no team ends the
/// views but keeps the one reader.
#[tauri::command(async)]
pub fn teams_follow(
    state: State<'_, AppState>,
    teams: State<'_, Teams>,
    list: bool,
    waiting: bool,
    team: Option<String>,
) -> Result<(), String> {
    let team = team.filter(|team| !team.is_empty());
    if team.as_deref().is_some_and(|team| !valid_team(team)) {
        return Err("Not a team id.".into());
    }
    let follow = TeamsFollow {
        list,
        waiting,
        team,
    };
    let mut poller = teams.poller();
    if poller.is_none() {
        let pump = state.pump();
        let reader = TeamsPoller::spawn(zerus_client_core::default_hgs(), move |change| {
            pump.push(change)
        })
        .map_err(|error| format!("The Teams reader did not start: {error}"))?;
        *poller = Some(reader);
    }
    if let Some(poller) = poller.as_ref() {
        poller.follow(follow);
    }
    Ok(())
}

/// The Team tab was hidden (`true`) or shown again. A hidden tab reads nothing;
/// shown again, every followed view is read at once.
#[tauri::command]
pub fn teams_hidden(teams: State<'_, Teams>, hidden: bool) -> Result<(), String> {
    if let Some(poller) = teams.poller().as_ref() {
        poller.hidden(hidden);
    }
    Ok(())
}

/// The followed project's tasks topic changed: the watched team's status and
/// queue are read again (a report moves a node's state).
#[tauri::command]
pub fn teams_tasks_changed(teams: State<'_, Teams>) -> Result<(), String> {
    if let Some(poller) = teams.poller().as_ref() {
        poller.tasks_changed();
    }
    Ok(())
}

/// Reads one page of a followed view by its `next` cursor; the page replaces
/// the shown one and stays selected for every later read. No cursor selects
/// the first page again. The key is `list`, `waiting` or `team:<id>`.
#[tauri::command]
pub fn teams_page(
    teams: State<'_, Teams>,
    key: String,
    cursor: Option<String>,
) -> Result<(), String> {
    let poller = teams.poller();
    let Some(poller) = poller.as_ref() else {
        return Err("No Teams view is followed.".into());
    };
    if poller.page(&key, cursor.as_deref()) {
        Ok(())
    } else {
        Err("No such page for this view.".into())
    }
}

/// Runs one team action through `hgs team <op> --json`: exactly once, off the
/// main thread, with the request id the person's action made. The followed
/// view is read again after it.
#[tauri::command]
pub async fn team_action(teams: State<'_, Teams>, call: TeamCall) -> Result<TeamOutcome, String> {
    let admission = teams.intents().admit(&call);
    run(teams, admission, call).await
}

/// Sends an uncertain intent again because the person asked to check it: the
/// same request id and content, which the store answers from its record.
#[tauri::command]
pub async fn team_action_again(
    teams: State<'_, Teams>,
    call: TeamCall,
) -> Result<TeamOutcome, String> {
    let admission = teams.intents().admit_again(&call);
    run(teams, admission, call).await
}

async fn run(
    teams: State<'_, Teams>,
    admission: Admission,
    call: TeamCall,
) -> Result<TeamOutcome, String> {
    match admission {
        Admission::InFlight => Ok(TeamOutcome::Pending {
            request_id: call.request_id.clone(),
        }),
        Admission::Recorded(outcome) | Admission::Refused(outcome) => Ok(outcome),
        Admission::Send => {
            // The guard settles this intent on every way out, including a
            // panicking or failed worker: an unknown outcome, never a stray
            // Pending entry.
            let pending = Pending::new(&teams.intents, &call);
            let hgs = zerus_client_core::default_hgs();
            let sent = call.clone();
            let outcome =
                match tauri::async_runtime::spawn_blocking(move || run_action(&hgs, &sent)).await {
                    Ok(outcome) => outcome,
                    Err(error) => TeamOutcome::Uncertain {
                        detail: format!(
                            "The action's worker failed: {error}. Read the team before checking it."
                        ),
                        request_id: call.request_id.clone(),
                    },
                };
            let recorded = pending.settle(outcome);
            teams.refresh();
            Ok(recorded)
        }
    }
}

/// The person dismisses an uncertain intent: it settles as abandoned and
/// nothing is sent. Only an uncertain intent can be dismissed.
#[tauri::command]
pub fn team_intent_dismiss(teams: State<'_, Teams>, call: TeamCall) -> Result<(), String> {
    if teams.intents().dismiss(&call) {
        Ok(())
    } else {
        Err("Only an uncertain action can be dismissed.".into())
    }
}

/// The person's Teams catalog: `hgs team catalog show --json` answers
/// `{"ok": true, "catalog": {...}}` (roles, pipelines, templates and the
/// lead's planning rules). The form indexes the catalog's arrays, so the
/// boundary refuses a success that does not carry them.
#[tauri::command]
pub async fn teams_catalog() -> Result<Value, String> {
    let reply = read_team_json(&["team", "catalog", "show", "--json"], None).await?;
    catalog_shape(&reply)
}

/// Saves one template to the person's own catalog: `hgs team template save`
/// reads the template on standard input. The request is bounded before it is
/// serialized into a child's stdin.
#[tauri::command]
pub async fn teams_template_save(template: Value) -> Result<Value, String> {
    let size = serde_json::to_vec(&template)
        .map_err(|error| error.to_string())?
        .len();
    if size > TEMPLATE_INPUT_MAX {
        return Err(format!(
            "The template is {size} bytes; the limit is {TEMPLATE_INPUT_MAX}."
        ));
    }
    let reply = read_team_json(&["team", "template", "save", "--json"], Some(template)).await?;
    let saved = reply
        .get("saved")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reply.get("ok").and_then(Value::as_bool) != Some(true) || saved.is_empty() {
        return Err("The template was not saved: the answer named no template.".into());
    }
    Ok(reply)
}

/// A project folder's checkout facts (`hgs git-status --path PATH --json`):
/// its branch and head commit, or the reason it is not a repository. This
/// reply has no `ok` field of its own, so only its object shape is checked.
#[tauri::command]
pub async fn teams_project(path: String) -> Result<Value, String> {
    if path.len() > PATH_INPUT_MAX {
        return Err(format!(
            "The folder path is {} bytes; the limit is {PATH_INPUT_MAX}.",
            path.len()
        ));
    }
    let reply = read_team_json(&["git-status", "--path", &path, "--json"], None).await?;
    if !reply.is_object() {
        return Err("The folder check's answer is not an object.".into());
    }
    for key in ["state", "head", "branch"] {
        if let Some(value) = reply.get(key) {
            if !value.is_string() {
                return Err(format!("The folder check's {key} is not a string."));
            }
        }
    }
    Ok(reply)
}

/// Turns the person's Teams preview on or off: `hgs team preview on|off` with
/// a fixed word, no renderer-supplied text. The answer must name the state that
/// was asked for.
#[tauri::command]
pub async fn teams_preview(on: bool) -> Result<Value, String> {
    let reply = read_team_json(preview_args(on), None).await?;
    preview_shape(&reply, on)
}

/// The fixed words of the preview command, never a renderer's.
fn preview_args(on: bool) -> &'static [&'static str] {
    if on {
        &["team", "preview", "on"]
    } else {
        &["team", "preview", "off"]
    }
}

/// `{ok:true, preview:"on"|"off"}`; the state must be the one asked for.
fn preview_shape(reply: &Value, on: bool) -> Result<Value, String> {
    let wanted = if on { "on" } else { "off" };
    if reply.get("ok").and_then(Value::as_bool) != Some(true)
        || reply.get("preview").and_then(Value::as_str) != Some(wanted)
    {
        return Err(format!("The preview answer did not name {wanted}."));
    }
    Ok(reply.clone())
}

/// The catalog's own shape: a success with the three arrays the form reads.
fn catalog_shape(reply: &Value) -> Result<Value, String> {
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err("The catalog answer is not a success.".into());
    }
    let catalog = reply
        .get("catalog")
        .filter(|catalog| catalog.is_object())
        .ok_or("The catalog answer has no catalog.")?;
    for key in ["roles", "pipelines", "templates"] {
        if catalog.get(key).and_then(Value::as_array).is_none() {
            return Err(format!("The catalog answer has no {key} array."));
        }
    }
    let templates = catalog["templates"].as_array().ok_or("no templates")?;
    if templates.iter().any(|template| {
        !template.is_object()
            || template
                .get("id")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
    }) {
        return Err("The catalog has a template without an id.".into());
    }
    Ok(reply.clone())
}

/// The CLI's own request bound for one write (262,144 bytes).
const TEMPLATE_INPUT_MAX: usize = 262_144;
/// The longest folder path accepted before any child runs (4 KiB).
const PATH_INPUT_MAX: usize = 4_096;

/// One person-only `hgs` read through the client's bounded runner. These
/// commands start nothing: a refusal keeps its code and words.
async fn read_team_json(args: &[&str], stdin: Option<Value>) -> Result<Value, String> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let stdin = stdin.map(|value| value.to_string().into_bytes());
    let invocation = Invocation {
        args,
        stdin,
        timeout: std::time::Duration::from_secs(15),
        request_id: None,
    };
    let hgs = zerus_client_core::default_hgs();
    let exit = tauri::async_runtime::spawn_blocking(move || {
        run_limited(&hgs, &invocation, &[], zerus_client_core::reads::READ_LIMIT)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|too_large| format!("hgs answer passed {} bytes", too_large.cap))?;
    let (code, stdout, stderr) = match exit {
        Exit::Code {
            code,
            stdout,
            stderr,
        } => (code, stdout, stderr),
        Exit::Abnormal { stderr } => (1, Vec::new(), stderr),
        Exit::TimedOut => return Err("hgs did not answer in time".into()),
        Exit::StartFailed(message) => return Err(format!("hgs did not start: {message}")),
        Exit::WriteFailed => return Err("hgs did not take the request".into()),
    };
    decode_team_reply(code, &stdout, stderr.as_bytes())
}

/// Decode one helper's answer: a structured refusal on stdout is read even on a
/// nonzero exit, so its reason reaches the person; otherwise a nonzero exit
/// falls back to stderr, and a success is the JSON object it answered.
fn decode_team_reply(code: i32, stdout: &[u8], stderr: &[u8]) -> Result<Value, String> {
    if let Ok(reply) = serde_json::from_slice::<Value>(stdout) {
        if reply.get("ok").and_then(Value::as_bool) == Some(false) {
            let refusal = reply["error"]["code"].as_str().unwrap_or_default();
            let message = reply["error"]["message"].as_str().unwrap_or_default();
            return Err(if message.is_empty() {
                format!("hgs refused: {refusal}")
            } else {
                format!("{refusal}: {message}")
            });
        }
    }
    if code != 0 {
        let text = String::from_utf8_lossy(stderr);
        let text = text.trim();
        return Err(if text.is_empty() {
            "hgs refused the read".into()
        } else {
            text.to_owned()
        });
    }
    serde_json::from_slice(stdout).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_structured_refusal_is_decoded_on_a_nonzero_exit() {
        let refusal = json!({"ok": false, "error": {"code": "permission_denied", "message": "Only the person may save a template."}});
        let error = decode_team_reply(1, refusal.to_string().as_bytes(), b"").unwrap_err();
        assert!(
            error.contains("Only the person may save a template."),
            "{error}"
        );
        assert!(error.contains("permission_denied"), "{error}");
    }

    #[test]
    fn a_nonzero_exit_without_a_reply_keeps_stderr_or_a_generic_reason() {
        assert_eq!(
            decode_team_reply(1, b"", b"hgs: the catalog is unreadable\n").unwrap_err(),
            "hgs: the catalog is unreadable"
        );
        assert_eq!(
            decode_team_reply(1, b"", b"").unwrap_err(),
            "hgs refused the read"
        );
    }

    #[test]
    fn a_malformed_success_is_refused_by_the_catalog_shape() {
        assert!(catalog_shape(&json!({})).is_err());
        assert!(catalog_shape(&json!({"ok": true, "catalog": {}})).is_err());
        assert!(catalog_shape(&json!({"ok": true, "catalog": {"roles": [], "pipelines": [], "templates": [{"name": "No id"}]}})).is_err());
        let catalog = json!({"ok": true, "catalog": {"roles": [], "pipelines": [], "templates": [{"id": "bugfix", "name": "Bugfix"}]}});
        assert!(catalog_shape(&catalog).is_ok());
    }

    #[test]
    fn a_folder_check_object_needs_no_ok_field() {
        assert!(decode_team_reply(
            0,
            br#"{"state":"ok","head":"0123456789abcdef0123456789abcdef01234567","branch":"main"}"#,
            b""
        )
        .is_ok());
    }

    #[test]
    fn the_preview_command_takes_only_its_own_fixed_words() {
        assert_eq!(preview_args(true), ["team", "preview", "on"]);
        assert_eq!(preview_args(false), ["team", "preview", "off"]);
    }

    #[test]
    fn the_preview_answer_must_name_the_state_that_was_asked_for() {
        assert!(preview_shape(&json!({"ok": true, "preview": "on"}), true).is_ok());
        assert!(preview_shape(&json!({"ok": true, "preview": "off"}), false).is_ok());
        assert!(preview_shape(&json!({"ok": true, "preview": "off"}), true).is_err());
        assert!(preview_shape(&json!({"ok": true}), false).is_err());
        assert!(preview_shape(&json!({}), true).is_err());
    }

    #[test]
    fn the_input_bounds_refuse_before_a_child_runs() {
        assert!(TEMPLATE_INPUT_MAX == 262_144 && PATH_INPUT_MAX == 4_096);
        assert!(
            serde_json::to_vec(&json!({"x": "a".repeat(TEMPLATE_INPUT_MAX)}))
                .unwrap()
                .len()
                > TEMPLATE_INPUT_MAX
        );
    }
}
