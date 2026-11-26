//! Composer-owned queue. Enqueue writes the editable draft and its new position
//! in the queue together. Only a durably reserved head can reach native Send.
use super::{Composer, Composers, Hub, MAX_TEXT_BYTES};
use serde::Serialize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use tauri::State as TauriState;
use tokio::sync::oneshot;
use zerus_client_core::attachments::Draft;
use zerus_client_core::drafts::StoredDraft;
use zerus_client_core::drafts::outbox::{self, Entry, State};
use zerus_client_core::send::{BlockFacts, block_reason};
use zerus_client_core::{Action, Outcome, Target, ViewRow};

const CHECK_INTERVAL: f64 = 3.0;
const BUSY: &str = "The draft could not be queued. Its text is still in the editor.";
pub(super) fn reserved(c: &Composer) -> bool {
    c.outbox
        .first()
        .is_some_and(|head| head.state == State::Reserved)
}

const CHANGED: &str = "The session changed. The queued message was kept in Zerus.";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EntryView {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<f64>,
    pub preview: String,
    pub state: State,
    pub target_changed: bool,
}

pub(super) fn view(
    entries: &[Entry],
    key: &str,
    context: Option<&super::send::Context>,
) -> Vec<EntryView> {
    entries
        .iter()
        .map(|entry| EntryView {
            request_id: entry.request_id.clone(),
            queued_at: entry.queued_at,
            preview: entry.text.chars().take(140).collect(),
            state: entry.state,
            target_changed: entry.draft_key != key
                || context.is_some_and(|ctx| {
                    let active = target(ctx);
                    !active.run_id.is_empty()
                        && !active.conversation_id.is_empty()
                        && (entry.target != active || entry.catalog_key != ctx.session.key)
                }),
        })
        .collect()
}

fn target(ctx: &super::send::Context) -> Target {
    Target {
        computer: ctx.session.computer.clone(),
        name: ctx.session.name.clone(),
        run_id: ctx.run(),
        conversation_id: ctx.conversation(),
    }
}

fn queue_target(ctx: &super::send::Context) -> bool {
    ctx.online
        && ctx.session.subagent.is_none()
        && !ctx.session.archived()
        && matches!(ctx.row.agent.as_str(), "codex" | "kimi")
        && ctx.row.process_state == "running"
        && ctx.reason().is_none()
}

impl Composers {
    pub(super) fn offers_outbox(&self, hub: &Hub, key: &str) -> bool {
        let Some(ctx) = self
            .context(hub)
            .filter(|ctx| ctx.key == key && queue_target(ctx))
        else {
            return false;
        };
        let c = &hub.composers[key];
        let target = target(&ctx);
        let busy = ctx
            .details
            .as_ref()
            .is_some_and(|d| d["activity"] == "busy");
        !target.run_id.is_empty()
            && !target.conversation_id.is_empty()
            && (busy || ctx.ordinary_input_held(hub) || !c.outbox.is_empty())
            && c.outbox.first().is_none_or(|head| {
                head.target == target
                    && head.draft_key == key
                    && head.catalog_key == ctx.session.key
            })
    }

    /// The last editor flush is ordered before this call. Epoch, revision and
    /// handle protect a newer draft or a selection change during the flush.
    pub(super) fn enqueue_outbox(
        &self,
        handle: u64,
        rev: u64,
        epoch: u64,
        text: &str,
        done: oneshot::Sender<Result<super::ComposerView, String>>,
    ) {
        let mut hub = self.lock();
        let result = self.enqueue_locked(&mut hub, handle, rev, epoch, text);
        let (key, before, stored) = match result {
            Ok(result) => result,
            Err(reason) => {
                let _ = done.send(Err(reason.into()));
                return;
            }
        };
        let me = self.clone();
        self.0.store.enqueue_outbox(&key, before, stored.clone(), {
            let key = key.clone();
            move |result| {
                let mut hub = me.lock();
                let c = hub
                    .composers
                    .get_mut(&key)
                    .expect("queue write keeps the draft");
                c.queue_editing = false;
                let view = match result {
                    Ok(()) => {
                        c.outbox = stored.outbox;
                        c.draft = stored.draft;
                        c.position = 0;
                        c.anchor = 0;
                        c.text_epoch += 1;
                        c.removed = None;
                        c.offer = None;
                        c.attachments_changed();
                        c.draft_cleared();
                        c.storage_error = None;
                        c.save_failed = false;
                        hub.prompts.retain(|_, prompt| prompt.key != key);
                        me.view_of(&hub, &key).ok_or_else(|| BUSY.into())
                    }
                    Err(error) => Err(error),
                };
                drop(hub);
                me.changed();
                let _ = done.send(view);
            }
        });
    }

    fn enqueue_locked(
        &self,
        hub: &mut Hub,
        handle: u64,
        rev: u64,
        epoch: u64,
        text: &str,
    ) -> Result<(String, StoredDraft, StoredDraft), &'static str> {
        let Some(ctx) = self.context(hub) else {
            return Err(BUSY);
        };
        let Some(c) = hub.composers.get(&ctx.key) else {
            return Err(BUSY);
        };
        if hub.closed
            || c.handle != handle
            || !c.loaded
            || c.busy()
            || c.queue_writing
            || reserved(c)
            || c.uncertain
            || c.storage_error.is_some()
            || c.rev != rev
            || c.text_epoch != epoch
            || c.draft.text != text
            || !c.has_content()
            || text.len() > MAX_TEXT_BYTES
            || !self.question_part(hub).1
            || !self.offers_outbox(hub, &ctx.key)
        {
            return Err(BUSY);
        }
        if !outbox::capacity(&c.outbox, text, &c.draft.attachments) {
            return Err("The local message queue is full. The draft is still in the editor.");
        }
        let before = c.stored();
        let mut stored = before.clone();
        stored.outbox.push(Entry {
            request_id: uuid::Uuid::new_v4().to_string(),
            queued_at: outbox::enqueue_time(self.now()),
            target: target(&ctx),
            catalog_key: ctx.session.key,
            draft_key: ctx.key.clone(),
            text: text.to_owned(),
            attachments: c.draft.attachments.clone(),
            state: State::Waiting,
        });
        stored.draft = Draft::default();
        stored.position = 0;
        stored.anchor = 0;
        let c = hub.composers.get_mut(&ctx.key).expect("checked above");
        c.queue_editing = true;
        Ok((ctx.key, before, stored))
    }

    pub(super) fn outbox_waiting(hub: &Hub) -> bool {
        hub.composers.iter().any(|(key, c)| {
            c.outbox
                .first()
                .is_some_and(|head| head.state == State::Waiting && &head.draft_key == key)
        })
    }

    pub(super) fn tick_outbox(&self, now: f64) {
        let mut hub = self.lock();
        if hub.closed || hub.outbox_probe {
            return;
        }
        let Some((key, c)) = hub
            .composers
            .iter_mut()
            .filter(|(key, c)| {
                c.queue_check_at <= now
                    && !c.busy()
                    && !c.uncertain
                    && !c.queue_writing
                    && c.storage_error.is_none()
                    && c.outbox
                        .first()
                        .is_some_and(|head| head.state == State::Waiting && &head.draft_key == *key)
            })
            .min_by(|(_, a), (_, b)| a.queue_check_at.total_cmp(&b.queue_check_at))
        else {
            return;
        };
        c.queue_check_at = now + CHECK_INTERVAL;
        let key = key.clone();
        let entry = c.outbox[0].clone();
        hub.outbox_probe = true;
        drop(hub);
        let me = self.clone();
        (self.0.deps.spawn)(Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| me.check_outbox(&entry)));
            let mut hub = me.lock();
            hub.outbox_probe = false;
            if let Ok(Some(details)) = result {
                me.reserve_head(&mut hub, key, entry, details);
            }
        }));
    }

    fn outbox_row(&self, entry: &Entry) -> Option<ViewRow> {
        let t = &entry.target;
        let (row, online) = (self.0.deps.rows)(&t.computer, &entry.catalog_key, &t.name)?;
        (online
            && row.name == t.name
            && row.run_id.as_deref() == Some(&t.run_id)
            && row.conversation_id.as_deref() == Some(&t.conversation_id)
            && row.process_state == "running"
            && row.state == "running"
            && matches!(row.agent.as_str(), "codex" | "kimi")
            && (self.0.deps.facts)(&t.computer, &t.name, &t.run_id).is_none())
        .then_some(row)
    }

    fn check_outbox(&self, entry: &Entry) -> Option<serde_json::Value> {
        let row = self.outbox_row(entry)?;
        // Wait for an eligible idle turn. A present native queue, question,
        // or unconfirmed earlier input gives no permission to type.
        if row.phase.as_deref() != Some("idle") {
            return None;
        }
        (self.0.deps.inspect)(&entry.target, None, None).ok()
    }

    fn ready(&self, entry: &Entry, details: &serde_json::Value) -> bool {
        let Some(row) = self.outbox_row(entry) else {
            return false;
        };
        row.phase.as_deref() == Some("idle")
            && super::send::previous_message_pending(
                &row,
                (&entry.target.run_id, &entry.target.conversation_id),
                details,
            ) == Some(false)
            && details["activity"] == "idle"
            && details["phase"] == "idle"
            && details["input_queue"].is_null()
            && details["pending_questions_complete"] != false
            && details["pending_questions"]
                .as_array()
                .is_none_or(Vec::is_empty)
            && !zerus_client_core::cache::expired(details, self.now())
            && block_reason(
                &BlockFacts {
                    online: true,
                    state: row.state,
                    cmd: row.agent,
                    ..Default::default()
                },
                Some(details),
            )
            .is_none()
    }

    fn reserve_head(&self, hub: &mut Hub, key: String, entry: Entry, details: serde_json::Value) {
        if hub.closed || !self.ready(&entry, &details) {
            return;
        }
        let Some(c) = hub.composers.get_mut(&key) else {
            return;
        };
        if c.busy()
            || c.queue_writing
            || c.uncertain
            || c.outbox.first() != Some(&entry)
            || c.storage_error.is_some()
        {
            return;
        }
        c.queue_writing = true;
        c.outbox[0].state = State::Reserved;
        let me = self.clone();
        self.0.store.save_outbox(&key, c.outbox.clone(), {
            let key = key.clone();
            move |saved| {
                let mut hub = me.lock();
                let Some(c) = hub.composers.get_mut(&key) else {
                    return;
                };
                c.queue_writing = false;
                if let Err(error) = saved {
                    c.outbox[0].state = State::Waiting;
                    c.storage_error = Some(error);
                    drop(hub);
                    me.changed();
                    return;
                }
                if hub.closed {
                    return;
                }
                drop(hub);
                me.changed();
                let work = me.clone();
                (me.0.deps.spawn)(Box::new(move || work.dispatch_head(key, entry, details)));
            }
        });
    }

    fn dispatch_head(&self, key: String, entry: Entry, details: serde_json::Value) {
        // Once this reservation has answered, there is only one attempt.
        // With no matching receipt, the queue stops at the original message.
        let ready = {
            let hub = self.lock();
            !hub.closed
                && hub.composers.get(&key).is_some_and(|c| {
                    c.outbox.first().is_some_and(|head| {
                        head.state == State::Reserved && head.request_id == entry.request_id
                    })
                })
                && self.ready(&entry, &details)
        };
        let outcome = if ready {
            let action = Action::SendWhenIdle {
                text: entry.text.clone(),
                attachments: entry.attachments.clone(),
            };
            match catch_unwind(AssertUnwindSafe(|| {
                (self.0.deps.dispatch)(entry.target.clone(), action, entry.request_id.clone())
            })) {
                Ok(Ok(outcome)) => outcome,
                _ => Outcome::Uncertain {
                    detail: "The send receipt is unavailable.".into(),
                },
            }
        } else {
            Outcome::Failed {
                detail: CHANGED.into(),
            }
        };
        self.settle_head(key, entry, outcome);
    }

    fn settle_head(&self, key: String, entry: Entry, outcome: Outcome) {
        let mut hub = self.lock();
        let Some(c) = hub.composers.get_mut(&key) else {
            return;
        };
        if c.outbox
            .first()
            .is_none_or(|head| head.state != State::Reserved || head.request_id != entry.request_id)
        {
            return;
        }
        let receipt = match &outcome {
            Outcome::Done { receipt, .. }
                if receipt["request_id"] == entry.request_id
                    && receipt["name"] == entry.target.name
                    && receipt["run_id"] == entry.target.run_id
                    && receipt["conversation_id"] == entry.target.conversation_id =>
            {
                Some(receipt)
            }
            _ => None,
        };
        let exact = receipt.is_some_and(|receipt| receipt["status"] == "submitted");
        let busy = receipt.is_some_and(|receipt| {
            receipt["status"] == "waiting_for_idle" && receipt["proof"] == "native_input_lock"
        });
        if exact {
            c.outbox.remove(0);
        } else {
            c.outbox[0].state = if busy {
                State::Waiting
            } else if matches!(outcome, Outcome::Failed { .. }) {
                State::Failed
            } else {
                State::Unknown
            };
        }
        c.queue_writing = true;
        let me = self.clone();
        self.0.store.save_outbox(&key, c.outbox.clone(), {
            let key = key.clone();
            move |saved| {
                let mut hub = me.lock();
                let c = hub
                    .composers
                    .get_mut(&key)
                    .expect("queue write keeps the draft");
                c.queue_writing = false;
                if let Err(error) = saved {
                    c.storage_error = Some(error);
                } else {
                    c.queue_check_at = me.now() + CHECK_INTERVAL;
                    if exact {
                        hub.local.sending(
                            &key,
                            &entry.request_id,
                            None,
                            &entry.text,
                            &[],
                            &entry.target.run_id,
                            &entry.target.conversation_id,
                            me.now(),
                        );
                        hub.local.finished(&key, &entry.request_id, &outcome);
                    }
                }
                drop(hub);
                me.changed();
            }
        });
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn composer_enqueue(
    state: TauriState<'_, Composers>,
    handle: u64,
    rev: u64,
    epoch: u64,
    text: String,
) -> Result<super::ComposerView, String> {
    let (tx, rx) = oneshot::channel();
    state.enqueue_outbox(handle, rev, epoch, &text, tx);
    rx.await.map_err(|_| BUSY.to_owned())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn composer_remove_waiting(
    state: TauriState<'_, Composers>,
    handle: u64,
    request_id: String,
) -> Result<super::ComposerView, String> {
    let (tx, rx) = oneshot::channel();
    state.remove_waiting(handle, &request_id, tx);
    rx.await.map_err(|_| BUSY.to_owned())?
}

impl Composers {
    fn remove_waiting(
        &self,
        handle: u64,
        request_id: &str,
        done: oneshot::Sender<Result<super::ComposerView, String>>,
    ) {
        let mut hub = self.lock();
        let key = hub.open_key.clone();
        let Some(c) = Self::editable(&mut hub, handle)
            .filter(|c| key.as_ref() == Some(&c.key) && !reserved(c) && !c.queue_writing)
        else {
            let _ = done.send(Err(BUSY.into()));
            return;
        };
        let mut entries = c.outbox.clone();
        let Some(index) = entries.iter().position(|row| row.request_id == request_id) else {
            let _ = done.send(Err(BUSY.into()));
            return;
        };
        entries.remove(index);
        c.queue_editing = true;
        let key = c.key.clone();
        let me = self.clone();
        self.0.store.save_outbox(&key, entries.clone(), {
            let key = key.clone();
            move |result| {
                let mut hub = me.lock();
                let c = hub
                    .composers
                    .get_mut(&key)
                    .expect("queue write keeps the draft");
                c.queue_editing = false;
                let reply = match result {
                    Ok(()) => {
                        c.outbox = entries;
                        c.queue_check_at = me.now() + CHECK_INTERVAL;
                        me.view_of(&hub, &key).ok_or_else(|| BUSY.into())
                    }
                    Err(error) => Err(error),
                };
                drop(hub);
                me.changed();
                let _ = done.send(reply);
            }
        });
    }
}

#[cfg(test)]
mod tests;
