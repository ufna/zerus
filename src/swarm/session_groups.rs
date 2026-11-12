//! New group registers stay outside catalog.json so older local CLIs can
//! still read and update projects. Both files use the catalog's existing lock.
use super::{
    catalog::{Catalog, Key},
    Unread,
};
use anyhow::{ensure, Context, Result};
use std::path::Path;

pub const FILE: &str = "session-groups.json";
pub const MAX_BYTES: u64 = 128 * 1024 * 1024;

fn is_group(key: &Key) -> bool {
    matches!(
        key,
        Key::ProjectGroup { .. } | Key::SessionGroup { .. } | Key::SessionGroupOrder { .. }
    )
}

pub fn merge(catalog: &mut Catalog, bytes: &Result<Vec<u8>, Unread>) -> Result<()> {
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(Unread::Missing) => return Ok(()),
        Err(_) => anyhow::bail!("Session group catalog is unavailable; the original is retained"),
    };
    let groups: Catalog = serde_json::from_slice(bytes).context("read session group catalog")?;
    groups.validate()?;
    ensure!(
        groups.operations.values().all(|op| is_group(&op.key)),
        "invalid session group catalog keys"
    );
    catalog.merge(&groups.swarm_id, groups.operations.into_values().collect())?;
    Ok(())
}

pub fn read(directory: &Path) -> Result<Vec<u8>, Unread> {
    super::read_regular(&directory.join(FILE), MAX_BYTES)
}

pub fn bytes(catalog: &Catalog) -> Result<Option<Vec<u8>>> {
    let mut groups = catalog.clone();
    groups.operations.retain(|_, op| is_group(&op.key));
    if groups.operations.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_vec(&groups)?))
}

pub fn legacy_bytes(store: &super::store::Store) -> Result<Vec<u8>> {
    let mut legacy = store.clone();
    legacy.catalog.operations.retain(|_, op| !is_group(&op.key));
    Ok(serde_json::to_vec(&legacy)?)
}
