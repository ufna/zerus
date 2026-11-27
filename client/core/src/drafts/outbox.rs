//! Drafts waiting in Zerus to enter a particular native conversation.
use super::{
    MAX_MANIFEST, atomic_write, folder, not_saved, private, prune_blobs, read_attachments,
    read_bounded, write_attachments,
};
use crate::Target;
use crate::attachments::{Attachment, MAX_TOTAL_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 16;
const MAX_TEXT_BYTES: usize = 80 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Waiting,
    Reserved,
    Unknown,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub request_id: String,
    /// Unix seconds from the desktop's original enqueue, absent for older entries.
    pub queued_at: Option<f64>,
    pub target: Target,
    pub catalog_key: String,
    /// If a draft move leaves two copies during recovery, only the original
    /// manifest is eligible to dispatch. A copied entry stays data.
    pub draft_key: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub state: State,
}

pub fn enqueue_time(seconds: f64) -> Option<f64> {
    (seconds > 0.0 && seconds < 253_402_300_800.0).then_some(seconds)
}

pub fn capacity(entries: &[Entry], text: &str, attachments: &[Attachment]) -> bool {
    entries.len() < MAX_ENTRIES
        && entries.iter().map(|entry| entry.text.len()).sum::<usize>() + text.len()
            <= MAX_TEXT_BYTES
        && attachment_bytes(entries) + bytes(attachments) <= MAX_TOTAL_BYTES
}

fn bytes(files: &[Attachment]) -> usize {
    files.iter().map(|file| file.bytes.len()).sum()
}

pub fn attachment_bytes(entries: &[Entry]) -> usize {
    entries.iter().map(|entry| bytes(&entry.attachments)).sum()
}

fn valid(entries: &[Entry]) -> bool {
    let ids: HashSet<_> = entries.iter().map(|entry| &entry.request_id).collect();
    ids.len() == entries.len()
        && entries.len() <= MAX_ENTRIES
        && entries.iter().map(|entry| entry.text.len()).sum::<usize>() <= MAX_TEXT_BYTES
        && attachment_bytes(entries) <= MAX_TOTAL_BYTES
        && entries.iter().enumerate().all(|(index, entry)| {
            uuid::Uuid::parse_str(&entry.request_id)
                .is_ok_and(|id| id.to_string() == entry.request_id)
                && !entry.catalog_key.is_empty()
                && !entry.draft_key.is_empty()
                && !entry.target.name.is_empty()
                && !entry.target.run_id.is_empty()
                && !entry.target.conversation_id.is_empty()
                && (!entry.text.trim().is_empty() || !entry.attachments.is_empty())
                && (index == 0 || entry.state == State::Waiting)
        })
}

pub(super) struct Queue {
    pub(super) entries: Vec<Entry>,
    pub(super) consumed_draft: Option<String>,
}

// The previous desktop prunes attachment blobs only inside the editable
// draft's folder. Its writer never sees these per-queue directories.
fn queue_dir(root: &Path, key: &str) -> PathBuf {
    folder(&root.join("outbox"), key)
}

mod discovery;
pub use discovery::Scan;

fn read_manifest(path: &Path, key: &str) -> Result<Option<Value>, ()> {
    let bytes = match read_bounded(&path.join("queue.json"), MAX_MANIFEST) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if value["version"] != 1
        || value["key"] != key
        || !value["entries"].is_array()
        || value.get("consumed_draft").is_some_and(|claim| {
            !claim.as_str().is_some_and(|id| {
                uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id)
            })
        })
    {
        return Err(());
    }
    Ok(Some(value))
}

pub(super) fn load(root: &Path, key: &str) -> Result<Option<Queue>, ()> {
    let path = queue_dir(root, key);
    let manifest = match read_manifest(&path, key) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => return Ok(None),
        Err(()) => {
            // An obstruction at or above the draft root means there is no
            // queue there yet. The ordinary writer can still retry its save.
            // An unreadable queue below an existing root stays damaged.
            return match fs::metadata(root) {
                Ok(metadata) if !metadata.is_dir() => Ok(None),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    Ok(None)
                }
                _ => Err(()),
            };
        }
    };
    Ok(Some(Queue {
        entries: decode(&path, &manifest["entries"]).ok_or(())?,
        consumed_draft: manifest["consumed_draft"].as_str().map(str::to_owned),
    }))
}

pub(super) fn preserve_inline(root: &Path, key: &str, entries: &[Entry]) -> Result<(), String> {
    let path = queue_dir(root, key);
    match read_manifest(&path, key) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => save(root, key, entries, None),
        Err(()) => Err(not_saved("the local message queue is unreadable")),
    }
}

pub(super) fn save(
    root: &Path,
    key: &str,
    entries: &[Entry],
    claim: Option<&str>,
) -> Result<(), String> {
    let path = queue_dir(root, key);
    let previous = read_manifest(&path, key)
        .map_err(|()| not_saved("the local message queue is unreadable"))?;
    let claim = claim.or_else(|| previous.as_ref()?.get("consumed_draft")?.as_str());
    fs::create_dir_all(&path).map_err(not_saved)?;
    if !private(root, 0o700) || !private(&root.join("outbox"), 0o700) || !private(&path, 0o700) {
        return Err(not_saved("cannot create private queue storage"));
    }
    let mut retained = HashSet::new();
    let mut manifest =
        json!({"version": 1, "key": key, "entries": encode(&path, entries, &mut retained)?});
    if let Some(claim) = claim {
        manifest["consumed_draft"] = json!(claim);
    }
    let bytes = manifest.to_string();
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(not_saved("the local message queue is full"));
    }
    atomic_write(&path.join("queue.json"), bytes.as_bytes())?;
    prune_blobs(&path, &retained);
    Ok(())
}

fn encode(path: &Path, entries: &[Entry], retained: &mut HashSet<String>) -> Result<Value, String> {
    if !valid(entries) {
        return Err(not_saved("invalid local message queue"));
    }
    let queued = entries
        .iter()
        .map(|entry| {
            let files = write_attachments(path, &entry.attachments, retained)?;
            let mut row = json!({
                "request_id": entry.request_id, "target": entry.target,
                "catalog_key": entry.catalog_key, "draft_key": entry.draft_key, "text": entry.text,
                "attachments": files, "state": entry.state,
            });
            if let Some(queued_at) = entry.queued_at {
                row["queued_at"] = json!(queued_at);
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Value::Array(queued))
}

pub(super) fn decode(path: &Path, value: &Value) -> Option<Vec<Entry>> {
    let rows = value.as_array()?;
    if rows.len() > MAX_ENTRIES {
        return None;
    }
    let mut entries = Vec::with_capacity(rows.len());
    let mut attachment_total = 0usize;
    for row in rows {
        let files = row["attachments"].as_array()?;
        // Limit reads as well as the saved queue, even if a damaged manifest
        // repeatedly points at a large blob.
        let listed = files.iter().try_fold(0usize, |sum, file| {
            let size = usize::try_from(file["bytes"].as_u64()?).ok()?;
            sum.checked_add(size)
        })?;
        attachment_total = attachment_total.checked_add(listed)?;
        if attachment_total > MAX_TOTAL_BYTES {
            return None;
        }
        let (attachments, complete) = read_attachments(path, Some(files));
        if !complete {
            return None;
        }
        let state = serde_json::from_value(row["state"].clone()).ok()?;
        entries.push(Entry {
            request_id: row["request_id"].as_str()?.into(),
            queued_at: row["queued_at"].as_f64().and_then(enqueue_time),
            target: serde_json::from_value(row["target"].clone()).ok()?,
            catalog_key: row["catalog_key"].as_str()?.into(),
            draft_key: row["draft_key"].as_str()?.into(),
            text: row["text"].as_str()?.into(),
            attachments,
            // The write before dispatch is durable. A crash can occur on
            // either side of the native call, so the reserved head stops here.
            state: if state == State::Reserved {
                State::Unknown
            } else {
                state
            },
        });
    }
    valid(&entries).then_some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::{StoredDraft, load_once, save_editable};

    const KEY: &str = "\ncodex/example";
    const ID: &str = "a0000000-0000-4000-8000-000000000001";

    pub(super) struct Scratch(pub(super) PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    pub(super) fn scratch() -> Scratch {
        Scratch(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()))
    }

    #[test]
    fn only_a_committed_queue_claim_consumes_the_exact_editor_snapshot() {
        let root = scratch();
        let draft = StoredDraft {
            draft: crate::attachments::Draft {
                text: "same".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let entry = Entry {
            request_id: ID.into(),
            queued_at: Some(1_793_000_000.0),
            target: Target {
                computer: String::new(),
                name: "codex/example".into(),
                run_id: "run".into(),
                conversation_id: "conversation".into(),
            },
            catalog_key: "run:run".into(),
            draft_key: KEY.into(),
            text: draft.draft.text.clone(),
            attachments: Vec::new(),
            state: State::Waiting,
        };
        save_editable(&root.0, KEY, &draft, Some(ID)).unwrap();
        let (loaded, valid) = load_once(&root.0, KEY);
        assert!(valid);
        assert_eq!(loaded.draft.text, "same");
        assert!(loaded.outbox.is_empty());

        save(&root.0, KEY, &[entry], Some(ID)).unwrap();
        let (loaded, valid) = load_once(&root.0, KEY);
        assert!(valid);
        assert_eq!(loaded.outbox[0].request_id, ID);
        assert!(loaded.draft.text.is_empty());

        // A later same-text edit is independent, as is an old writer's
        // save, which does not know about the claim.
        save_editable(&root.0, KEY, &draft, None).unwrap();
        let (loaded, valid) = load_once(&root.0, KEY);
        assert!(valid);
        assert_eq!(loaded.draft.text, "same");
        assert_eq!(loaded.outbox[0].request_id, ID);
    }
}
