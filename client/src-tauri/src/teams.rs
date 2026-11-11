//! The Teams views' reads and actions (M1): one Teams poller of this computer
//! feeding the pump, and the guarded `hgs team <op>` calls of the Team tab.
//!
//! The webview never runs or parses an `hgs team` read: it follows the views it
//! shows (`teams_follow`), tells the core when the tab is hidden
//! (`teams_hidden`) and when the followed project's tasks topic changed
//! (`teams_tasks_changed`), and takes them from the per-frame batches
//! (`batch.teams`, keyed `list`, `waiting` and `team:<id>`). One team is
//! followed at a time; following another ends the first.
//!
//! An action runs once, off the main thread. Its outcome says whether it was
//! done, failed, may have been applied (`uncertain`) or waits for the person's
//! confirmation; the desktop never sends an uncertain call again by itself.
use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::State;
use zerus_client_core::teams::action::{run_action, TeamCall, TeamOutcome};
use zerus_client_core::teams::read::valid_team;
use zerus_client_core::teams::{TeamsFollow, TeamsPoller};

use crate::commands::AppState;

/// The one Teams poller of this computer, or none while nothing is followed.
#[derive(Default)]
pub struct Teams {
    poller: Mutex<Option<TeamsPoller>>,
}

impl Teams {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Option<TeamsPoller>> {
        self.poller.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Follows the Teams views this window shows: the team list, what waits for
/// the person and at most one team. Every flag false and no team stops the
/// reader and ends the views.
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
    {
        let mut poller = teams.lock();
        if follow == TeamsFollow::default() {
            if let Some(poller) = poller.take() {
                // Its ends go out through the batch; then the thread stops.
                poller.follow(follow);
            }
            return Ok(());
        }
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
}

/// The Team tab was hidden (`true`) or shown again. A hidden tab reads nothing;
/// shown again, every followed view is read at once.
#[tauri::command]
pub fn teams_hidden(teams: State<'_, Teams>, hidden: bool) -> Result<(), String> {
    if let Some(poller) = teams.lock().as_ref() {
        poller.hidden(hidden);
    }
    Ok(())
}

/// The followed project's tasks topic changed: the watched team's status and
/// queue are read again (a report moves a node's state).
#[tauri::command]
pub fn teams_tasks_changed(teams: State<'_, Teams>) -> Result<(), String> {
    if let Some(poller) = teams.lock().as_ref() {
        poller.tasks_changed();
    }
    Ok(())
}

/// Runs one team action through `hgs team <op> --json`: exactly once, off the
/// main thread. The followed view is read again after it.
#[tauri::command]
pub async fn team_action(teams: State<'_, Teams>, call: TeamCall) -> Result<TeamOutcome, String> {
    let hgs = zerus_client_core::default_hgs();
    let outcome = tauri::async_runtime::spawn_blocking(move || run_action(&hgs, &call))
        .await
        .map_err(|error| error.to_string())?;
    // The desktop's own action: read the views again instead of waiting for
    // the next poll.
    if let Some(poller) = teams.lock().as_ref() {
        poller.refresh();
    }
    Ok(outcome)
}
