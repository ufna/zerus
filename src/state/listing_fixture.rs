//! A fleet row as the listing makes it, for tests of the readers of fleet
//! rows (the Teams actor, zerus-63rru.51). It runs `watch_source::rows`' own
//! steps for one live pane of `record`: the terminal's listing of that pane is
//! the only input given by hand, and every field of the row is the producer's.
//! The state root is the process's own (`HGS_STATE_DIR`), so a caller runs it
//! in a child test process with a private root.
use super::*;

/// The live row of `record`, whose agent process this test process stands in
/// for: its `pid` and `process_start` are this process's own.
pub(crate) fn listed_live_row(mut record: Value) -> Result<Value> {
    let pid = std::process::id();
    record["pid"] = json!(pid);
    record["process_start"] = json!(storage::process_start(pid));
    let listing = backend::Listing {
        clients: Vec::new(),
        sessions: vec![backend::SessionRow {
            name: terminal_name(&record).to_owned(),
            id: "$1".into(),
            attached: 0,
            created: string(&record, "created").parse().unwrap_or(1),
            launch_id: string(&record, "launch_id").to_owned(),
        }],
    };
    let mut session = listed_rows(&listing).remove(0);
    label_row(&mut session, &record);
    session
        .as_object_mut()
        .ok_or("invalid session")?
        .extend(telemetry::unavailable().as_object().unwrap().clone());
    let marks = read_marks::load();
    let preferences = notifications::load();
    let keep_warm_settings = keep_warm::load();
    let shared = Shared {
        marks: &marks,
        preferences: preferences.as_ref(),
        keep_warm: keep_warm_settings.as_ref(),
        budget: read_marks::question_budget(),
    };
    let mut process_work = processes::WorkSample::default();
    live_row(&mut session, record, &shared, &mut process_work)?;
    let mut rows = vec![session];
    finish_rows(&mut rows, &marks, &BTreeMap::new());
    Ok(rows.remove(0))
}

/// The listed row of a DeepSeek binding `record` as its host's `list`
/// answer `list` shows it, through the same steps as `native_rows`.
pub(crate) fn listed_deepseek_row(record: &Value, list: &Value) -> Value {
    let mut rows = vec![dsh::listed_for_fixture(record, list)];
    finish_rows(&mut rows, &read_marks::load(), &BTreeMap::new());
    rows.remove(0)
}
