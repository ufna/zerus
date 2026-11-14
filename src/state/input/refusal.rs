//! Public-send negative journal publication, reachable only from a live witness
//! under the original input lock. It cannot type, overwrite or replay anything.
use super::*;
use crate::state::repositories::directory::Directory;
use zerus_client_core::desktop::deferred_send::{refusal, Refusal};

pub(super) fn remember(witness: &NotOffered) -> Result<()> {
    let value = Refusal::new(
        &witness.request_id,
        &witness.name,
        &witness.run_id,
        &witness.conversation_id,
        &witness.digest,
        &witness.reason,
    )
    .and_then(|value| value.cli_record())
    .ok_or("Invalid pre-input refusal binding")?;
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() > refusal::BYTES {
        return Err("Pre-input refusal exceeds its bound".into());
    }
    let root = witness
        .receipts
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid pre-input refusal store")?;
    let root = Directory::open(root)?;
    let receipts = root.ensure_child("input_receipts")?;
    let filename = format!("{}.json", witness.request_id);
    if receipts.path.join(&filename) != witness.receipts || !receipts.private()? {
        return Err("Pre-input refusal store changed".into());
    }
    // The maintained publisher syncs private staging bytes, publishes without
    // replacement, and syncs the original directory. Any failure stays Unknown.
    receipts.write_new(&filename, &bytes)?;
    root.verify()?;
    receipts.verify()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn witness(root: &Path, reason: &str) -> NotOffered {
        let request_id = uuid::Uuid::new_v4().to_string();
        NotOffered {
            receipts: root
                .join("input_receipts")
                .join(format!("{request_id}.json")),
            request_id,
            name: "claude/private-send".into(),
            run_id: "run-1".into(),
            conversation_id: "conversation-1".into(),
            digest: "a".repeat(64),
            reason: reason.into(),
            waiting: true,
            dialog: None,
        }
    }
    #[test]
    fn only_the_public_untyped_witness_publishes_and_a_duplicate_cannot_replace_it() {
        let private = tempfile::tempdir().unwrap();
        let root = private.path().canonicalize().unwrap();
        let w = witness(&root, "original draft refusal");
        let path = w.receipts.clone();
        let digest = w.digest.clone();
        let name = w.name.clone();
        assert!(matches!(
            remember_before_input(false, w, true),
            Failure::NotOffered(_)
        ));
        let before = fs::read(&path).unwrap();
        let record: Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(record["status"], refusal::STATUS);
        assert_eq!(record["reason"], "original draft refusal");
        assert_eq!(
            record["request_id"],
            path.file_stem().unwrap().to_str().unwrap()
        );
        assert!(before.len() <= refusal::BYTES);
        assert!(
            matches!(existing(&path, &digest, &name), Existing::Unknown(_)),
            "negative UUIDs cannot replay"
        );
        let mut duplicate = witness(&root, "replacement reason");
        duplicate.request_id = record["request_id"].as_str().unwrap().into();
        duplicate.receipts = path.clone();
        assert!(matches!(
            remember_before_input(false, duplicate, true),
            Failure::Unknown(_)
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        for (typed, retain) in [(true, true), (false, false)] {
            let w = witness(&root, "never a public pre-input negative");
            let path = w.receipts.clone();
            let failure = remember_before_input(typed, w, retain);
            assert_eq!(matches!(failure, Failure::Unknown(_)), typed);
            assert!(
                !path.exists(),
                "possible input and internal callers never create a negative"
            );
        }
    }
    #[test]
    fn publication_failure_keeps_unknown_and_retains_the_original_object() {
        let private = tempfile::tempdir().unwrap();
        let root = private.path().canonicalize().unwrap();
        let w = witness(&root, "draft refusal");
        fs::write(root.join("input_receipts"), b"foreign object").unwrap();
        assert!(matches!(
            remember_before_input(false, w, true),
            Failure::Unknown(_)
        ));
        assert_eq!(
            fs::read(root.join("input_receipts")).unwrap(),
            b"foreign object"
        );
    }
}
