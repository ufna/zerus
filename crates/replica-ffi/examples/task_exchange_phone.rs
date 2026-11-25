//! The mobile apps' native task replica for the relay's hosted HTTP contract.
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use zerus_replica_ffi::{TaskExchange, TaskPosition, TaskStage, TaskStore, TaskTarget};

fn exchange_for(store: Arc<TaskStore>, features: &[String]) -> Arc<TaskExchange> {
    if features
        .iter()
        .any(|feature| feature == "task_media_upload_v1")
    {
        store.exchange_uploading(Vec::new())
    } else {
        store.exchange()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [root, project, name, features, add @ ..] = args.as_slice() else {
        return Err(
            io::Error::other("expected ROOT PROJECT NAME FEATURES_JSON [TASK TEXT]").into(),
        );
    };
    let features: Vec<String> = serde_json::from_str(features)?;
    let store = TaskStore::new(root.clone(), project.clone(), name.clone());
    match add {
        [] => {}
        [text] => {
            store.add(
                text.clone(),
                TaskStage::Todo,
                TaskTarget::AnySession,
                TaskPosition::End,
            )?;
        }
        _ => return Err(io::Error::other("expected at most one task to file").into()),
    }

    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let exchange = exchange_for(store.clone(), &features);
    let mut changed = 0;
    while let Some(request) = exchange.next_request()? {
        let request: Value = serde_json::from_str(&request)?;
        writeln!(output, "{}", json!({"request": request}))?;
        output.flush()?;
        let mut reply = String::new();
        input.read_line(&mut reply)?;
        let progress = exchange.accept(reply)?;
        if !progress.refused_here.is_empty()
            || !progress.refused_there.is_empty()
            || !progress.unsent.is_empty()
        {
            return Err(
                io::Error::other(format!("task exchange refused an update: {progress:?}")).into(),
            );
        }
        changed += progress.changed;
    }

    let listing = store.list()?;
    if !listing.damaged.is_empty() {
        return Err(io::Error::other(format!("damaged phone tasks: {:?}", listing.damaged)).into());
    }
    let tasks: Vec<_> = listing.tasks.into_iter().map(|task| {
        json!({"uuid": task.uuid, "text": task.text, "writable": task.writable, "schema": task.schema})
    }).collect();
    writeln!(
        output,
        "{}",
        json!({"digest": store.live_digest()?, "changed": changed, "tasks": tasks})
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use zerus_replica_ffi::TaskMediaInput;

    #[test]
    fn the_advertised_feature_enables_large_attachment_uploads() {
        let phone_root = tempfile::tempdir().unwrap();
        let computer_root = tempfile::tempdir().unwrap();
        let store = |root: &tempfile::TempDir| {
            TaskStore::new(
                root.path().display().to_string(),
                "project-1".into(),
                "orchard".into(),
            )
        };
        let phone = store(&phone_root);
        let computer = store(&computer_root);
        let data: Vec<u8> = (0u64..25600)
            .flat_map(|i| Sha256::digest(i.to_le_bytes()))
            .collect();
        let task = phone
            .add_with_media(
                "Review the photo".into(),
                TaskStage::Todo,
                TaskTarget::AnySession,
                TaskPosition::End,
                vec![TaskMediaInput {
                    name: "fixture.png".into(),
                    mime: "image/png".into(),
                    data: data.clone(),
                    reference: "image-1".into(),
                }],
            )
            .unwrap();

        let plain = exchange_for(phone.clone(), &["task_session_identity_v2".into()]);
        let mut unsent = false;
        while let Some(request) = plain.next_request().unwrap() {
            let progress = plain
                .accept(computer.answer_exchange(request).unwrap())
                .unwrap();
            unsent |= progress.unsent.contains(&task.uuid);
        }
        assert!(unsent, "an older computer must get the plain exchange");
        assert!(computer.list().unwrap().tasks.is_empty());

        let uploading = exchange_for(phone, &["task_media_upload_v1".into()]);
        while let Some(request) = uploading.next_request().unwrap() {
            let progress = uploading
                .accept(computer.answer_exchange(request).unwrap())
                .unwrap();
            assert!(
                progress.unsent.is_empty(),
                "the advertised upload must send the task"
            );
            assert!(progress.refused_here.is_empty());
            assert!(progress.refused_there.is_empty());
        }
        let media = computer.media(task.uuid.clone(), None).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(
            computer
                .read_media(task.uuid, None, 0, media[0].token.clone())
                .unwrap()
                .data,
            data
        );
    }
}
