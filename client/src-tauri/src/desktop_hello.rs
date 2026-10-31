//! Background setup for the canonical instance's read-only delegation hello.
//! The socket callback uses cached identity only; there is no probe-time child.
use std::sync::Arc;
use zerus_client_core::desktop::{self, Build, Responder};

pub fn prepare(identity: &crate::identity::Identity) -> (crate::instance::Reply, Arc<Responder>) {
    let info = crate::cli::build_info(&crate::cli::BUILD);
    let responder = Arc::new(Responder::new(
        identity.bundle_id.clone(),
        Build {
            version: info["versionName"].as_str().unwrap_or("").into(),
            source_commit: info["sourceCommit"].as_str().unwrap_or("").into(),
            source_dirty: info["sourceDirty"].as_bool(),
        },
        uuid::Uuid::new_v4().to_string(),
        true,
        std::env::var_os("ZERUS_CLIENT_REPLAY").is_some_and(|v| !v.is_empty()),
    ));
    let cached = Arc::clone(&responder);
    let reply: crate::instance::Reply = Arc::new(move |stream, line| {
        desktop::same_user(stream)
            .then(|| cached.reply(line))
            .flatten()
    });
    (reply, responder)
}

/// Only the process that claimed the canonical socket calls this, before any
/// writers or GUI actor exist. Replay and other bundle identities stay isolated.
pub fn hold(
    production: bool,
) -> Result<Option<desktop::startup::Bootstrap>, desktop::startup::Refused> {
    if !production || std::env::var_os("ZERUS_CLIENT_REPLAY").is_some_and(|v| !v.is_empty()) {
        return Ok(None);
    }
    let mut held = desktop::startup::Bootstrap::begin(
        &zerus_client_core::default_hgs(),
        &uuid::Uuid::new_v4().to_string(),
        &[],
    )?;
    held.wait_ready()?;
    Ok(Some(held))
}

/// Request release after setup registered its writers, then retain and actually
/// join the original helper worker off the GUI actor. No late/unknown result can
/// publish configuration or make this instance eligible for producer binding.
pub fn finish(mut held: desktop::startup::Bootstrap, responder: Arc<Responder>) {
    if held.release().is_err() {
        eprintln!(
            "zerus-client: desktop startup release failed; producer identity remains unavailable"
        );
        return;
    }
    tauri::async_runtime::spawn_blocking(move || loop {
        match held.try_finish() {
            Ok(Some(configuration)) => {
                responder.configure_once(configuration);
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(_) => {
                eprintln!("zerus-client: desktop startup helper did not finish; producer identity remains unavailable");
                break;
            }
        }
    });
}
