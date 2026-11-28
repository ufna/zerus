use super::{queue_dir, read_bounded, MAX_MANIFEST};
use serde_json::Value;
use std::{fs, path::Path};

/// Directory position for cold-start recovery. Only the app's own two storage
/// layouts participate; the iterator does not retain keys or attachment bytes.
#[derive(Default)]
pub struct Scan {
    directory: Option<fs::ReadDir>,
    inline: bool,
}

impl Scan {
    pub(in crate::drafts) fn page(mut self, root: &Path) -> (Option<Self>, Option<String>) {
        // Yield to other draft writer jobs after a small batch of metadata
        // reads, or as soon as there is one ordinary queue to load.
        for _ in 0..16 {
            let dir = if self.inline {
                root.to_owned()
            } else {
                root.join("outbox")
            };
            if self.directory.is_none() && fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
                self.directory = fs::read_dir(&dir).ok();
            }
            let Some(entry) = self.directory.as_mut().and_then(Iterator::next) else {
                if self.inline {
                    return (None, None);
                }
                self.inline = true;
                self.directory = None;
                continue;
            };
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = entry.file_name();
            let Some(hash) = name.to_str() else {
                continue;
            };
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let path = entry.path().join(if self.inline {
                "draft.json"
            } else {
                "queue.json"
            });
            if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let Some(object) = read_bounded(&path, MAX_MANIFEST)
                .ok()
                .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
            else {
                continue;
            };
            let Some(key) = object["key"].as_str() else {
                continue;
            };
            let Some(head) = object[if self.inline { "outbox" } else { "entries" }]
                .as_array()
                .and_then(|rows| rows.first())
            else {
                continue;
            };
            if object["version"] != 1
                || crate::drafts::sha256_hex(key.as_bytes()) != hash
                || head["draft_key"] != key
                || !matches!(
                    head["state"].as_str(),
                    Some("waiting" | "reserved" | "unknown")
                )
            {
                continue;
            }
            if key.len() > 2048 || key.split('\n').count() != 2 {
                continue;
            }
            let owner = crate::drafts::owner(key);
            if owner.kind != crate::drafts::OwnerKind::Message
                || !owner.archive.is_empty()
                || head["target"]["computer"] != owner.machine
                || (owner.session_id.is_empty() && head["target"]["name"] != owner.session)
            {
                continue;
            }
            // A separate (even empty) manifest supersedes the legacy inline
            // copy. The regular loader revalidates the entire queue and blobs.
            if self.inline && queue_dir(root, key).join("queue.json").exists() {
                continue;
            }
            return (Some(self), Some(key.to_owned()));
        }
        (Some(self), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::folder;
    use crate::drafts::outbox::{self, Entry, State};
    use crate::Target;
    use std::collections::HashSet;

    #[test]
    fn discovery_pages_both_formats_and_skips_empties_copies_and_superseded_queues() {
        let scratch = super::super::tests::scratch();
        let root = &scratch.0;
        let mut expected = HashSet::new();
        for n in 0..40 {
            let name = format!("codex/project/s{n}");
            let key = format!("\n{name}");
            let entry = Entry {
                request_id: uuid::Uuid::new_v4().to_string(),
                queued_at: None,
                delivery_in_composer: false,
                catalog_key: name.clone(),
                draft_key: key.clone(),
                target: Target {
                    name,
                    computer: String::new(),
                    run_id: "r".into(),
                    conversation_id: "c".into(),
                },
                text: "original".into(),
                attachments: Vec::new(),
                state: State::Waiting,
            };
            match n {
                0 | 3 => {
                    let path = folder(root, &key);
                    fs::create_dir_all(&path).unwrap();
                    let rows = outbox::encode(&path, &[entry], &mut HashSet::new()).unwrap();
                    let value = serde_json::json!({"version":1,"key":key,"outbox":rows});
                    fs::write(path.join("draft.json"), value.to_string()).unwrap();
                    if n == 3 {
                        outbox::save(root, &key, &[], None).unwrap();
                        continue;
                    }
                }
                1 => {
                    let entries = [Entry {
                        draft_key: "other".into(),
                        ..entry
                    }];
                    outbox::save(root, &key, &entries, None).unwrap();
                    continue;
                }
                2 => {
                    outbox::save(root, &key, &[], None).unwrap();
                    continue;
                }
                _ => outbox::save(root, &key, &[entry], None).unwrap(),
            }
            expected.insert(key);
        }
        let mut cursor = Some(Scan::default());
        let mut found = HashSet::new();
        while let Some(scan) = cursor {
            let (next, key) = scan.page(root);
            if let Some(key) = key {
                assert!(found.insert(key));
            }
            cursor = next;
        }
        assert_eq!(found, expected);
    }
}
