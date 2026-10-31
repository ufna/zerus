//! In-memory Undo windows. This owns no source or writer stack: reservations
//! use Actions' existing lane and remain invisible to its send-block facts.
//! Only the main webview can register/cancel; close and quit discard the queue.
use crate::actions::{Actions, Claim};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Instant,
};
use tauri::{State, Webview};
use zerus_client_core::{
    creation,
    end_action::{Captured, Window, STATE_CHANGED},
    Action, Target,
};

const WINDOWS: usize = 5;
const TARGETS: usize = 1024;
const REGISTRATIONS: usize = 16_384;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub id: String,
    pub index: usize,
}
struct Member {
    captured: Captured,
    used: bool,
}
struct Group {
    window: Window,
    waiting: bool,
    ready: bool,
    members: Vec<Member>,
    claims: BTreeMap<(String, String), Arc<Claim>>,
}
#[derive(Default)]
struct Model {
    closed: bool,
    seen: BTreeSet<String>,
    groups: BTreeMap<String, Group>,
}
#[derive(Default)]
struct Inner {
    model: Mutex<Model>,
    changed: Condvar,
}
#[derive(Clone, Default)]
pub struct Delays(Arc<Inner>);
fn lock(inner: &Inner) -> MutexGuard<'_, Model> {
    inner.model.lock().unwrap_or_else(|error| {
        let mut model = error.into_inner();
        model.closed = true;
        model.groups.clear();
        inner.changed.notify_all();
        model
    })
}

impl Delays {
    fn begin(&self, actions: &Actions, id: String, captures: Vec<Captured>) -> Result<(), String> {
        if !creation::canonical_uuid(&id)
            || captures.is_empty()
            || captures.len() > TARGETS
            || captures.iter().any(|c| !c.valid())
        {
            return Err(STATE_CHANGED.into());
        }
        let mut model = lock(&self.0);
        if model.closed
            || model.groups.len() >= WINDOWS
            || model.seen.len() >= REGISTRATIONS
            || model.seen.contains(&id)
        {
            return Err("busy".into());
        }
        let mut claims = BTreeMap::new();
        for c in &captures {
            let key = (c.target.computer.clone(), c.target.name.clone());
            if let std::collections::btree_map::Entry::Vacant(entry) = claims.entry(key) {
                // A partial reservation drops all its claims before returning.
                entry.insert(actions.reserve_end(&c.target).ok_or("busy")?);
            }
        }
        // Retain bounded tombstones for this process: an old waiter/cancel
        // must never bind to a reused caller UUID after Undo or completion.
        model.seen.insert(id.clone());
        model.groups.insert(
            id,
            Group {
                window: Window::new(Instant::now()),
                waiting: false,
                ready: false,
                members: captures
                    .into_iter()
                    .map(|captured| Member {
                        captured,
                        used: false,
                    })
                    .collect(),
                claims,
            },
        );
        Ok(())
    }

    /// Blocking only on a dedicated worker, with an absolute native deadline.
    /// Cancelling removes the group and wakes the worker immediately. Returning
    /// true is timing only: `take` must still find the current exact reservation.
    fn wait(&self, id: &str) -> bool {
        let mut model = lock(&self.0);
        let Some(group) = model.groups.get_mut(id) else {
            return false;
        };
        if group.waiting || group.ready {
            return false;
        }
        group.waiting = true;
        loop {
            let Some(group) = model.groups.get_mut(id) else {
                return false;
            };
            if group.ready {
                return false;
            }
            let now = Instant::now();
            if group.window.take_elapsed(now) {
                group.ready = true;
                return true;
            }
            let remaining = group.window.remaining(now);
            match self.0.changed.wait_timeout(model, remaining) {
                Ok((next, _)) => model = next,
                Err(error) => {
                    (model, _) = error.into_inner();
                    model.closed = true;
                    model.groups.clear();
                    self.0.changed.notify_all();
                }
            }
        }
    }

    pub(crate) fn take(
        &self,
        reference: &Reference,
        target: &Target,
        action: &Action,
    ) -> Result<(Captured, Arc<Claim>), String> {
        self.take_if(reference, |captured| {
            captured.target == *target && captured.action == *action
        })
    }

    pub(crate) fn take_clear(
        &self,
        reference: &Reference,
        run: &str,
        conversation: &str,
        dismiss: bool,
    ) -> Result<(Captured, Arc<Claim>), String> {
        self.take_if(reference, |captured| {
            captured.target.run_id == run
                && captured.target.conversation_id == conversation
                && captured.action
                    == (Action::ClearContext {
                        dismiss_prompt: dismiss,
                    })
        })
    }

    fn take_if(
        &self,
        reference: &Reference,
        matches: impl FnOnce(&Captured) -> bool,
    ) -> Result<(Captured, Arc<Claim>), String> {
        let mut model = lock(&self.0);
        let group = model.groups.get_mut(&reference.id).ok_or(STATE_CHANGED)?;
        let member = group
            .members
            .get_mut(reference.index)
            .ok_or(STATE_CHANGED)?;
        if !group.ready || member.used || !matches(&member.captured) {
            return Err(STATE_CHANGED.into());
        }
        let captured = member.captured.clone();
        let claim = Arc::clone(
            group
                .claims
                .get(&(
                    captured.target.computer.clone(),
                    captured.target.name.clone(),
                ))
                .ok_or(STATE_CHANGED)?,
        );
        member.used = true;
        if group.members.iter().all(|m| m.used) {
            model.groups.remove(&reference.id);
        }
        Ok((captured, claim))
    }

    fn cancel(&self, id: &str) {
        let mut model = lock(&self.0);
        // Undo can beat async command registration. Its UUID remains cancelled
        // even then; a late waiter must not recreate the action.
        if creation::canonical_uuid(id) && model.seen.len() < REGISTRATIONS {
            model.seen.insert(id.into());
        }
        if let Some(mut group) = model.groups.remove(id) {
            group.window.cancel();
        }
        self.0.changed.notify_all();
    }

    /// Called before any exit flush, and on close/destroy. A permanent quit
    /// also refuses later IPC admission; hiding/minimizing does not call this.
    pub(crate) fn cancel_all(&self, quitting: bool) {
        let mut model = lock(&self.0);
        model.closed |= quitting;
        model.groups.clear();
        self.0.changed.notify_all();
    }
}

fn main_only(window: &Webview) -> Result<(), String> {
    if window.label() != crate::MAIN_WINDOW {
        return Err(STATE_CHANGED.into());
    }
    Ok(())
}

/// Synchronous admission orders the confirmation against main-window close.
/// No hgs, filesystem access, source read or automatic dispatch occurs here.
#[tauri::command]
pub fn action_delay_begin(
    window: Webview,
    state: State<'_, Delays>,
    actions: State<'_, Actions>,
    id: String,
    captures: Vec<Captured>,
) -> Result<(), String> {
    main_only(&window)?;
    if !window.window().is_visible().unwrap_or(false)
        && !window.window().is_minimized().unwrap_or(false)
    {
        return Err(STATE_CHANGED.into());
    }
    state.begin(&actions, id, captures)
}

#[tauri::command]
pub async fn action_delay_wait(
    window: Webview,
    state: State<'_, Delays>,
    id: String,
) -> Result<bool, String> {
    main_only(&window)?;
    let delays = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delays.wait(&id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn action_delay_cancel(
    window: Webview,
    state: State<'_, Delays>,
    id: String,
) -> Result<(), String> {
    main_only(&window)?;
    state.cancel(&id);
    Ok(())
}

#[cfg(test)]
mod tests;
