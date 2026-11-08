//! Publish through the original file handle without reopening a pinned parent
//! for write access. The parent's denied write/delete sharing stays in force.
use super::{Directory, File, OpenOptions, Result};
use std::{io::Write, os::windows::fs::OpenOptionsExt, os::windows::io::AsRawHandle};
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FileRenameInformation, NtSetInformationFile, FILE_RENAME_INFORMATION,
    },
    Win32::{
        Foundation::{RtlNtStatusToDosError, GENERIC_READ, GENERIC_WRITE},
        Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ,
            SYNCHRONIZE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

pub(super) fn publish(
    directory: &Directory,
    name: &str,
    bytes: &[u8],
    replace: bool,
) -> Result<File> {
    // A bounded single native component; never accept a path, stream, device
    // prefix or a Win32 trailing-dot/space alias for the published name.
    if name.is_empty()
        || name.len() > 1020
        || name.encode_utf16().count() > 255
        || name.contains(['/', '\\', ':', '\0'])
        || name.ends_with(['.', ' '])
    {
        return Err("Invalid file name".into());
    }
    directory.verify()?;
    let staging = directory
        .path
        .join(format!(".hgs-new-{}", uuid::Uuid::new_v4()));
    // Errors retain the owned staging/published object for review. Never
    // unlink a guessed name after a potentially completed publication.
    // All flags are combined in ONE custom_flags call. A second setter would
    // replace the no-reparse/write-through flags instead of adding to them.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH)
        .share_mode(FILE_SHARE_READ)
        .open(&staging)
        .map_err(|e| e.to_string())?;
    if !crate::platform::verify_private_file(&file).map_err(|e| e.to_string())? {
        return Err("The new file must be private, regular and singly linked".into());
    }
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    directory.verify()?;
    // SAFETY: this SAME CREATE_NEW handle is synchronous, has DELETE access,
    // and denies other writers/deleters throughout the native rename.
    unsafe { rename(&file, name, replace) }.map_err(|e| e.to_string())?;
    // Flush the renamed original, not a path-selected replacement. On NTFS
    // write-through also requests metadata flushing. This is not a directory
    // fsync claim for arbitrary filesystems/hardware; see the rollback guide.
    file.sync_all().map_err(|e| e.to_string())?;
    directory.verify()?;
    let named = directory.read_private(name)?;
    if crate::platform::FileIdentity::of(&file).map_err(|e| e.to_string())?
        != crate::platform::FileIdentity::of(&named).map_err(|e| e.to_string())?
    {
        return Err("The published original file changed".into());
    }
    // Return the original handle, never adopt the path-selected check above.
    Ok(file)
}

/// # Safety
/// `file` is the original synchronous CREATE_NEW handle with DELETE access.
/// `name` is the bounded simple component validated by publish().
unsafe fn rename(file: &File, name: &str, replace: bool) -> std::io::Result<()> {
    let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>() + name.len() * 2;
    let mut storage = vec![0u64; bytes.div_ceil(8)];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: aligned initialized storage contains the complete header/name.
    // NULL RootDirectory and a simple name select the original file's SAME
    // parent, never cwd or a pathname-opened destination directory.
    // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
    let result = unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = ((name.len() - 1) * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            info.cast(),
            bytes as u32,
            FileRenameInformation,
        )
    };
    if result < 0 {
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "windows_publication/tests.rs"]
mod tests;
