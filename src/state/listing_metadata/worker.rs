//! One certificate-aware worker per root. An older resident worker may own
//! the original lease and publish to the compatibility index independently.
use super::*;
use fs2::FileExt;
use std::{
    process::{Command, Stdio},
    thread,
};

const LEGACY_LEASE: &str = "worker.lock";
const COVERAGE_LEASE: &str = "certified-worker.lock";
const ATTEMPT: &str = "certified-attempt.json";

fn directory(root: &Path) -> anyhow::Result<Directory> {
    let parent = checked(Directory::open(&crate::platform::canonical(root)?))?;
    let dir = checked(parent.child("listing-metadata"))?;
    checked(parent.verify())?;
    Ok(dir)
}
fn take(directory: &Directory, name: &str) -> anyhow::Result<Option<storage::Guard>> {
    let file = checked(directory.open_private_lock(name))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(storage::Guard::held(file))),
        Err(e) if crate::platform::lock_contended(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn running(root: &Path) -> bool {
    let inspect = || -> anyhow::Result<bool> {
        let root = checked(Directory::open(&crate::platform::canonical(root)?))?;
        let dir = coverage::directory(&root, false)?;
        let Some(file) = checked(dir.read_private_optional(COVERAGE_LEASE))? else {
            return Ok(false);
        };
        match file.try_lock_exclusive() {
            Ok(()) => Ok(false),
            Err(e) if crate::platform::lock_contended(&e) => Ok(true),
            Err(e) => Err(e.into()),
        }
    };
    inspect().unwrap_or(false)
}
pub(in crate::state) fn request(root: &Path) {
    if !changes::qualified() || !enabled(root) {
        return;
    }
    let launch = || -> anyhow::Result<()> {
        let root = checked(Directory::open(&crate::platform::canonical(root)?))?;
        let dir = coverage::directory(&root, true)?;
        let Some(_launch) = take(&dir, "launch.lock")? else {
            return Ok(());
        };
        let attempt = checked(dir.read_private_optional(ATTEMPT))?;
        if let Some(file) = attempt {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(4097).read_to_end(&mut bytes)?;
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                let age = now() - value["at"].as_f64().unwrap_or(0.0);
                if (0.0..2.0).contains(&age) {
                    return Ok(());
                }
            }
        }
        // A recent request has already started a child. Probing the lease
        // before that child takes it can make the child lose admission.
        let Some(probe) = take(&dir, COVERAGE_LEASE)? else {
            return Ok(());
        };
        checked(dir.replace_file(ATTEMPT, format!("{{\"at\":{}}}\n", now()).as_bytes()))?;
        checked(root.verify())?;
        let mut command = Command::new(std::env::current_exe()?);
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("HGS_"))
        {
            command.env_remove(key);
        }
        command
            .args(["__state", "listing-metadata-worker"])
            .env("HGS_STATE_DIR", &root.path)
            .current_dir(&root.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        drop(probe);
        let mut child = crate::platform::spawn_detached(&mut command)?;
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    };
    let _ = launch();
}

/// Keep the common mutation fence usable when no older publisher is present.
/// Legacy discovery is only needed to settle abandoned advisory intents.
fn maintain_shared(root: &Path, work: &mut Option<repair::Background>) -> anyhow::Result<()> {
    let read = Store::read(root).and_then(|store| {
        Ok(store.db.query_row("SELECT EXISTS(SELECT 1 FROM intents)", [], |row| {
            row.get::<_, bool>(0)
        })?)
    });
    if matches!(read, Ok(false)) {
        *work = None;
        return Ok(());
    }
    let Some(_legacy) = take(&directory(root)?, LEGACY_LEASE)? else {
        *work = None;
        return Ok(());
    };
    match read {
        Err(error) if error.is::<PublicationPending>() => {
            if let Err(error) = Store::open(root) {
                Store::recover(root, store::Index::Shared, &error)?;
            }
        }
        Ok(true) | Err(_) => {
            repair::background_at(root, work)?;
        }
        Ok(false) => (),
    }
    Ok(())
}
fn status(root: &Path, index: store::Index, state: &str, reason: &str) {
    let write = || -> anyhow::Result<()> {
        let dir = index.directory(directory(root)?)?;
        let value = json!({"version":1,"state":state,"reason":reason.chars().take(512).collect::<String>(),"at":now()});
        checked(dir.replace_file("worker-status.json", format!("{value}\n").as_bytes()))
    };
    let _ = write();
}

pub(in crate::state) fn entry() -> Result<i32> {
    let run = || -> anyhow::Result<i32> {
        anyhow::ensure!(
            changes::qualified(),
            "Indexed session listing is not qualified on this platform"
        );
        let selected = root();
        let root = checked(Directory::open(&crate::platform::canonical(&selected)?))?;
        let coverage = coverage::directory(&root, true)?;
        let Some(_lease) = take(&coverage, COVERAGE_LEASE)? else {
            return Ok(0);
        };
        let executable = std::env::current_exe()?;
        let binary = crate::platform::FileIdentity::at(&executable)
            .ok()
            .map(|v| v.0);
        if binary.is_none() {
            return Ok(0);
        }
        let index = store::Index::Certified;
        let mut work = None;
        let mut compatibility = None;
        let mut reconciliation = reconcile::Reconcile::default();
        let mut last_state = String::new();
        loop {
            checked(root.verify())?;
            checked(coverage.verify())?;
            if crate::platform::canonical(&selected).ok().as_ref() != Some(&root.path)
                || !enabled(&root.path)
                || crate::platform::FileIdentity::at(&executable)
                    .ok()
                    .map(|v| v.0)
                    != binary
            {
                return Ok(0);
            }
            let _ = maintain_shared(&root.path, &mut compatibility);
            match repair::background_for(&root.path, index, &mut work) {
                Ok(stats) => {
                    anyhow::ensure!(
                        stats.entries <= 256
                            && stats.rows <= 128
                            && stats.bytes <= 16 << 20
                            && stats.seed_bytes <= 512 << 10,
                        "Metadata worker step exceeded its bound"
                    );
                    let state = if stats.complete {
                        "ready"
                    } else if stats.done {
                        "unavailable"
                    } else {
                        "building"
                    };
                    if last_state != state {
                        status(&root.path, index, state, "");
                        last_state = state.into();
                    }
                    if stats.complete {
                        if let Ok(Some(_legacy)) =
                            directory(&root.path).and_then(|dir| take(&dir, LEGACY_LEASE))
                        {
                            // This optional work owns no display transaction or gate.
                            let _ = reconciliation.step(&root.path);
                        }
                    }
                    // A resident stream closes its gap before each new cut.
                    // Idle polling is fixed-size native metadata, never a census.
                    thread::sleep(Duration::from_millis(if stats.complete {
                        200
                    } else if stats.done {
                        2000
                    } else {
                        1
                    }));
                }
                Err(error) => {
                    let busy = sqlite_busy(&error);
                    if last_state != "unavailable" || !busy {
                        gap_at(&root.path, index);
                        status(&root.path, index, "unavailable", &public_index_error(&error));
                    }
                    last_state = "unavailable".into();
                    work = None;
                    thread::sleep(Duration::from_millis(if busy { 20 } else { 2000 }));
                }
            }
        }
    };
    run().map_err(|e| e.to_string())
}
