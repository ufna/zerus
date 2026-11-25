//! A display cut is certified outside the replaceable SQLite cache.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read;

const DIRECTORY: &str = "listing-coverage-v1";
const MAX_BYTES: u64 = 1024;

pub(super) fn directory(root: &Directory, create: bool) -> anyhow::Result<Directory> {
    if create {
        checked(root.ensure_child(DIRECTORY))
    } else {
        checked(root.child(DIRECTORY))
    }
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Cut {
    epoch: String,
    generation: i64,
    high_water: i64,
    database: (u64, u64),
}

#[derive(Deserialize, Serialize)]
struct Witness {
    #[serde(flatten)]
    cut: Cut,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical: Option<CanonicalWrites>,
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct CanonicalWrites {
    database: (u64, u64),
    epoch: String,
    writes: i64,
    pub pending: bool,
}

impl Store {
    fn witness_name(&self) -> &'static str {
        match self.index {
            store::Index::Shared => "cut.json",
            store::Index::Certified => "certified-cut.json",
        }
    }

    /// Older binaries reserve canonical writes in the shared index before
    /// touching a binding. A certified companion must see those reservations
    /// even when its own change feed has yet to observe the binding.
    pub fn canonical_writes(&self) -> anyhow::Result<Option<CanonicalWrites>> {
        if self.index == store::Index::Shared {
            return Ok(None);
        }
        let read = || -> anyhow::Result<_> {
            let shared = Self::read(&self.root.path)?;
            let database = shared.object()?;
            let (epoch, writes, pending) = shared.db.query_row(
                "SELECT substr(epoch,1,37),writes,EXISTS(SELECT 1 FROM intents) FROM meta WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            shared.verify()?;
            Ok(Some(CanonicalWrites { database, epoch, writes, pending }))
        };
        // A fault in the shared database cannot authorize recovering or
        // replacing the companion database.
        read().map_err(|error| anyhow::anyhow!("Shared listing write fence is unavailable: {error}"))
    }

    pub fn active_cut(&self) -> anyhow::Result<Option<Cut>> {
        let database = self.object()?;
        Ok(self
            .db
            .query_row(
                "SELECT substr(m.epoch,1,37),m.generation,g.high_water FROM meta m
             JOIN generations g ON g.generation=m.generation AND g.complete=1 WHERE m.id=1",
                [],
                |row| {
                    Ok(Cut {
                        epoch: row.get(0)?,
                        generation: row.get(1)?,
                        high_water: row.get(2)?,
                        database,
                    })
                },
            )
            .optional()?)
    }

    fn witness(&self) -> Option<Witness> {
        let read = || -> anyhow::Result<Witness> {
            let directory = checked(self.root.child(DIRECTORY))?;
            let file = checked(directory.read_private(self.witness_name()))?;
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_BYTES,
                "Display coverage witness exceeds its bound"
            );
            let witnessed = serde_json::from_slice(&bytes)?;
            checked(directory.verify())?;
            self.verify()?;
            Ok(witnessed)
        };
        read().ok()
    }

    pub fn matches_certificate(&self, cut: &Cut) -> bool {
        self.witness().is_some_and(|witness| {
            witness.cut == *cut
                && witness.canonical.is_some() == (self.index == store::Index::Certified)
                && witness.canonical.as_ref().is_none_or(|state| !state.pending)
        })
    }

    pub fn covers(&self, cut: &Cut) -> bool {
        self.witness().is_some_and(|witness| {
            witness.cut == *cut
                && witness.canonical.as_ref().is_none_or(|state| !state.pending)
                && self.canonical_writes().is_ok_and(|state| state == witness.canonical)
        })
    }

    pub fn coverage_directory(&self) -> anyhow::Result<Directory> {
        match std::fs::symlink_metadata(self.root.path.join(DIRECTORY)) {
            Ok(_) => directory(&self.root, false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                directory(&self.root, true)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn certify(
        &self,
        directory: &Directory,
        canonical: Option<CanonicalWrites>,
    ) -> anyhow::Result<()> {
        let cut = self
            .active_cut()?
            .ok_or_else(|| anyhow::anyhow!("Missing display coverage cut"))?;
        let witness = Witness { cut, canonical };
        checked(directory.replace_file(self.witness_name(), &serde_json::to_vec(&witness)?))
    }
}
