//! Platform availability for both callers, and namespace admission only for
//! the unmarked Person proof.
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

// Lift only after the real-process caller suite has passed on a Mac, including
// matching walk/record births through platform::process_identity.
const MACOS_ANCESTRY_QUALIFIED: bool = false;

pub(super) fn available() -> Result<(), Problem> {
    if !(cfg!(target_os = "linux") || cfg!(target_os = "macos") && MACOS_ANCESTRY_QUALIFIED) {
        return Err(Problem::new(code::NOT_ALLOWED,
            "The Teams preview is not available on this platform yet: its real-process ancestry proof has not been qualified."));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) fn unmarked() -> Result<(), Problem> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    // Linux UAPI linux/nsfs.h: USER_NS_INIT_INO. Compare the actual namespace
    // object, not /proc/1 (which a PID namespace can replace) or uid_map (a
    // privileged child namespace can retain a full identity mapping).
    const USER_NS_INIT_INO: u64 = 0xEFFF_FFFD;
    const NS_GET_NSTYPE: libc::c_ulong = 0xb703;
    let refused = || {
        Problem::new(code::NOT_ALLOWED,
        "The unmarked Teams caller proof requires the initial Linux user namespace; this process context cannot establish the person.")
    };
    let namespace = std::fs::File::open("/proc/self/ns/user").map_err(|_| refused())?;
    // SAFETY: this nsfs ioctl takes no pointer and does not mutate the process.
    let kind = unsafe { libc::ioctl(namespace.as_raw_fd(), NS_GET_NSTYPE) };
    if kind != libc::CLONE_NEWUSER
        || namespace.metadata().map_err(|_| refused())?.ino() != USER_NS_INIT_INO
    {
        return Err(refused());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(super) fn unmarked() -> Result<(), Problem> {
    Ok(())
}
