//! Composer-owned queue. Enqueue writes the editable draft and its new position
//! in the queue together. Only a durably reserved head can reach native Send.
use super::send::{Context, SendReply, exact_input_details, first_input_details};
use super::{Composer, Composers, Hub, MAX_TEXT_BYTES};
use serde::Serialize;
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};
use tauri::State as TauriState;
use tokio::sync::oneshot;
use zerus_client_core::attachments::{Attachment, AttachmentInfo, Draft};
use zerus_client_core::drafts::outbox::{self, Entry, State};
use zerus_client_core::send::{BlockFacts, block_reason};
use zerus_client_core::{Action, Outcome, Target, ViewRow};

const CHECK_INTERVAL: f64 = 3.0;
const BUSY: &str = "The draft could not be queued. Its text is still in the editor.";
const TERMINAL_SUBMITTED: &str = "Submitted to terminal";
pub(super) fn reserved(c: &Composer) -> bool {
    c.outbox
        .first()
        .is_some_and(|head| head.state == State::Reserved)
}

const CHANGED: &str = "The session changed. The queued message was kept in Zerus.";
const QUEUE_UNAVAILABLE: &str = zerus_client_core::source!(
    "This session cannot queue messages in Zerus. Wait for the agent to become ready."
);
const QUEUE_TARGET_CHANGED: &str =
    zerus_client_core::source!("The original session changed. The draft is still in the editor.");
const QUEUE_OLD_HOST: &str = zerus_client_core::source!(
    "Update hgs on this computer to queue messages. The draft stays in the editor."
);

/// Proof stays in memory and follows the reserved request to its child.
#[derive(Clone)]
pub(crate) struct OutboxAdmission {
    composers: Composers,
    revision: u64,
}

impl OutboxAdmission {
    pub(crate) fn spawn<T>(&self, child: impl FnOnce() -> T) -> Option<T> {
        // Observing a route change and starting the original child must have
        // a definite order. Release the hub as soon as the spawn returns,
        // leaving stdin, stdout and the wait to the ordinary runner.
        let hub = self.composers.lock();
        let started = (!hub.closed && hub.outbox_route_revision == self.revision).then(child);
        drop(hub);
        started
    }
}

pub(super) enum ManualSend {
    Direct,
    Wait,
    TryNow,
    TryNativeQueue,
    Blocked(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EntryView {
    pub request_id: String,
    pub run_id: String,
    pub conversation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<f64>,
    pub preview: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentInfo>,
    pub state: State,
    pub target_changed: bool,
    pub can_check_terminal: bool,
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
            run_id: entry.target.run_id.clone(),
            conversation_id: entry.target.conversation_id.clone(),
            queued_at: entry.queued_at,
            preview: {
                let mut text = entry.text.trim().chars();
                let mut preview: String = text.by_ref().take(240).collect();
                if text.next().is_some() {
                    preview.push('…');
                }
                preview
            },
            attachments: entry.attachments.iter().map(Attachment::info).collect(),
            state: entry.state,
            target_changed: entry.draft_key != key
                || context.is_some_and(|ctx| {
                    let active = target(ctx);
                    !active.run_id.is_empty()
                        && !active.conversation_id.is_empty()
                        && (entry.target != active || entry.catalog_key != ctx.session.key)
                }),
            can_check_terminal: entry.state == State::Unknown
                && !opencode_outbox(entry)
                && !dsh_outbox(entry)
                && entry.draft_key == key
                && context.is_some_and(|ctx| {
                    ctx.online
                        && !ctx.session.archived()
                        && ctx.session.subagent.is_none()
                        && entry.catalog_key == ctx.session.key
                        && entry.target == target(ctx)
                        && ctx.row.run_id.as_deref() == Some(&entry.target.run_id)
                        && ctx.row.conversation_id.as_deref() == Some(&entry.target.conversation_id)
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

fn supported_agent(agent: &str) -> bool {
    matches!(agent, "codex" | "kimi" | "opencode" | "claude" | "dsh" | "deepseek")
}

fn codex_work_phase(phase: Option<&str>) -> bool {
    matches!(phase, Some("working" | "tool" | "compacting"))
}

/// A Waiting entry came from the saved FIFO. An immediate ordinary send
/// owns a Reserved entry from the outset and keeps its existing transport.
fn idle_codex_queue(entry: &Entry, row: &ViewRow, details: &Value) -> bool {
    entry.state == State::Waiting && row.agent == "codex"
        && row.phase.as_deref() == Some("idle") && details["codex_idle_outbox_supported"] == true
}

fn dsh_agent(agent: &str) -> bool {
    matches!(agent, "dsh" | "deepseek")
}

fn opencode_outbox(entry: &Entry) -> bool {
    entry.target.name.starts_with("opencode/")
}

fn dsh_outbox(entry: &Entry) -> bool {
    entry.target.name.starts_with("dsh/")
}

fn terminal_native_agent(agent: &str) -> bool {
    matches!(agent, "claude" | "kimi")
}

fn terminal_native_supported(agent: &str, details: &Value) -> bool {
    terminal_native_agent(agent) && details["terminal_native_accept_supported"] == true
}

// Ready Kimi sessions retain the existing route even if their native history
// is too old or incomplete to establish the ordered working-turn floor.
fn terminal_native_route(agent: &str, details: &Value) -> bool {
    terminal_native_supported(agent, details) && (agent != "kimi" || details["activity"] == "busy")
}

fn receipt_action(entry: &Entry) -> Action {
    Action::SendWhenIdle { text: entry.text.clone(), attachments: entry.attachments.clone() }
}

pub(super) fn can_probe(entry: &Entry) -> bool {
    matches!(entry.state, State::Waiting | State::Unknown)
}

fn guarded_supported(agent: &str, details: &serde_json::Value) -> bool {
    details["guarded_idle_send"] == true
        && (agent != "claude" || details["claude_outbox_supported"] == true)
        && (!dsh_agent(agent) || details["dsh_outbox_supported"] == true)
}

fn provider_queue(agent: &str) -> bool {
    agent == "opencode" || dsh_agent(agent)
}

impl Context {
    pub(super) fn submission_notice(&self, composer: &Composer) -> Option<&'static str> {
        let (submission, notice) = composer.last_submission.as_ref()?;
        (composer.outbox.is_empty()
            && !self.awaiting_outbox_details
            && self.details.as_deref().is_some_and(|details| self.exact_outbox_details(details))
            && *submission == target(self)).then_some(*notice)
    }

    pub(super) fn outbox_candidate(&self) -> bool {
        self.session.subagent.is_none()
            && !self.session.archived()
            && supported_agent(&self.row.agent)
    }

    fn exact_outbox_details(&self, details: &Value) -> bool {
        self.row.name == self.session.name
            && exact_input_details(
                &self.row,
                (
                    self.row.run_id.as_deref().unwrap_or_default(),
                    self.row.conversation_id.as_deref().unwrap_or_default(),
                ),
                details,
            )
    }
}

impl Composers {
    // The ticker may have parked while a durable reservation was being
    // written. Its successor must run even when no editor is selected.
    fn outbox_changed(&self) {
        self.0.opened.notify_all();
        self.changed();
    }

    pub(crate) fn set_outbox_dispatch(&self, dispatch: super::OutboxDispatch) {
        assert!(self.0.outbox_dispatch.set(dispatch).is_ok());
    }

    pub(crate) fn set_outbox_status(&self, status: super::OutboxStatus) {
        assert!(self.0.outbox_status.set(status).is_ok());
    }

    fn offer_guarded(
        &self,
        entry: &Entry,
        action: Action,
        revision: u64,
    ) -> Result<Option<Outcome>, String> {
        let admission = OutboxAdmission {
            composers: self.clone(),
            revision,
        };
        let target = entry.target.clone();
        let request = entry.request_id.clone();
        if let Some(dispatch) = self.0.outbox_dispatch.get() {
            return dispatch(target, action, request, admission);
        }
        #[cfg(test)]
        return (self.0.deps.dispatch)(target, action, request).map(Some);
        #[cfg(not(test))]
        Err("Guarded Send is unavailable.".into())
    }

    pub(super) fn observe_outbox_details(&self, hub: &mut Hub, details: &Value) {
        if hub.followed.awaiting_outbox_details
            && self
                .context(hub)
                .is_some_and(|ctx| {
                    !ctx.outbox_candidate() || ctx.exact_outbox_details(details)
                        || (ctx.row.name == ctx.session.name && first_input_details(&ctx.row, details))
                })
        {
            hub.followed.awaiting_outbox_details = false;
        }
    }

    pub(super) fn manual_send(&self, hub: &Hub, ctx: &Context, c: &Composer) -> ManualSend {
        // Recovery asks for consent before offering input. Retained Activity
        // from the stopped run can still describe an earlier native queue.
        if Self::can_offer_start(hub, ctx) || ctx.session.subagent.is_some() || ctx.session.archived() {
            return ManualSend::Direct;
        }
        let details = ctx.details.as_deref().unwrap_or(&serde_json::Value::Null);
        let busy = details["activity"] == "busy"
            || matches!(
                ctx.row.phase.as_deref(),
                Some("tool" | "working" | "compacting")
            )
            || details["input_queue"].is_object()
            || dsh_agent(&ctx.row.agent) && details["dsh_outbox_ready"] == false;
        let supported = ctx.outbox_candidate() && ctx.online && ctx.row.process_state == "running";
        // A later Activity frame or catalog update can replace the initial run.
        // Recheck the binding at Send as well as when accepting a fresh read.
        let stale_first = details["first_message_can_send"] == true
            && ctx.row.agent == "codex" && details["conversation_id"].as_str().unwrap_or_default().is_empty()
            && (ctx.session.name != ctx.row.name || !first_input_details(&ctx.row, details));
        if supported && (ctx.awaiting_outbox_details || stale_first) {
            return ManualSend::Blocked(
                ctx.reason()
                    .unwrap_or_else(|| "Checking the session…".into()),
            );
        }
        let waiting = !c.outbox.is_empty() || busy || (supported && ctx.ordinary_input_held(hub));
        let target = target(ctx);
        let exact = ctx.exact_outbox_details(details);
        // A person's Send to the current working Codex turn uses its normal
        // guarded composer and Enter. A saved FIFO head keeps its own route.
        if supported && exact && c.outbox.is_empty()
            && ctx.row.agent == "codex" && codex_work_phase(ctx.row.phase.as_deref())
            && details["codex_manual_turn_send"] == true
            && !ctx.ordinary_input_held(hub)
            && self.guarded_send_details(&ctx.row, &target, details)
        {
            if details["codex_ordered_manual_turn_send"] == true {
                return ManualSend::TryNow;
            }
            return ManualSend::Direct;
        }
        let native_queue = exact && supported && self.native_queue_details(&ctx.row, &target, details);
        if waiting {
            if let Some(reason) = ctx.reason() {
                return ManualSend::Blocked(reason);
            }
            if !supported {
                return ManualSend::Blocked(QUEUE_UNAVAILABLE.into());
            }
            if !exact
                || c.outbox.first().is_some_and(|head| {
                    head.target != target
                        || head.draft_key != ctx.key
                        || head.catalog_key != ctx.session.key
                })
            {
                return ManualSend::Blocked(QUEUE_TARGET_CHANGED.into());
            }
            if !guarded_supported(&ctx.row.agent, details) {
                return ManualSend::Blocked(QUEUE_OLD_HOST.into());
            }
            if c.outbox.is_empty()
                && ((native_queue && !ctx.exact_earlier_input_hold(hub))
                    || (terminal_native_route(&ctx.row.agent, details) && !ctx.ordinary_input_held(hub)
                        && self.guarded_send_details(&ctx.row, &target, details)))
            {
                ManualSend::TryNativeQueue
            } else {
                ManualSend::Wait
            }
        } else if exact
            && supported
            && ctx.reason().is_none()
            && (native_queue || !dsh_agent(&ctx.row.agent) && self.guarded_send_details(&ctx.row, &target, details))
        {
            if native_queue { ManualSend::TryNativeQueue } else { ManualSend::TryNow }
        } else {
            ManualSend::Direct
        }
    }

    /// Choose from the currently selected session under the same lock that
    /// claims the editable draft. A ready attempt first saves a Reserved head.
    pub(super) fn offer_manual_send(
        &self,
        hub: &mut Hub,
        handle: u64,
        rev: u64,
        epoch: u64,
        text: &str,
    ) -> Result<Option<oneshot::Receiver<SendReply>>, SendReply> {
        let Some(ctx) = self.context(hub) else {
            return Ok(None);
        };
        let Some(c) = hub.composers.get(&ctx.key).filter(|c| c.handle == handle) else {
            return Err(SendReply::Busy);
        };
        let ready = match self.manual_send(hub, &ctx, c) {
            ManualSend::Direct => return Ok(None),
            ManualSend::Wait => None,
            ManualSend::TryNow | ManualSend::TryNativeQueue => ctx.details.clone(),
            ManualSend::Blocked(reason) => {
                return Err(SendReply::Notice {
                    text: reason,
                    error: true,
                });
            }
        };
        if hub.closed
            || !c.loaded
            || c.busy()
            || c.queue_writing
            || c.outbox_damaged
            || reserved(c)
            || c.uncertain
            || c.checkpoint.is_some()
            || c.storage_error.is_some()
            || c.rev != rev
            || c.text_epoch != epoch
            || c.draft.text != text
            || !c.has_content()
            || text.len() > MAX_TEXT_BYTES
            || !self.question_part(hub).1
            || (!c.draft.attachments.is_empty()
                && ctx
                    .details
                    .as_ref()
                    .is_some_and(|d| d["attachments_supported"] == false))
        {
            return Err(SendReply::Busy);
        }
        if !outbox::capacity(&c.outbox, text, &c.draft.attachments) {
            return Err(SendReply::Notice {
                text: "The local message queue is full. The draft is still in the editor.".into(),
                error: true,
            });
        }
        let before = c.stored();
        let mut stored = before.clone();
        let entry = Entry {
            target: target(&ctx),
            request_id: uuid::Uuid::new_v4().to_string(),
            queued_at: outbox::enqueue_time(self.now()),
            catalog_key: ctx.session.key,
            draft_key: ctx.key.clone(),
            text: text.to_owned(),
            attachments: c.draft.attachments.clone(),
            state: if ready.is_some() {
                State::Reserved
            } else {
                State::Waiting
            },
        };
        stored.outbox.push(entry.clone());
        stored.draft = Draft::default();
        stored.position = 0;
        stored.anchor = 0;
        let key = ctx.key;
        hub.composers
            .get_mut(&key)
            .expect("checked above")
            .queue_editing = true;
        let (done, accepted) = oneshot::channel();
        let route_revision = hub.outbox_route_revision;
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
                        me.view_of(&hub, &key)
                            .map_or(SendReply::Busy, |view| SendReply::Accepted {
                                view: Box::new(view),
                                waiting: ready.is_none(),
                            })
                    }
                    Err(error) => SendReply::Notice {
                        text: error,
                        error: true,
                    },
                };
                let dispatch = matches!(view, SendReply::Accepted { .. }) && !hub.closed;
                drop(hub);
                me.outbox_changed();
                let _ = done.send(view);
                if let Some(details) = ready.filter(|_| dispatch) {
                    let work = me.clone();
                    (me.0.deps.spawn)(Box::new(move || {
                        work.dispatch_head(key, entry, &details, route_revision);
                    }));
                }
            }
        });
        Ok(Some(accepted))
    }

    pub(crate) fn restore_saved_outboxes(&self) {
        self.lock().outbox_scanning = true;
        self.read_outbox_page(outbox::Scan::default());
    }

    fn read_outbox_page(&self, scan: outbox::Scan) {
        let me = self.clone();
        self.0.store.scan_outboxes(scan, move |next, key| {
            let mut hub = me.lock();
            hub.outbox_scanning = next.is_some();
            if hub.closed { return; }
            if let Some(key) = key.filter(|key| !hub.composers.contains_key(key)) {
                hub.order.push_back(key.clone());
                me.ensure(&mut hub, &key);
                // Give this queue a turn before paging more candidates into
                // the bounded composer cache. Selection stays with the person.
                hub.outbox_scan = next;
                me.0.opened.notify_all();
            } else if let Some(next) = next {
                drop(hub);
                me.read_outbox_page(next);
            }
        });
    }

    pub(super) fn outbox_waiting(hub: &Hub) -> bool {
        hub.outbox_scan.is_some() || hub.outbox_rescan_at.is_some() || hub.composers.iter().any(|(key, c)| {
            c.outbox
                .first()
                .is_some_and(|head| can_probe(head) && &head.draft_key == key)
        })
    }

    pub(super) fn tick_outbox(&self, now: f64) {
        let mut hub = self.lock();
        if !hub.closed {
            if let Some(scan) = hub.outbox_scan.take() {
                self.read_outbox_page(scan);
            } else if !hub.outbox_scanning && hub.outbox_rescan_at.is_some_and(|at| at <= now) {
                hub.outbox_rescan_at = None;
                hub.outbox_scanning = true;
                self.read_outbox_page(outbox::Scan::default());
            }
        }
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
                        .is_some_and(|head| can_probe(head) && &head.draft_key == *key)
            })
            .min_by(|(_, a), (_, b)| a.queue_check_at.total_cmp(&b.queue_check_at))
        else {
            return;
        };
        c.queue_check_at = now + CHECK_INTERVAL;
        let key = key.clone();
        let entry = c.outbox[0].clone();
        hub.outbox_probe = true;
        let route_revision = hub.outbox_route_revision;
        drop(hub);
        let me = self.clone();
        (self.0.deps.spawn)(Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                if entry.state == State::Unknown {
                    me.check_outbox_receipt(&entry)
                } else {
                    me.check_outbox(&entry)
                }
            }));
            let mut hub = me.lock();
            hub.outbox_probe = false;
            if hub.outbox_route_revision != route_revision {
                return;
            }
            if let Ok(Some(result)) = result {
                if entry.state == State::Unknown {
                    if me.outbox_row(&entry).is_some() {
                        let outcome = Outcome::Done { receipt: result, cleanup: None };
                        me.settle_head_locked(&mut hub, key, entry, Some(outcome), State::Unknown);
                    }
                } else {
                    me.reserve_head(&mut hub, key, entry, result);
                }
            }
        }));
    }

    fn outbox_row(&self, entry: &Entry) -> Option<ViewRow> {
        let t = &entry.target;
        let (row, online) = (self.0.deps.rows)(&t.computer, &entry.catalog_key, &t.name)?;
        (online
            && row.name == t.name
            && entry.draft_key.split_once("\nid:").is_none_or(|(_, id)| row.session_id.as_deref() == Some(id))
            && row.run_id.as_deref() == Some(&t.run_id)
            && row.conversation_id.as_deref() == Some(&t.conversation_id)
            && row.process_state == "running"
            && row.state == "running"
            && supported_agent(&row.agent)
            && (self.0.deps.facts)(&t.computer, &t.name, &t.run_id).is_none())
        .then_some(row)
    }

    fn check_outbox(&self, entry: &Entry) -> Option<serde_json::Value> {
        let row = self.outbox_row(entry)?;
        // Inspect eligible working terminals even when the editor is closed.
        // Every new route still requires its owning host's exact capability.
        let phase = row.phase.as_deref();
        let eligible = phase == Some("idle")
            || (row.agent == "codex" && (phase == Some("interrupted") || codex_work_phase(phase)))
            || (provider_queue(&row.agent) && matches!(phase, Some("working" | "tool")))
            || (terminal_native_agent(&row.agent) && matches!(phase, Some("working" | "tool" | "compacting")));
        if !eligible {
            return None;
        }
        (self.0.deps.inspect)(&entry.target, None, None).ok()
    }

    fn check_outbox_receipt(&self, entry: &Entry) -> Option<serde_json::Value> {
        self.outbox_row(entry)?;
        let status = self.0.outbox_status.get()?;
        // Each guarded provider route freezes the same stdin. This only reads the original receipt.
        status(&entry.target, &receipt_action(entry), &entry.request_id)
    }

    fn ready(&self, entry: &Entry, details: &serde_json::Value) -> bool {
        let Some(row) = self.outbox_row(entry) else {
            return false;
        };
        self.entry_ready(entry, &row, details)
    }

    fn entry_ready(&self, entry: &Entry, row: &ViewRow, details: &Value) -> bool {
        (self.native_queue_details(row, &entry.target, details)
            || self.guarded_send_details(row, &entry.target, details))
            && (!idle_codex_queue(entry, row, details)
                || (details["pending_questions_complete"] == true
                    && details["pending_questions"].as_array().is_some_and(Vec::is_empty)))
    }

    fn eligible_details(&self, row: &ViewRow, target: &Target, details: &Value) -> bool {
        guarded_supported(&row.agent, details)
            && exact_input_details(row, (&target.run_id, &target.conversation_id), details)
            && details["pending_questions_complete"] != false
            && details["pending_questions"].as_array().is_none_or(Vec::is_empty)
            && !zerus_client_core::cache::expired(details, self.now())
            && block_reason(
                &BlockFacts {
                    online: true,
                    state: row.state.clone(),
                    cmd: super::send::cmd(row),
                    ..Default::default()
                },
                Some(details),
            )
            .is_none()
    }

    fn native_queue_details(&self, row: &ViewRow, target: &Target, details: &Value) -> bool {
        let provider_ready = if row.agent == "opencode" && target.name.starts_with("opencode/") {
            details["opencode_busy_outbox_supported"] == true
        } else if dsh_agent(&row.agent) && target.name.starts_with("dsh/") {
            details["dsh_busy_outbox_supported"] == true && details["dsh_busy_outbox_ready"] == true
        } else {
            return false;
        };
        provider_ready
            && matches!(row.phase.as_deref(), Some("idle" | "working" | "tool"))
            && matches!(details["phase"].as_str(), Some("idle" | "working" | "tool"))
            && matches!(details["activity"].as_str(), Some("idle" | "busy"))
            && details["pending_questions"].as_array().is_some_and(Vec::is_empty)
            && self.eligible_details(row, target, details)
    }

    fn guarded_send_details(&self, row: &ViewRow, target: &Target, details: &Value) -> bool {
        let phase = row.phase.as_deref();
        let idle = phase == Some("idle") && details["phase"] == "idle" && details["activity"] == "idle";
        let questions_clear = details["pending_questions_complete"] == true
            && details["pending_questions"].as_array().is_some_and(Vec::is_empty);
        let codex_queue = row.agent == "codex" && codex_work_phase(phase)
            && codex_work_phase(details["phase"].as_str()) && details["activity"] == "busy"
            && details["codex_busy_outbox_supported"] == true && questions_clear;
        let interrupted = row.agent == "codex" && phase == Some("interrupted")
            && details["phase"] == "interrupted" && details["activity"] == "unknown"
            && details["codex_interrupted_outbox_supported"] == true
            && details["interrupted_message_can_send"] == true
            && questions_clear;
        let native = terminal_native_supported(&row.agent, details);
        let working = native
            && matches!(phase, Some("idle" | "working" | "tool" | "compacting"))
            && matches!(details["phase"].as_str(), Some("idle" | "working" | "tool" | "compacting"))
            && matches!(details["activity"].as_str(), Some("idle" | "busy"));
        (idle || interrupted || codex_queue || working)
            && self.eligible_details(row, target, details)
            && (dsh_agent(&row.agent) || super::send::previous_message_pending(
                row,
                (&target.run_id, &target.conversation_id),
                details,
            ) == Some(false))
            // The new hosts check their native queue against the current
            // composer frame while admitting the offer.
            && (terminal_native_route(&row.agent, details) || details["input_queue"].is_null())
            && (!dsh_agent(&row.agent) || details["dsh_outbox_ready"] == true)
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
        let route_revision = hub.outbox_route_revision;
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
                    me.outbox_changed();
                    return;
                }
                if hub.closed {
                    return;
                }
                drop(hub);
                me.outbox_changed();
                let work = me.clone();
                (me.0.deps.spawn)(Box::new(move || {
                    work.dispatch_head(key, entry, &details, route_revision);
                }));
            }
        });
    }

    fn dispatch_head(
        &self,
        key: String,
        entry: Entry,
        details: &serde_json::Value,
        route_revision: u64,
    ) {
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
        };
        if !ready {
            return;
        }
        if self.lock().outbox_route_revision != route_revision {
            self.settle_head(key, entry, None);
            return;
        }
        let outcome = if let Some(row) = self.outbox_row(&entry) {
            if !self.entry_ready(&entry, &row, details)
                || self.lock().outbox_route_revision != route_revision
            {
                self.settle_head(key, entry, None);
                return;
            }
            let action = if entry.state == State::Reserved && row.agent == "codex"
                && (row.phase.as_deref() == Some("idle") || codex_work_phase(row.phase.as_deref()))
                && details["codex_ordered_manual_turn_send"] == true {
                Action::SendToOrderedCodexTurn { text: entry.text.clone(), attachments: entry.attachments.clone() }
            } else if (row.agent == "codex" && codex_work_phase(row.phase.as_deref()))
                || idle_codex_queue(&entry, &row, details)
            {
                Action::SendToCodexQueue { text: entry.text.clone(), attachments: entry.attachments.clone() }
            } else if self.native_queue_details(&row, &entry.target, details) {
                Action::SendToNativeQueue { text: entry.text.clone(), attachments: entry.attachments.clone() }
            } else if terminal_native_route(&row.agent, details) {
                Action::SendWhenNativeAccepts { text: entry.text.clone(), attachments: entry.attachments.clone() }
            } else {
                receipt_action(&entry)
            };
            match catch_unwind(AssertUnwindSafe(|| {
                self.offer_guarded(&entry, action, route_revision)
            })) {
                Ok(Ok(outcome)) => outcome,
                _ => Some(Outcome::Uncertain {
                    detail: "The send receipt is unavailable.".into(),
                }),
            }
        } else {
            Some(Outcome::Failed {
                detail: CHANGED.into(),
            })
        };
        self.settle_head(key, entry, outcome);
    }

    // None means this process retained the reservation and made no native call.
    fn settle_head(&self, key: String, entry: Entry, outcome: Option<Outcome>) {
        self.settle_head_locked(&mut self.lock(), key, entry, outcome, State::Reserved);
    }

    fn settle_head_locked(
        &self,
        hub: &mut Hub,
        key: String,
        entry: Entry,
        outcome: Option<Outcome>,
        expected: State,
    ) {
        let Some(c) = hub.composers.get_mut(&key) else {
            return;
        };
        if c.outbox
            .first()
            .is_none_or(|head| {
                head.state != expected || head.request_id != entry.request_id
            })
            || c.queue_editing || c.queue_writing
        {
            return;
        }
        let receipt = match &outcome {
            Some(Outcome::Done { receipt, .. })
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
        let provider_queued = exact && receipt.is_some_and(|receipt| receipt["queued"] == true)
            && (opencode_outbox(&entry) || dsh_outbox(&entry));
        let terminal_refusal = receipt.is_some_and(|receipt|
            zerus_client_core::actions::terminal_outbox_refusal(&entry.target, receipt));
        if expected == State::Unknown && !exact && !terminal_refusal {
            return;
        }
        let terminal_only = exact
            && receipt.is_some_and(|receipt| receipt["transport"] == "terminal")
            && (entry.target.name.starts_with("codex/") && entry.attachments.is_empty()
                || entry.target.name.starts_with("kimi/") && entry.attachments.is_empty()
                    && receipt.is_some_and(|receipt| receipt["kimi_outbox"] == true)
                || entry.target.name.starts_with("claude/") && receipt.is_some_and(|receipt| {
                    zerus_client_core::actions::claude_outbox_prompt(receipt).is_none()
                }));
        let codex_queued = exact && receipt.is_some_and(zerus_client_core::actions::codex_outbox_submitted);
        let notice = if codex_queued {
            Some(zerus_client_core::source!("Queued in Codex"))
        } else {
            terminal_only.then_some(TERMINAL_SUBMITTED)
        };
        let native_accepted = exact && receipt.is_some_and(|receipt| {
            receipt["kimi_outbox"] == true || (receipt["claude_outbox"] == true && receipt["native"] == "queue")
        });
        let busy = outcome.is_none()
            || receipt.is_some_and(|receipt| {
                receipt["status"] == "waiting_for_idle"
                    && receipt["proof"] == if opencode_outbox(&entry) {
                        "native_opencode_queue"
                    } else if dsh_outbox(&entry) {
                        "native_dsh_idle_guard"
                    } else {
                        "native_input_lock"
                    }
            });
        let mut entries = c.outbox.clone();
        if exact {
            entries.remove(0);
        } else {
            entries[0].state = if busy {
                State::Waiting
            } else if terminal_refusal || matches!(outcome, Some(Outcome::Failed { .. })) {
                State::Failed
            } else {
                State::Unknown
            };
        }
        c.queue_writing = true;
        let me = self.clone();
        self.0.store.save_outbox(&key, entries.clone(), {
            let key = key.clone();
            move |saved| {
                let mut hub = me.lock();
                let c = hub
                    .composers
                    .get_mut(&key)
                    .expect("queue write keeps the draft");
                c.queue_writing = false;
                let next_ready = (codex_queued || provider_queued || native_accepted) && saved.is_ok()
                    && entries.first().is_some_and(|next| next.state == State::Waiting);
                if let Err(error) = saved {
                    if exact {
                        // The on-disk reservation is still held. A failed
                        // write never makes an offered request eligible again.
                        c.outbox[0].state = State::Unknown;
                    } else {
                        c.outbox = entries;
                    }
                    c.storage_error = Some(error);
                } else {
                    // Publish the pending-to-sent handoff together. A
                    // catalog update during the write keeps the head visible.
                    c.outbox = entries;
                    // Confirmed queue admission frees the next FIFO head immediately.
                    c.queue_check_at = if next_ready { me.now() } else { me.now() + CHECK_INTERVAL };
                    c.last_submission = notice.map(|text| (entry.target.clone(), text));
                    if exact {
                        if let Some(outcome) = outcome.as_ref() {
                            hub.local.sent_from_outbox(&key, &entry, outcome, me.now());
                        }
                    }
                }
                drop(hub);
                me.outbox_changed();
                if next_ready {
                    // Use the same bounded, fair scheduler and a fresh inspection.
                    me.tick_outbox(me.now());
                }
            }
        });
    }
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
                me.outbox_changed();
                let _ = done.send(reply);
            }
        });
    }
}

#[cfg(test)]
mod tests;
