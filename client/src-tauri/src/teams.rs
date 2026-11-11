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

use tauri::State;
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
