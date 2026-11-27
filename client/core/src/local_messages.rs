//! Messages the client sent but the agent's own history does not show yet,
//! ported from the Qt client's `m_localMessages`: a row per send, kept while
//! it is sending, after it failed, or until the history shows it, at most 50
//! per session. In memory only, as in Qt; the view keeps them across a
//! webview reload.
//!
//! Reference: `tray/src/SessionsWindow.cpp` (`deliverMessage`,
//! `finishMessage`, `reconcileMessages`, `messageAction`, the rename paths,
//! and the voice composer's `messageChanged`).
use crate::actions::Outcome;
use crate::attachments::StoredFile;
use crate::drafts::outbox::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Rows kept per session; the oldest go first.
pub const MAX_PER_SESSION: usize = 50;
/// Sessions with rows. Past it, the least recently touched session without
/// a row still sending is dropped; a sending row's outcome is still due.
pub const MAX_SESSIONS: usize = 512;
pub const OPENCODE_SOURCE: &str = "opencode_native";
const CODEX_QUEUE_SOURCE: &str = "zerus_codex_queue";
const CLAUDE_OUTBOX_SOURCE: &str = "zerus_claude_outbox";
const DSH_OUTBOX_SOURCE: &str = "zerus_dsh_outbox";
/// An original durable FIFO request, which only an exact history receipt can join.
const OUTBOX_SOURCE: &str = "zerus_outbox";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Sending,
    Sent,
    Error,
}

/// One local message row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalMessage {
    /// The send's request ID, or `voice-{message ID}` for a voice message.
    pub id: String,
    pub text: String,
    pub status: Status,
    /// The cached files ([`StoredFile`]); after a send each also has the
    /// receipt's `request_id` and its `index`.
    pub attachments: Vec<Value>,
    pub run_id: String,
    pub conversation_id: String,
    /// Seconds since the epoch.
    pub submitted_at: f64,
    #[serde(default)]
    pub error: String,
    /// The failure may have delivered the message: never retried.
    #[serde(default)]
    pub uncertain: bool,
    /// The associated provider message, or the Zerus receipt ID for terminal sends.
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub submitted_text: Option<String>,
    /// The provenance of a provider-backed or voice row.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub voice: Option<Value>,
    /// The stopped run a desktop-service resume-then-send was recorded under,
    /// once its send is bound to the run the receipted resume returned
    /// (`run_id`). Until the agent's history shows it, the row belongs to
    /// either run, so an inspection of either keeps it (zerus-5wmh.66.4.9.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed_from: Option<String>,
}

/// The detail of a send that may have delivered, as Qt shows it.
pub fn uncertain_detail(error: &str) -> String {
    format!("Delivery is not confirmed. Check Terminal before retrying. {error}")
}

/// What reconciling removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciled {
    pub removed: usize,
}

/// Where Inspect on a failed row leads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inspect {
    NativeUi,
    Terminal,
    /// Refresh the session and show this notice.
    Refresh {
        notice: String,
    },
    /// Refresh the exact followed Antigravity Activity.
    Activity {
        run_id: String,
        conversation_id: String,
    },
}

/// Inspect for a failed row: DeepSeek's Native UI, else Terminal when the
/// session has one, else a refresh.
pub fn inspect(cmd: &str, terminal: bool, machine: &str) -> Inspect {
    if cmd == "dsh" {
        Inspect::NativeUi
    } else if terminal {
        Inspect::Terminal
    } else {
        Inspect::Refresh {
            notice: format!(
                "Refreshing the native session. Check its provider configuration on {machine} if the error remains."
            ),
        }
    }
}

/// The local rows of every session, by session key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalMessages {
    rows: HashMap<String, Vec<LocalMessage>>,
    /// When each session was last changed, by a counter.
    touched: HashMap<String, u64>,
    clock: u64,
}

fn files_value(files: &[StoredFile]) -> Vec<Value> {
    files
        .iter()
        .map(|file| {
            let mut value = serde_json::to_value(file).unwrap_or(Value::Null);
            // These are our prepared immutable copies, never a journal path.
            // The existing private-file protocol validates the cache location
            // again when read, including size, ownership and symlink checks.
            if file.image_size.is_some() {
                if let (Some(parent), Some(name)) =
                    (file.local_path.parent(), file.local_path.file_name())
                {
                    if let Some(digest) = parent.file_name() {
                        value["preview_path"] = json!(format!(
                            "cache/{}/{}",
                            digest.to_string_lossy(),
                            name.to_string_lossy()
                        ));
                    }
                }
            }
            value
        })
        .collect()
}

impl LocalMessages {
    pub fn rows(&self, key: &str) -> &[LocalMessage] {
        self.rows.get(key).map_or(&[], Vec::as_slice)
    }

    /// The keys of the sessions that have rows, at most [`MAX_SESSIONS`].
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().map(String::as_str)
    }

    fn touch(&mut self, key: &str) {
        self.clock += 1;
        self.touched.insert(key.to_owned(), self.clock);
    }

    fn drop_key(&mut self, key: &str) {
        self.rows.remove(key);
        self.touched.remove(key);
    }

    /// Keeps at most [`MAX_PER_SESSION`] rows in `key`, and at most
    /// [`MAX_SESSIONS`] sessions, never dropping `key` or a session with a
    /// row still sending.
    fn trim(&mut self, key: &str) {
        if let Some(rows) = self.rows.get_mut(key) {
            let excess = rows.len().saturating_sub(MAX_PER_SESSION);
            rows.drain(..excess);
            if rows.is_empty() {
                self.drop_key(key);
            }
        }
        while self.rows.len() > MAX_SESSIONS {
            let oldest = self
                .rows
                .iter()
                .filter(|(other, rows)| {
                    other.as_str() != key && rows.iter().all(|row| row.status != Status::Sending)
                })
                .min_by_key(|(other, _)| self.touched.get(other.as_str()).copied().unwrap_or(0))
                .map(|(other, _)| other.clone());
            match oldest {
                Some(oldest) => self.drop_key(&oldest),
                None => break,
            }
        }
    }

    /// A send started. Earlier failed rows of the same retry, or with the
    /// same text and files, are replaced.
    #[allow(clippy::too_many_arguments)]
    pub fn sending(
        &mut self,
        key: &str,
        request_id: &str,
        retry_id: Option<&str>,
        text: &str,
        files: &[StoredFile],
        run_id: &str,
        conversation_id: &str,
        now: f64,
        source: Option<&str>,
    ) {
        self.record_sending(
            key,
            request_id,
            retry_id,
            text,
            files_value(files),
            run_id,
            conversation_id,
            source,
            now,
        );
    }

    /// A submitted FIFO entry appears through the same outgoing row and
    /// receipt projection as an ordinary message. Its file metadata comes
    /// from the original entry. It has no editable-draft or cache path.
    pub fn sent_from_outbox(&mut self, key: &str, entry: &Entry, outcome: &Outcome, now: f64) {
        let files = entry.attachments.iter().map(|file| json!(file.info())).collect();
        let source = if entry.target.name.starts_with("codex/")
            && matches!(outcome, Outcome::Done { receipt, .. } if crate::actions::codex_outbox_submitted(receipt))
        {
            CODEX_QUEUE_SOURCE
        } else if entry.target.name.starts_with("opencode/") {
            OPENCODE_SOURCE
        } else if entry.target.name.starts_with("claude/") {
            CLAUDE_OUTBOX_SOURCE
        } else if entry.target.name.starts_with("dsh/") {
            DSH_OUTBOX_SOURCE
        } else {
            OUTBOX_SOURCE
        };
        self.record_sending(
            key,
            &entry.request_id,
            None,
            &entry.text,
            files,
            &entry.target.run_id,
            &entry.target.conversation_id,
            Some(source),
            now,
        );
        self.finished(key, &entry.request_id, outcome);
    }

    #[allow(clippy::too_many_arguments)]
    fn record_sending(
        &mut self,
        key: &str,
        request_id: &str,
        retry_id: Option<&str>,
        text: &str,
        attachments: Vec<Value>,
        run_id: &str,
        conversation_id: &str,
        source: Option<&str>,
        now: f64,
    ) {
        let rows = self.rows.entry(key.to_owned()).or_default();
        rows.retain(|row| row.status != Status::Error || Some(row.id.as_str()) != retry_id);
        rows.push(LocalMessage {
            id: request_id.into(),
            text: text.into(),
            status: Status::Sending,
            attachments,
            run_id: run_id.into(),
            conversation_id: conversation_id.into(),
            submitted_at: now,
            error: String::new(),
            uncertain: false,
            message_id: None,
            submitted_text: None,
            source: source.map(str::to_owned),
            voice: None,
            resumed_from: None,
        });
        self.touch(key);
        self.trim(key);
    }

    /// The row of `request_id` was sent to `run_id`: a send the desktop
    /// service bound to the run its own receipted resume returned
    /// (zerus-5wmh.66.4.9.3). The row was recorded under the stopped run it
    /// resumed; it keeps that one as [`LocalMessage::resumed_from`], so an
    /// inspection of either run keeps it until the agent's history shows it.
    /// Without this its next reconcile would drop it as another run's once
    /// it stops sending. Nothing else of the row changes.
    pub fn rebind_run(&mut self, key: &str, request_id: &str, run_id: &str) {
        let Some(row) = self
            .rows
            .get_mut(key)
            .and_then(|rows| rows.iter_mut().find(|row| row.id == request_id))
        else {
            return;
        };
        if row.run_id != run_id {
            row.resumed_from = Some(std::mem::replace(&mut row.run_id, run_id.into()));
            self.touch(key);
        }
    }

    /// A send ended. Returns the detail the composer shows, empty on
    /// success.
    pub fn finished(&mut self, key: &str, request_id: &str, outcome: &Outcome) -> String {
        self.finish(key, request_id, outcome, true)
    }

    /// `finished`, for a send whose uncertain detail is already a complete
    /// line of its own (the desktop service's): it is shown as is, without
    /// the [`uncertain_detail`] prefix of the direct path.
    pub fn finished_as_is(&mut self, key: &str, request_id: &str, outcome: &Outcome) -> String {
        self.finish(key, request_id, outcome, false)
    }

    fn finish(&mut self, key: &str, request_id: &str, outcome: &Outcome, prefix: bool) -> String {
        let detail = match outcome {
            Outcome::Done { .. } => String::new(),
            Outcome::Failed { detail } => detail.clone(),
            Outcome::Uncertain { detail } if prefix => uncertain_detail(detail),
            Outcome::Uncertain { detail } => detail.clone(),
        };
        if self.rows.contains_key(key) {
            self.touch(key);
        }
        let Some(row) = self
            .rows
            .get_mut(key)
            .and_then(|rows| rows.iter_mut().find(|row| row.id == request_id))
        else {
            return detail;
        };
        match outcome {
            Outcome::Done { receipt, .. } => {
                row.status = Status::Sent;
                row.error.clear();
                row.uncertain = false;
                let request = receipt.get("request_id").cloned().unwrap_or(Value::Null);
                row.message_id = request.as_str().map(str::to_owned);
                if row.source.as_deref() == Some(OPENCODE_SOURCE)
                    && receipt["transport"] == "native_opencode"
                    && receipt["status"] == "submitted"
                    && request == row.id
                    && receipt["run_id"] == row.run_id
                    && receipt["conversation_id"] == row.conversation_id
                {
                    if let Some(id) = receipt["native_message_id"]
                        .as_str()
                        .filter(|id| crate::actions::native_message_id(id))
                    {
                        row.message_id = Some(id.into());
                    }
                }
                if row.source.as_deref() == Some(CODEX_QUEUE_SOURCE) {
                    row.message_id = receipt["native_queued_message_id"].as_str().map(str::to_owned);
                }
                if row.source.as_deref() == Some(CLAUDE_OUTBOX_SOURCE)
                    && receipt["status"] == "submitted" && request == row.id
                    && receipt["run_id"] == row.run_id && receipt["conversation_id"] == row.conversation_id
                {
                    row.message_id = crate::actions::claude_outbox_prompt(receipt).map(str::to_owned);
                }
                if row.source.as_deref() == Some(DSH_OUTBOX_SOURCE)
                    && crate::actions::dsh_outbox_submitted(receipt)
                    && request == row.id && receipt["run_id"] == row.run_id
                    && receipt["conversation_id"] == row.conversation_id
                {
                    row.message_id = receipt["native_message_id"].as_str().map(str::to_owned);
                }
                for (index, file) in row.attachments.iter_mut().enumerate() {
                    file["request_id"] = request.clone();
                    file["index"] = json!(index);
                }
                row.submitted_text = receipt
                    .get("submitted_text")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(at) = receipt.get("submitted_at").and_then(Value::as_f64) {
                    row.submitted_at = at;
                }
            }
            Outcome::Failed { .. } | Outcome::Uncertain { .. } => {
                row.status = Status::Error;
                row.error = detail.clone();
                row.uncertain = matches!(outcome, Outcome::Uncertain { .. });
            }
        }
        detail
    }

    /// Drops rows the agent's own history now shows, and rows of another run
    /// or conversation (except those still sending). `details` and `events`
    /// are the session's current inspection.
    pub fn reconcile(&mut self, key: &str, details: &Value, events: &[Value]) -> Reconciled {
        let mut result = Reconciled::default();
        let Some(rows) = self.rows.get_mut(key) else {
            return result;
        };
        let run = details["run_id"].as_str().unwrap_or("");
        let conversation = details["conversation_id"].as_str().unwrap_or("");
        let listed = |list: &str, id: &Option<String>| {
            id.as_deref().is_some_and(|id| {
                !id.is_empty()
                    && details[list].as_array().is_some_and(|items| {
                        items.iter().any(|item| item["message_id"] == json!(id))
                    })
            })
        };
        rows.retain(|row| {
            if row.status == Status::Sending {
                return true;
            }
            // A resumed send's row belongs to both its runs until shown.
            let other_run =
                !run.is_empty() && row.run_id != run && row.resumed_from.as_deref() != Some(run);
            if other_run || (!conversation.is_empty() && row.conversation_id != conversation) {
                result.removed += 1;
                return false;
            }
            if row.source.as_deref() == Some("hgs_voice") {
                let shown = listed("voice_messages", &row.message_id);
                result.removed += usize::from(shown);
                return !shown;
            }
            if row.source.as_deref() == Some(CODEX_QUEUE_SOURCE) {
                // A later native turn can have a different ID and identical text.
                // Retain this bounded bridge until the durable receipt is visible.
                let shown = details["codex_queued_messages"].as_array().is_some_and(|messages| {
                    messages.iter().any(|event| event["type"] == "CodexQueueReceipt"
                        && event["source"] == "hgs_delivery" && event["status"] == "submitted"
                        && event["transport"] == "codex_queue" && event["request_id"] == row.id
                        && event["message_id"] == row.id && event["detail"] == row.text
                        && event["attachments"].is_array() && row.message_id.is_some()
                        && event["run_id"] == row.run_id && event["conversation_id"] == row.conversation_id
                        && event["native_queued_message_id"].as_str() == row.message_id.as_deref())
                });
                result.removed += usize::from(shown);
                return !shown;
            }
            if row.source.as_deref() == Some(OPENCODE_SOURCE) {
                // The native Activity window may arrive before or after the
                // acknowledgement. Neither a text match nor another request
                // is evidence about this row or its draft.
                let shown = row.status == Status::Sent
                    && events.iter().any(|event| opencode_echo(row, event));
                result.removed += usize::from(shown);
                return !shown;
            }
            if row.source.as_deref() == Some(CLAUDE_OUTBOX_SOURCE) {
                let Some(id) = row.message_id.as_deref() else { return true; };
                let shown = row.status == Status::Sent
                    && (row.attachments.is_empty() || details["attachment_messages"].as_array()
                        .is_some_and(|messages| messages.iter().any(|event| {
                            event["message_id"] == row.id && claude_outbox_echo(row, event, id)
                        })))
                    && events.iter().filter(|event| claude_outbox_echo(row, event, id)).take(2).count() == 1;
                result.removed += usize::from(shown);
                return !shown;
            }
            if row.source.as_deref() == Some(DSH_OUTBOX_SOURCE) {
                let Some(id) = row.message_id.as_deref() else { return true; };
                let exact = |event: &Value| event["dsh_message_id"] == id
                    && event["run_id"] == row.run_id && event["conversation_id"] == row.conversation_id;
                let shown = row.status == Status::Sent
                    && (row.attachments.is_empty() || details["attachment_messages"].as_array()
                        .is_some_and(|messages| messages.iter().any(|event| {
                            event["dsh_outbox"] == true && event["message_id"] == row.id && exact(event)
                        })))
                    && events.iter().filter(|event| {
                        event["type"] == "UserPromptSubmit" && event["source"] == "dsh_native"
                            && event["dsh_request_id"] == row.id && exact(event)
                    }).take(2).count() == 1;
                result.removed += usize::from(shown);
                return !shown;
            }
            if details["backend"] == "agy" {
                // A saved Antigravity offer is written before its receipt.
                // Only preserved delivery evidence can settle this draft.
                let exact = |event: &&Value| {
                    event["source"] == "agy_acp"
                        && !row.id.is_empty()
                        && event["request_id"] == row.id
                        && event.get("run_id").is_none_or(|run| *run == row.run_id)
                        && event
                            .get("conversation_id")
                            .is_none_or(|id| *id == row.conversation_id)
                };
                let shown = (row.status == Status::Sent || row.uncertain)
                    && events.iter().filter(exact).any(|event| {
                        event["type"] == "UserPromptSubmit"
                            && (event["delivery_state"] == "submitted"
                                || (event.get("delivery_state").is_none()
                                    && events
                                        .iter()
                                        .filter(exact)
                                        .any(|reply| reply["type"] == "AgentMessage")))
                    });
                result.removed += usize::from(shown);
                return !shown;
            }
            if listed("attachment_messages", &row.message_id) {
                result.removed += 1;
                return false;
            }
            // A file card stays until the durable attachment metadata arrives.
            if !row.attachments.is_empty() {
                return true;
            }
            let shown = events.iter().filter(|event| {
                event["run_id"] == row.run_id && event["conversation_id"] == row.conversation_id
                    && ((event["type"] == "UserPromptSubmit"
                        && event["source"] == "hgs_delivery"
                        && event["message_id"] == row.id)
                        || (event["type"] == "TurnStarted"
                            && event["turn_origin"] == "user"
                            && event["kimi_request_id"] == row.id
                            && event["kimi_turn_id"].is_u64()
                            && event["kimi_conversation_id"] == row.conversation_id))
            }).take(2).count() == 1;
            result.removed += usize::from(shown);
            !shown
        });
        if rows.is_empty() {
            self.drop_key(key);
        }
        result
    }

    /// The row Retry would send: a definite failure of the run and
    /// conversation the session has now. The caller also checks that the
    /// session takes messages and that no retry read is in flight.
    pub fn retryable(
        &self,
        key: &str,
        id: &str,
        run_id: &str,
        conversation_id: &str,
    ) -> Option<&LocalMessage> {
        self.rows(key).iter().find(|row| {
            row.id == id
                && row.status == Status::Error
                && !row.uncertain
                && row.run_id == run_id
                && row.conversation_id == conversation_id
        })
    }

    /// The retried row's cached files, to read back with
    /// [`crate::attachments::read_stored`].
    pub fn stored_files(row: &LocalMessage) -> Option<Vec<StoredFile>> {
        row.attachments
            .iter()
            .map(|file| serde_json::from_value(file.clone()).ok())
            .collect()
    }

    /// Delete removes a failed row only.
    pub fn delete(&mut self, key: &str, id: &str) -> bool {
        let Some(rows) = self.rows.get_mut(key) else {
            return false;
        };
        let before = rows.len();
        rows.retain(|row| !(row.id == id && row.status == Status::Error));
        let removed = rows.len() != before;
        if rows.is_empty() {
            self.drop_key(key);
        } else if removed {
            self.touch(key);
        }
        removed
    }

    /// A voice message the voice composer submitted or updated.
    pub fn voice_message(&mut self, key: &str, message: &Value) {
        let id = message["id"].as_str().unwrap_or("").to_owned();
        let rows = self.rows.entry(key.to_owned()).or_default();
        rows.retain(|row| row.message_id.as_deref() != Some(id.as_str()));
        rows.push(LocalMessage {
            id: format!("voice-{id}"),
            text: message["transcript"].as_str().unwrap_or("").into(),
            status: Status::Sent,
            attachments: vec![message["audio"].clone()],
            run_id: message["run_id"].as_str().unwrap_or("").into(),
            conversation_id: message["conversation_id"].as_str().unwrap_or("").into(),
            submitted_at: message["created_at"].as_f64().unwrap_or(0.0),
            error: String::new(),
            uncertain: false,
            message_id: Some(id),
            submitted_text: None,
            source: Some("hgs_voice".into()),
            voice: Some(message.clone()),
            resumed_from: None,
        });
        self.touch(key);
        self.trim(key);
    }

    /// A session renamed in place, or a run continued under a new key: its
    /// rows move unless the new key already has rows of its own.
    pub fn rename(&mut self, old: &str, new: &str) {
        if self.rows.get(new).is_some_and(|rows| !rows.is_empty()) {
            return;
        }
        if let Some(rows) = self.rows.remove(old) {
            self.touched.remove(old);
            self.rows.insert(new.to_owned(), rows);
            self.touch(new);
        }
    }

    /// Drops a session's rows, as when it leaves the catalog.
    pub fn forget(&mut self, key: &str) {
        self.drop_key(key);
    }
}

fn claude_outbox_echo(row: &LocalMessage, event: &Value, id: &str) -> bool {
    event["type"] == "UserPromptSubmit" && event["claude_prompt_id"] == id
        && event["claude_outbox_request_id"] == row.id
        && event["run_id"] == row.run_id && event["conversation_id"] == row.conversation_id
        && event["agent_id"].as_str().unwrap_or("").is_empty()
        && event["origin"].as_str().unwrap_or("").is_empty()
        && matches!(event["turn_origin"].as_str(), None | Some("" | "user"))
}

fn opencode_echo(row: &LocalMessage, event: &Value) -> bool {
    let Some(native_id) = event["message_id"]
        .as_str()
        .filter(|id| crate::actions::native_message_id(id))
    else {
        return false;
    };
    if event["source"] != OPENCODE_SOURCE
        || event["type"] != "UserPromptSubmit"
        || !event["agent_id"].as_str().unwrap_or("").is_empty()
        || event.get("run_id").is_some_and(|run| *run != row.run_id)
        || event
            .get("conversation_id")
            .is_some_and(|id| *id != row.conversation_id)
        || event.get("request_id").is_some_and(|id| *id != row.id)
    {
        return false;
    }
    match row
        .message_id
        .as_deref()
        .filter(|id| crate::actions::native_message_id(id))
    {
        Some(expected) => native_id == expected,
        None => event["request_id"] == row.id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "studio\nclaude/sample-project/review";

    fn file(name: &str) -> StoredFile {
        StoredFile {
            name: name.into(),
            mime: "text/plain".into(),
            bytes: 5,
            reference: "[File #1]".into(),
            local_path: "/home/example/.cache/attachments/abc/a.txt".into(),
            image_size: None,
        }
    }

    fn send(messages: &mut LocalMessages, id: &str, text: &str, files: &[StoredFile]) {
        messages.sending(KEY, id, None, text, files, "run-1", "conv-1", 100.0, None);
    }

    #[test]
    fn codex_queue_keeps_original_text_and_files_until_its_durable_receipt_arrives() {
        let mut messages = LocalMessages::default();
        let id = "00000001-2222-4444-8888-000000000001";
        let native = "00000002-2222-4444-8888-000000000002";
        let mut receipt = json!({"transport":"codex_queue","status":"submitted","codex_outbox":true,
            "request_id":id,"native_queued_message_id":native,"run_id":"run-1","conversation_id":"conv-1"});
        messages.sending(KEY, id, None, "same", &[file("note.txt")], "run-1", "conv-1", 100.0, Some(CODEX_QUEUE_SOURCE));
        messages.finished(KEY, id, &Outcome::Done { receipt: receipt.clone(), cleanup: None });
        let echo = json!({"type":"UserPromptSubmit","detail":"same","at":100});
        let details = json!({"run_id":"run-1","conversation_id":"conv-1","attachment_messages":[{"message_id":id}]});
        assert_eq!(messages.reconcile(KEY, &details, &[echo]).removed, 0);
        let row = &messages.rows(KEY)[0];
        assert_eq!(row.message_id.as_deref(), Some(native));
        assert_eq!(row.attachments[0]["request_id"], id);
        receipt["type"] = json!("CodexQueueReceipt");
        receipt["source"] = json!("hgs_delivery");
        receipt["message_id"] = json!(id);
        receipt["detail"] = json!("same");
        receipt["attachments"] = json!(row.attachments);
        for (key, wrong) in [("request_id","other"), ("native_queued_message_id","other"), ("run_id","other"), ("conversation_id","other")] {
            let mut other = receipt.clone();
            other[key] = json!(wrong);
            assert_eq!(messages.reconcile(KEY, &json!({"codex_queued_messages":[other]}), &[]).removed, 0);
        }
        assert_eq!(messages.reconcile(KEY, &json!({"codex_queued_messages":[receipt]}), &[]).removed, 1);
    }

    #[test]
    fn opencode_reconciles_only_the_confirmed_native_id_or_exact_request() {
        let mut messages = LocalMessages::default();
        for id in ["one", "two", "uncertain", "refused"] {
            messages.sending(
                KEY,
                id,
                None,
                "same text",
                &[],
                "run-1",
                "conv-1",
                100.0,
                Some(OPENCODE_SOURCE),
            );
        }
        for (id, native) in [("one", "msg_one"), ("two", "msg_two")] {
            messages.finished(KEY, id, &Outcome::Done {
                receipt: json!({"transport":"native_opencode","status":"submitted","request_id":id,
                    "run_id":"run-1","conversation_id":"conv-1","native_message_id":native}),
                cleanup: None,
            });
            assert_eq!(
                messages
                    .rows(KEY)
                    .iter()
                    .find(|row| row.id == id)
                    .unwrap()
                    .message_id
                    .as_deref(),
                Some(native)
            );
        }
        messages.finished(
            KEY,
            "uncertain",
            &Outcome::Uncertain {
                detail: "unknown".into(),
            },
        );
        messages.finished(
            KEY,
            "refused",
            &Outcome::Failed {
                detail: "refused".into(),
            },
        );
        let row = |native, request, conversation| {
            json!({"type":"UserPromptSubmit","source":OPENCODE_SOURCE,
            "message_id":native,"request_id":request,"conversation_id":conversation,"detail":"same text","at":100})
        };
        let details = json!({"run_id":"run-1","conversation_id":"conv-1"});
        let wrong = [
            row("msg_other", "two", "conv-1"),
            row("msg_two", "wrong", "conv-1"),
            row("msg_one", "one", "conv-other"),
            row("msg_uncertain", "uncertain", "conv-1"),
            row("msg_refused", "refused", "conv-1"),
        ];
        assert_eq!(messages.reconcile(KEY, &details, &wrong).removed, 0);
        let old =
            json!({"type":"UserPromptSubmit","source":OPENCODE_SOURCE,"message_id":"msg_one"});
        assert_eq!(messages.reconcile(KEY, &details, &[old]).removed, 1);
        assert_eq!(
            messages
                .rows(KEY)
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["two", "uncertain", "refused"]
        );
        let result = messages.reconcile(KEY, &details, &[row("msg_two", "two", "conv-1")]);
        assert_eq!(result.removed, 1);
        assert!(messages.rows(KEY)[0].uncertain);
        assert!(!messages.rows(KEY)[1].uncertain);
    }

    #[test]
    fn antigravity_offer_and_other_request_proof_keep_the_uncertain_row() {
        let mut messages = LocalMessages::default();
        send(&mut messages, "current", "identical prompt", &[]);
        messages.finished(
            KEY,
            "current",
            &Outcome::Uncertain {
                detail: "missing proof".into(),
            },
        );
        let details = json!({"backend":"agy","run_id":"run-1","conversation_id":"conv-1"});
        let offer = json!({"source":"agy_acp","type":"UserPromptSubmit","request_id":"current",
            "delivery_state":"unconfirmed","detail":"identical prompt","at":100});
        let other = json!({"source":"agy_acp","type":"UserPromptSubmit","request_id":"prior",
            "delivery_state":"submitted","detail":"identical prompt","at":100});
        let error = json!({"source":"agy_acp","type":"ProviderError","request_id":"current"});
        let mut wrong_run = offer.clone();
        wrong_run["run_id"] = json!("other-run");
        wrong_run["delivery_state"] = json!("submitted");
        let mut wrong_conversation = offer.clone();
        wrong_conversation["conversation_id"] = json!("other-conversation");
        wrong_conversation["delivery_state"] = json!("submitted");
        let mut legacy = offer.clone();
        legacy.as_object_mut().unwrap().remove("delivery_state");
        for events in [
            [
                offer.clone(),
                other.clone(),
                error.clone(),
                wrong_run,
                wrong_conversation,
            ]
            .as_slice(),
            [legacy, other, error].as_slice(),
        ] {
            let result = messages.reconcile(KEY, &details, events);
            assert_eq!(result.removed, 0);
                assert_eq!(messages.rows(KEY)[0].id, "current");
            assert!(messages.rows(KEY)[0].uncertain);
        }
        let mut confirmed = offer;
        confirmed["delivery_state"] = json!("submitted");
        let result = messages.reconcile(KEY, &details, &[confirmed]);
        assert_eq!(result.removed, 1);
    }

    /// zerus-5wmh.66.4.9.3: a row sent to the run a receipted resume returned
    /// survives the reconcile of that run's inspection once it is sent, until
    /// the agent's history shows it; an unbound one is dropped as another
    /// run's.
    #[test]
    fn a_row_rebound_to_its_resumed_run_survives_that_runs_reconcile() {
        let ok = Outcome::Done {
            receipt: json!({"request_id": "r1"}),
            cleanup: None,
        };
        let resumed = json!({"run_id": "run-2", "conversation_id": "conv-1"});
        for rebound in [true, false] {
            let mut messages = LocalMessages::default();
            send(&mut messages, "r1", "after resume", &[]);
            if rebound {
                messages.rebind_run(KEY, "r1", "run-2");
            }
            messages.finished(KEY, "r1", &ok);
            messages.reconcile(KEY, &resumed, &[]);
            assert_eq!(messages.rows(KEY).len(), usize::from(rebound), "{rebound}");
        }
        // An inspection that still shows the stopped run keeps it too: the
        // row belongs to either run until the history shows it.
        let mut messages = LocalMessages::default();
        send(&mut messages, "r1", "after resume", &[]);
        messages.rebind_run(KEY, "r1", "run-2");
        messages.finished(KEY, "r1", &ok);
        let stale = json!({"run_id": "run-1", "conversation_id": "conv-1"});
        messages.reconcile(KEY, &stale, &[]);
        messages.reconcile(KEY, &resumed, &[]);
        assert_eq!(messages.rows(KEY).len(), 1);
        assert_eq!(messages.rows(KEY)[0].resumed_from.as_deref(), Some("run-1"));
        // A third run, or another conversation, is not this message's.
        let mut third = messages.clone();
        third.reconcile(
            KEY,
            &json!({"run_id": "run-3", "conversation_id": "conv-1"}),
            &[],
        );
        assert!(third.rows(KEY).is_empty());
        let mut other = messages.clone();
        other.reconcile(
            KEY,
            &json!({"run_id": "run-2", "conversation_id": "conv-2"}),
            &[],
        );
        assert!(other.rows(KEY).is_empty());
        // A row that was never rebound serializes exactly as before.
        let mut plain = LocalMessages::default();
        send(&mut plain, "r2", "plain", &[]);
        assert!(!serde_json::to_string(&plain.rows(KEY)[0])
            .unwrap()
            .contains("resumed_from"));
        // Rebound, it goes once the resumed run's history shows it.
        let mut messages = LocalMessages::default();
        send(&mut messages, "r1", "after resume", &[]);
        messages.rebind_run(KEY, "r1", "run-2");
        messages.finished(KEY, "r1", &ok);
        let shown = [json!({"type":"UserPromptSubmit","source":"hgs_delivery","message_id":"r1",
            "run_id":"run-2","conversation_id":"conv-1","detail":"after resume","at":101.0})];
        messages.reconcile(KEY, &resumed, &shown);
        assert!(messages.rows(KEY).is_empty());
        // Another request's row is never rebound.
        let mut messages = LocalMessages::default();
        send(&mut messages, "r1", "after resume", &[]);
        messages.rebind_run(KEY, "other", "run-2");
        assert_eq!(messages.rows(KEY)[0].run_id, "run-1");
    }

    #[test]
    fn keys_name_every_session_with_rows_and_follow_its_bound() {
        let mut messages = LocalMessages::default();
        assert_eq!(messages.keys().count(), 0);
        for n in 0..MAX_SESSIONS + 3 {
            let key = format!("studio\nclaude/sample-project/s{n}");
            messages.sending(&key, "1", None, "hi", &[], "run-1", "conv-1", 100.0, None);
            messages.finished(
                &key,
                "1",
                &Outcome::Failed {
                    detail: "refused".into(),
                },
            );
        }
        let keys: Vec<String> = messages.keys().map(str::to_owned).collect();
        assert_eq!(keys.len(), MAX_SESSIONS);
        assert!(keys.iter().all(|key| !messages.rows(key).is_empty()));
        assert!(
            !keys
                .iter()
                .any(|key| key == "studio\nclaude/sample-project/s0"),
            "the oldest went"
        );
        messages.forget(&keys[0]);
        assert_eq!(messages.keys().count(), MAX_SESSIONS - 1);
    }

    #[test]
    fn a_send_is_kept_until_finished_and_failures_are_replaced_by_their_retry() {
        let mut messages = LocalMessages::default();
        send(&mut messages, "1", "hello", &[]);
        assert_eq!(messages.rows(KEY)[0].status, Status::Sending);
        let detail = messages.finished(
            KEY,
            "1",
            &Outcome::Failed {
                detail: "no such session".into(),
            },
        );
        assert_eq!(detail, "no such session");
        let row = &messages.rows(KEY)[0];
        assert_eq!((row.status, row.uncertain), (Status::Error, false));
        assert!(messages.retryable(KEY, "1", "run-1", "conv-1").is_some());
        assert!(messages.retryable(KEY, "1", "run-2", "conv-1").is_none());
        // The retry replaces the failed row.
        messages.sending(
            KEY,
            "2",
            Some("1"),
            "hello",
            &[],
            "run-1",
            "conv-1",
            101.0,
            None,
        );
        assert_eq!(
            messages
                .rows(KEY)
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["2"]
        );
        let detail = messages.finished(
            KEY,
            "2",
            &Outcome::Uncertain {
                detail: "Delivery timed out.".into(),
            },
        );
        assert_eq!(
            detail,
            "Delivery is not confirmed. Check Terminal before retrying. Delivery timed out."
        );
        assert!(messages.rows(KEY)[0].uncertain);
        assert!(
            messages.retryable(KEY, "2", "run-1", "conv-1").is_none(),
            "never retried"
        );
        // A distinct ordinary Send leaves the equal-text original uncertain.
        messages.sending(KEY, "3", None, "hello", &[], "run-1", "conv-1", 102.0, None);
        assert_eq!(messages.rows(KEY).iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["2", "3"]);
        assert!(!messages.delete(KEY, "3"), "a sending row is not deleted");
        // A complete uncertain line of its own is shown as is: still
        // uncertain and never retried, without the direct path's prefix.
        let line =
            "Delivery is not confirmed by the desktop service. Check Terminal before retrying.";
        let detail = messages.finished_as_is(
            KEY,
            "3",
            &Outcome::Uncertain {
                detail: line.into(),
            },
        );
        assert_eq!(detail, line);
        let row = &messages.rows(KEY)[1];
        assert_eq!(
            (row.status, row.uncertain, row.error.as_str()),
            (Status::Error, true, line)
        );
        assert!(messages.retryable(KEY, "3", "run-1", "conv-1").is_none());
    }

    #[test]
    fn a_receipt_names_the_message_and_its_files() {
        let mut messages = LocalMessages::default();
        send(
            &mut messages,
            "1",
            "see [File #1]",
            &[file("a.txt"), file("b.txt")],
        );
        let receipt = json!({"request_id": "req-1", "submitted_text": "see [File #1] (2 files)", "submitted_at": 105.0});
        let detail = messages.finished(
            KEY,
            "1",
            &Outcome::Done {
                receipt,
                cleanup: None,
            },
        );
        assert_eq!(detail, "");
        let row = &messages.rows(KEY)[0];
        assert_eq!(row.status, Status::Sent);
        assert_eq!(row.message_id.as_deref(), Some("req-1"));
        assert_eq!(row.submitted_at, 105.0);
        assert_eq!(
            (
                row.attachments[1]["request_id"].clone(),
                row.attachments[1]["index"].clone()
            ),
            (json!("req-1"), json!(1))
        );
        assert_eq!(LocalMessages::stored_files(row).unwrap()[0].name, "a.txt");
    }

    #[test]
    fn only_exact_history_retires_the_original() {
        let mut messages = LocalMessages::default();
        send(&mut messages, "1", "  ship it  ", &[]);
        messages.finished(
            KEY,
            "1",
            &Outcome::Done {
                receipt: json!({"request_id": "m1"}),
                cleanup: None,
            },
        );
        send(&mut messages, "2", "exact failure", &[]);
        messages.finished(KEY, "2", &Outcome::Failed { detail: "x".into() });
        send(&mut messages, "3", "with a file", &[file("a.txt")]);
        messages.finished(
            KEY,
            "3",
            &Outcome::Done {
                receipt: json!({"request_id": "m3"}),
                cleanup: None,
            },
        );
        send(&mut messages, "4", "still sending", &[]);
        let details = json!({"run_id": "run-1", "conversation_id": "conv-1"});
        let events = [
            json!({"type":"UserPromptSubmit","source":"hgs_delivery","message_id":"1",
                "run_id":"run-1","conversation_id":"conv-1","detail":"ship it and more","at":101.0}),
            json!({"type": "UserPromptSubmit", "detail": "exact failure", "at": 99.0}),
            // A subagent's prompt or one long before the send never matches.
            json!({"type": "TurnStarted", "detail": "with a file", "at": 101.0, "agent_id": "a"}),
        ];
        let reconciled = messages.reconcile(KEY, &details, &events);
        assert_eq!(reconciled.removed, 1);
        let left: Vec<_> = messages
            .rows(KEY)
            .iter()
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(left, ["2", "3", "4"]);
        // The durable attachment metadata removes the file card.
        let details = json!({"run_id": "run-1", "conversation_id": "conv-1", "attachment_messages": [{"message_id": "m3"}]});
        assert_eq!(messages.reconcile(KEY, &details, &[]).removed, 1);
        // A new conversation drops the rest except what is still sending.
        messages.finished(KEY, "4", &Outcome::Failed { detail: "x".into() });
        send(&mut messages, "5", "fresh", &[]);
        let details = json!({"run_id": "run-1", "conversation_id": "conv-2"});
        messages.reconcile(KEY, &details, &[]);
        let left: Vec<_> = messages
            .rows(KEY)
            .iter()
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(left, ["5"]);
    }

    #[test]
    fn equal_or_normalized_text_does_not_confirm_an_unknown_original() {
        let details = json!({"run_id":"run-1","conversation_id":"conv-1"});
        let mut messages = LocalMessages::default();
        send(&mut messages, "1", "a\tb\tc", &[]);
        messages.finished(KEY, "1", &Outcome::Uncertain { detail: "lost response".into() });
        let prompt = json!({"type":"TurnStarted","detail":"a    b    c","turn_origin":"user",
            "run_id":"run-1","conversation_id":"conv-1","kimi_conversation_id":"conv-1","kimi_turn_id":7});
        assert_eq!(messages.reconcile(KEY, &details, &[prompt.clone()]).removed, 0);
        let mut exact = prompt;
        exact["kimi_request_id"] = json!("1");
        let mut foreign = exact.clone();
        foreign["run_id"] = json!("other");
        assert_eq!(messages.reconcile(KEY, &details, &[foreign]).removed, 0);
        assert!(messages.rows(KEY)[0].uncertain);
        assert_eq!(messages.reconcile(KEY, &details, &[exact]).removed, 1);
    }

    #[test]
    fn sessions_are_capped_without_dropping_a_send_in_flight() {
        let mut messages = LocalMessages::default();
        let key = |index: usize| format!("studio\nclaude/p/s{index}");
        // The first session still has a send in flight; the second failed.
        messages.sending(
            &key(0),
            "r0",
            None,
            "pending",
            &[],
            "run",
            "conv",
            1.0,
            None,
        );
        messages.sending(&key(1), "r1", None, "failed", &[], "run", "conv", 1.0, None);
        messages.finished(&key(1), "r1", &Outcome::Failed { detail: "x".into() });
        for index in 2..MAX_SESSIONS {
            messages.sending(&key(index), "r", None, "old", &[], "run", "conv", 1.0, None);
            messages.finished(&key(index), "r", &Outcome::Failed { detail: "x".into() });
        }
        // Touching the failed session keeps it newer than the rest.
        messages.finished(
            &key(1),
            "r1",
            &Outcome::Failed {
                detail: "again".into(),
            },
        );
        messages.sending(
            &key(MAX_SESSIONS),
            "r",
            None,
            "new",
            &[],
            "run",
            "conv",
            1.0,
            None,
        );
        assert_eq!(messages.rows.len(), MAX_SESSIONS);
        assert_eq!(messages.touched.len(), MAX_SESSIONS);
        assert_eq!(messages.rows(&key(0)).len(), 1, "a send in flight stays");
        assert_eq!(messages.rows(&key(1)).len(), 1, "recently touched stays");
        assert!(
            messages.rows(&key(2)).is_empty(),
            "the least recently touched goes"
        );
        assert_eq!(messages.rows(&key(MAX_SESSIONS)).len(), 1);
        messages.forget(&key(0));
        assert_eq!(
            (messages.rows.len(), messages.touched.len()),
            (MAX_SESSIONS - 1, MAX_SESSIONS - 1)
        );
    }

    #[test]
    fn voice_rows_bounds_renames_and_inspect() {
        let mut messages = LocalMessages::default();
        let voice = json!({"id": "v1", "transcript": "hello", "run_id": "run-1", "conversation_id": "conv-1",
                           "created_at": 100.0, "audio": {"name": "voice.wav"}});
        messages.voice_message(KEY, &voice);
        messages.voice_message(KEY, &voice);
        assert_eq!(messages.rows(KEY).len(), 1, "an update replaces the row");
        assert_eq!(messages.rows(KEY)[0].id, "voice-v1");
        let details = json!({"run_id": "run-1", "conversation_id": "conv-1", "voice_messages": [{"message_id": "v1"}]});
        assert_eq!(messages.reconcile(KEY, &details, &[]).removed, 1);
        assert!(messages.rows(KEY).is_empty());
        for index in 0..60 {
            send(&mut messages, &index.to_string(), &format!("m{index}"), &[]);
        }
        assert_eq!(messages.rows(KEY).len(), MAX_PER_SESSION);
        assert_eq!(messages.rows(KEY)[0].id, "10");
        messages.rename(KEY, "studio\nclaude/sample-project/renamed");
        assert!(messages.rows(KEY).is_empty());
        assert_eq!(
            messages.rows("studio\nclaude/sample-project/renamed").len(),
            50
        );
        assert_eq!(inspect("dsh", true, "studio"), Inspect::NativeUi);
        assert_eq!(inspect("claude", true, "studio"), Inspect::Terminal);
        assert_eq!(
            inspect("opencode", false, "studio"),
            Inspect::Refresh {
                notice: "Refreshing the native session. Check its provider configuration on studio if the error remains.".into()
            }
        );
    }
}
