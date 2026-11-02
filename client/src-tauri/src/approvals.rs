//! Settings > Automatic approvals: paged journal reads and the separate,
//! default-off native worker. Only that worker can reserve and deliver a
//! verified one-time permission; the webview never supplies an answer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::State;
use zerus_client_core::approvals::{self, Operation, Page};
use zerus_client_core::default_hgs;

mod automatic;

/// Whether the local `hgs` answered the probe, and whether this is a replay.
pub struct Approvals {
    supported: Arc<AtomicBool>,
    blocked: bool,
    _worker: Option<automatic::Worker>,
}

pub fn start(
    pump: crate::pump::Pump,
    actions: crate::actions::Actions,
    settings: zerus_client_core::settings::Settings,
) -> Approvals {
    let supported = Arc::new(AtomicBool::new(false));
    Approvals {
        _worker: (!crate::desktop_service::mode())
            .then(|| automatic::Worker::start(pump, actions, settings, supported.clone())),
        supported,
        blocked: crate::actions::blocked(),
    }
}

/// Runs one journal operation off the main thread.
async fn read(
    approvals: &Approvals,
    operation: Operation,
    data: serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    if crate::desktop_service::mode() {
        return Err(
            "The approval journal is unavailable with this background service connection.".into(),
        );
    }
    if approvals.blocked {
        return Err("The automatic approval journal is not read in a replay.".into());
    }
    let supported = Arc::clone(&approvals.supported);
    tauri::async_runtime::spawn_blocking(move || {
        let mut known = supported.load(Ordering::Acquire);
        let reply = approvals::call(&default_hgs(), operation, &data, &mut known);
        if known {
            supported.store(true, Ordering::Release);
        }
        reply
    })
    .await
    .map_err(|error| error.to_string())?
}

/// One page of the journal: exact computer and session filters, and a
/// newer (`after`) or older (`before`) cursor, 0 for none.
#[tauri::command]
pub async fn approvals_list(
    approvals: State<'_, Approvals>,
    host: String,
    session: String,
    before: i64,
    after: i64,
) -> Result<Page, String> {
    let reply = read(
        &approvals,
        Operation::List,
        approvals::list_request(&host, &session, before, after),
    )
    .await?;
    approvals::page(&reply)
}

/// One entry's question and outcome, as Qt words them.
#[tauri::command]
pub async fn approvals_detail(approvals: State<'_, Approvals>, id: i64) -> Result<String, String> {
    let reply = read(
        &approvals,
        Operation::Detail,
        serde_json::json!({ "id": id }),
    )
    .await?;
    Ok(approvals::detail(&reply))
}
