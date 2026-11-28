//! Canonical GUI lifetime exclusion. This namespace never consults HOME,
//! TMPDIR, XDG paths, hgs configuration or the desktop-service GUI flag.
//! Keep the inode after exit: unlinking a flock file can split ownership.
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
};

pub fn directory() -> PathBuf {
    // /tmp is the OS namespace, deliberately not std::env::temp_dir(). On
    // macOS /tmp and /private/tmp name the same system-owned directory.
    PathBuf::from(format!("/tmp/com.hgdev.hgs-tray-{}", unsafe {
        libc::geteuid()
    }))
}

/// Ordinary starts always use the canonical directory. On platforms that
/// allow private measurement roots, `acquire` applies the same ownership and
/// mode checks there. A canonical macOS GUI always needs the per-UID guard.
pub fn directory_for(
    canonical: &Path,
    measurement: bool,
    root: Option<&std::ffi::OsStr>,
    allow_private_root: bool,
) -> io::Result<PathBuf> {
    match root.filter(|root| measurement && !root.is_empty()) {
        Some(_) if !allow_private_root => Err(io::Error::other(
            "Remove ZERUS_CLIENT_GUARD_ROOT to start the canonical macOS Zerus GUI",
        )),
        Some(root) => {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return Err(io::Error::other(
                    "ZERUS_CLIENT_GUARD_ROOT must be an absolute path",
                ));
            }
            Ok(root)
        }
        _ => Ok(canonical.to_path_buf()),
    }
}

pub struct Guard {
    file: File,
    pub socket: PathBuf,
    inode: Option<u64>,
}

pub fn acquire(directory: &Path) -> io::Result<Option<Guard>> {
    match DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let uid = unsafe { libc::geteuid() };
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::other("unsafe canonical GUI directory"));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(directory.join("gui.lock"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
    {
        return Err(io::Error::other("unsafe canonical GUI lock"));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(Guard {
            file,
            socket: directory.join("gui.sock"),
            inode: None,
        })),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

impl Guard {
    pub fn bind(&mut self) -> io::Result<UnixListener> {
        // Only a kernel-lock owner may replace the crashed owner's socket.
        match fs::symlink_metadata(&self.socket) {
            Ok(metadata)
                if metadata.file_type().is_socket()
                    && metadata.uid() == unsafe { libc::geteuid() } =>
            {
                fs::remove_file(&self.socket)?;
            }
            Ok(_) => return Err(io::Error::other("unsafe canonical GUI socket")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(&self.socket)?;
        self.inode = Some(fs::symlink_metadata(&self.socket)?.ino());
        fs::set_permissions(
            &self.socket,
            <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
        )?;
        Ok(listener)
    }

    fn remove_socket(&mut self) {
        if self.inode.is_some_and(|inode| {
            fs::symlink_metadata(&self.socket).is_ok_and(|metadata| metadata.ino() == inode)
        }) {
            let _ = fs::remove_file(&self.socket);
        }
        self.inode = None;
    }

    pub fn retain_until_exit(mut self) {
        self.remove_socket();
        // The final GUI exit hook closes listeners before the OS has ended
        // the process. Do not admit a replacement during that interval.
        std::mem::forget(self);
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.remove_socket();
        // A concurrent fork may briefly retain a close-on-exec duplicate.
        // Explicit release also makes normal shutdown independent of that
        // transient inherited description. Crashes still release in kernel.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn only_a_measurement_start_with_private_root_support_can_use_it() {
        let canonical = directory();
        let private = std::ffi::OsStr::new("/tmp/zguard-private");
        assert_eq!(
            directory_for(&canonical, true, Some(private), true).unwrap(),
            PathBuf::from(private)
        );
        assert!(directory_for(&canonical, true, Some(private), false).is_err());
        // Ordinary starts ignore the override, even on macOS.
        assert_eq!(
            directory_for(&canonical, false, Some(private), false).unwrap(),
            canonical
        );
        assert_eq!(
            directory_for(&canonical, true, None, false).unwrap(),
            canonical
        );
        assert_eq!(
            directory_for(&canonical, true, Some(std::ffi::OsStr::new("")), false).unwrap(),
            canonical
        );
        let relative = std::ffi::OsStr::new("relative/root");
        assert!(directory_for(&canonical, true, Some(relative), true).is_err());
    }
    #[test]
    fn canonical_lock_is_close_on_exec() {
        let root = PathBuf::from("/tmp").join(format!("zguard-{}", uuid::Uuid::new_v4().simple()));
        let guard = acquire(&root).unwrap().unwrap();
        let flags = unsafe { libc::fcntl(guard.file.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0);
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }
}
