//! Stable APFS identity from descriptors already held by the receipt store.
use std::fs::File;

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct StableIdentity {
    volume: String,
    object: u64,
    created_seconds: u64,
    created_nanos: u32,
}

pub(super) fn observe(file: &File) -> Option<StableIdentity> {
    use std::os::fd::AsRawFd;
    use std::time::UNIX_EPOCH;

    fn attributes<const N: usize>(file: &File, common: u32, volume: u32) -> Option<[u32; N]> {
        let mut request = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: common,
            volattr: volume,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        let mut words = [0u32; N];
        let size = std::mem::size_of_val(&words);
        // SAFETY: the file remains open and both pointers refer to initialized,
        // writable storage of the stated sizes for the duration of the syscall.
        let result = unsafe {
            libc::fgetattrlist(
                file.as_raw_fd(),
                (&mut request as *mut libc::attrlist).cast(),
                words.as_mut_ptr().cast(),
                size,
                0,
            )
        };
        (result == 0 && words[0] as usize == size).then_some(words)
    }

    let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: a live descriptor and writable storage for the native result.
    if unsafe { libc::fstatfs(file.as_raw_fd(), fs.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: fstatfs returned success and initialized its output.
    let fs = unsafe { fs.assume_init() };
    if !fs
        .f_fstypename
        .starts_with(&[b'a' as _, b'p' as _, b'f' as _, b's' as _, 0])
    {
        return None;
    }
    let [_, uuid_a, uuid_b, uuid_c, uuid_d] =
        attributes(file, 0, libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID)?;
    let mut volume_bytes = [0; 16];
    for (bytes, word) in volume_bytes
        .chunks_exact_mut(4)
        .zip([uuid_a, uuid_b, uuid_c, uuid_d])
    {
        bytes.copy_from_slice(&word.to_ne_bytes());
    }
    let volume = uuid::Uuid::from_bytes(volume_bytes);
    let [_, id_a, id_b] = attributes(file, libc::ATTR_CMN_OBJPERMANENTID, 0)?;
    let mut object_bytes = [0; 8];
    object_bytes[..4].copy_from_slice(&id_a.to_ne_bytes());
    object_bytes[4..].copy_from_slice(&id_b.to_ne_bytes());
    let object = u64::from_ne_bytes(object_bytes);
    let created = file
        .metadata()
        .ok()?
        .created()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?;
    if volume.is_nil() || object == 0 || created.is_zero() {
        return None;
    }
    Some(StableIdentity {
        volume: volume.to_string(),
        object,
        created_seconds: created.as_secs(),
        created_nanos: created.subsec_nanos(),
    })
}
