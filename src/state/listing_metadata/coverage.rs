//! A display cut is certified outside the replaceable SQLite cache.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read;

const DIRECTORY: &str = "listing-coverage-v1";
const WITNESS: &str = "cut.json";
const MAX_BYTES: u64 = 1024;

#[derive(Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Cut {
    epoch: String,
    generation: i64,
    high_water: i64,
    database: (u64, u64),
}

impl Store {
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

    pub fn covers(&self, cut: &Cut) -> bool {
        let read = || -> anyhow::Result<bool> {
            let directory = checked(self.root.child(DIRECTORY))?;
            let file = checked(directory.read_private(WITNESS))?;
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_BYTES,
                "Display coverage witness exceeds its bound"
            );
            let witnessed: Cut = serde_json::from_slice(&bytes)?;
            checked(directory.verify())?;
            self.verify()?;
            Ok(witnessed == *cut)
        };
        read().unwrap_or(false)
    }

    pub fn coverage_directory(&self) -> anyhow::Result<Directory> {
        match std::fs::symlink_metadata(self.root.path.join(DIRECTORY)) {
            Ok(_) => checked(self.root.child(DIRECTORY)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                checked(self.root.ensure_child(DIRECTORY))
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn certify(&self, directory: &Directory) -> anyhow::Result<()> {
        let cut = self
            .active_cut()?
            .ok_or_else(|| anyhow::anyhow!("Missing display coverage cut"))?;
        checked(directory.replace_file(WITNESS, &serde_json::to_vec(&cut)?))
    }
}
