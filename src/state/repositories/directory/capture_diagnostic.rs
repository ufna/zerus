//! Owned synthetic diagnostic parts only. Consuming a Directory here never
//! yields replacement authority or a reconstructible Captured proof.
#![cfg(all(test, windows))]
use super::Directory;
use crate::platform::persist_diagnostic::{handle_info, HandleInfo, Object};
use serde::Serialize;
use std::{fs::File, io, time::Instant};
use windows_sys::Win32::{
    Foundation::GENERIC_READ,
    Storage::FileSystem::{
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_TRAVERSE, READ_CONTROL,
        SYNCHRONIZE,
    },
};

const HANDLES: usize = 256;
#[derive(Clone, Copy)]
pub(in crate::state) enum Acquisition {
    OpenedDirectory,
    CreatedDirectory,
    PrivateReader,
}
impl Acquisition {
    fn masks(self) -> (u32, u32, &'static str) {
        match self {
            Self::OpenedDirectory => (GENERIC_READ, FILE_SHARE_READ, "Directory::open"),
            Self::CreatedDirectory => (
                FILE_LIST_DIRECTORY
                    | FILE_TRAVERSE
                    | FILE_READ_ATTRIBUTES
                    | READ_CONTROL
                    | SYNCHRONIZE,
                FILE_SHARE_READ,
                "Directory::create",
            ),
            // std OpenOptions' default R/W/DELETE sharing; acquisition is
            // read_private on the original fact/writer, never a new open here.
            Self::PrivateReader => (GENERIC_READ, 7, "Directory::read_private"),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub(in crate::state) struct Inventory {
    pub origin: String,
    pub ownership: &'static str,
    pub acquisition: &'static str,
    pub requested_access: u32,
    pub requested_share: u32,
    pub duplicated_original: bool,
    pub native: HandleInfo,
}
pub(in crate::state) struct Held {
    file: File,
    pub inventory: Inventory,
}
impl Held {
    pub fn new(file: File, origin: String, acquisition: Acquisition) -> io::Result<Self> {
        if origin.len() > 128 {
            return Err(io::Error::other("diagnostic origin bound"));
        }
        let (requested_access, requested_share, acquisition) = acquisition.masks();
        let native = handle_info(&file)?;
        Ok(Self {
            file,
            inventory: Inventory {
                origin,
                ownership: "synthetic-test-owned",
                acquisition,
                requested_access,
                requested_share,
                duplicated_original: false,
                native,
            },
        })
    }
    pub fn verify(&self) -> io::Result<()> {
        if handle_info(&self.file)? != self.inventory.native {
            return Err(io::Error::other(
                "retained diagnostic object/access/mode changed",
            ));
        }
        Ok(())
    }
    /// Extra directory-handle alias only. Never duplicates a persist source,
    /// fact, writer or locked open-file description.
    pub fn duplicate_root_alias(&self, root: Object) -> io::Result<Self> {
        self.verify()?;
        if self.inventory.native.object != root || !self.inventory.native.directory {
            return Err(io::Error::other(
                "only the synthetic R directory may be aliased",
            ));
        }
        let file = self.file.try_clone()?;
        let mut inventory = self.inventory.clone();
        inventory.origin = "extra-duplicate-R".into();
        inventory.duplicated_original = true;
        inventory.native = handle_info(&file)?;
        Ok(Self { file, inventory })
    }
}
impl Directory {
    /// Moves ALL original handles, including implicit R ancestor aliases.
    /// No reopen/duplicate/drop or production field/shape change occurs here.
    pub(in crate::state) fn into_capture_diagnostic_parts(
        self,
        origin: &str,
        acquisition: Acquisition,
    ) -> io::Result<Vec<Held>> {
        // Captured was verified before decomposition. Inventory never walks
        // names or opens a replacement handle; privacy uses this original.
        if !self.private().map_err(io::Error::other)? {
            return Err(io::Error::other("diagnostic directory privacy changed"));
        }
        let Directory {
            path: _,
            file,
            _ancestors,
        } = self;
        if _ancestors.len() + 1 > HANDLES {
            return Err(io::Error::other("diagnostic directory inventory bound"));
        }
        let mut parts = vec![Held::new(file, format!("{origin}/primary"), acquisition)?];
        for (n, file) in _ancestors.into_iter().enumerate() {
            parts.push(Held::new(
                file,
                format!("{origin}/ancestor-{n}"),
                Acquisition::OpenedDirectory,
            )?);
        }
        Ok(parts)
    }
}

/// The complete known, disposable test-owned R cohort. All other original
/// objects remain owned here through the actual producer and read-back.
pub(in crate::state) struct Cohort {
    root: Object,
    roots: Vec<Held>,
    retained: Vec<Held>,
    inventory: Vec<Inventory>,
    expected_roots: usize,
    released: bool,
}
impl Cohort {
    pub fn new(root: Object, handles: Vec<Held>, expected_roots: usize) -> io::Result<Self> {
        if handles.len() > HANDLES || expected_roots == 0 {
            return Err(io::Error::other("diagnostic cohort bound"));
        }
        let inventory = handles.iter().map(|h| h.inventory.clone()).collect();
        let (roots, retained): (Vec<_>, Vec<_>) = handles
            .into_iter()
            .partition(|h| h.inventory.native.object == root);
        let this = Self {
            root,
            roots,
            retained,
            inventory,
            expected_roots,
            released: false,
        };
        this.verify_inventory()?;
        Ok(this)
    }
    pub fn inventory(&self) -> &[Inventory] {
        &self.inventory
    }
    pub fn verify_inventory(&self) -> io::Result<()> {
        if !self.released && self.roots.len() != self.expected_roots {
            return Err(io::Error::other(
                "incomplete or unexpected same-native-R inventory",
            ));
        }
        if self.released && !self.roots.is_empty() {
            return Err(io::Error::other("partial R release"));
        }
        for held in self.roots.iter().chain(&self.retained) {
            held.verify()?;
        }
        if self
            .roots
            .iter()
            .any(|h| !h.inventory.native.directory || h.inventory.native.object != self.root)
            || self
                .retained
                .iter()
                .any(|h| h.inventory.native.object == self.root)
        {
            return Err(io::Error::other("same-native-R partition changed"));
        }
        Ok(())
    }
    pub fn release_all_r(&mut self, until: Instant) -> io::Result<()> {
        if self.released {
            return Err(io::Error::other("diagnostic R cohort already released"));
        }
        self.verify_inventory()?;
        // Inventory work is charged to the original producer deadline too.
        // Do not discard an R object after that budget has expired.
        if Instant::now() >= until {
            return Err(io::Error::other(
                "original deadline exhausted before R release",
            ));
        }
        self.roots.clear(); // ONLY these synthetic owned R handles.
        self.released = true;
        self.verify_inventory()
    }
}
