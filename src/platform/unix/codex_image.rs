//! Darwin native Codex queue launch. A helper starts suspended and can run
//! only after the kernel identifies its signed main image as the live TUI's.
use super::{collect_bounded, helper_group, spawn_turn_before, RunError};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsStr},
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};

const PROC_PIDUNIQIDENTIFIERINFO: libc::c_int = 17;
const CS_OPS_STATUS: u32 = 0;
const CS_OPS_CDHASH: u32 = 5;
const CS_VALID: u32 = 1;

unsafe extern "C" {
    fn csops(pid: libc::pid_t, operation: u32, buffer: *mut libc::c_void, size: usize) -> i32;
}

#[repr(C)]
struct Unique {
    image: [u8; 16],
    birth: u64,
    parent_birth: u64,
    version: i32,
    parent_version: i32,
    reserved: [u64; 2],
}

const _: () = assert!(std::mem::size_of::<Unique>() == 56);

#[derive(Clone, Copy, PartialEq, Eq)]
struct Loaded {
    birth: u64,
    version: i32,
    cdhash: [u8; 20],
}

fn process_image_birth(pid: libc::pid_t) -> Option<(u64, i32)> {
    // SAFETY: The layout for this flavor consists of numeric fields and fixed
    // arrays. The size above is verified against the SDK.
    let mut value: Unique = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<Unique>() as libc::c_int;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            PROC_PIDUNIQIDENTIFIERINFO,
            0,
            (&mut value as *mut Unique).cast(),
            size,
        )
    };
    (read == size && value.birth != 0 && value.version != 0).then_some((value.birth, value.version))
}

fn kernel_loaded(pid: libc::pid_t) -> Option<Loaded> {
    let (first_birth, first_version) = process_image_birth(pid)?;
    let mut status = 0u32;
    let mut cdhash = [0; 20];
    // SAFETY: only read-only operations with their exact writable buffers.
    if unsafe { csops(pid, CS_OPS_STATUS, (&mut status as *mut u32).cast(), 4) } != 0
        || status & CS_VALID == 0
        || unsafe { csops(pid, CS_OPS_CDHASH, cdhash.as_mut_ptr().cast(), cdhash.len()) } != 0
        || cdhash == [0; 20]
        || process_image_birth(pid)? != (first_birth, first_version)
    {
        return None;
    }
    Some(Loaded {
        birth: first_birth,
        version: first_version,
        cdhash,
    })
}

pub(crate) struct Image {
    pid: libc::pid_t,
    loaded: Loaded,
}

impl Image {
    pub(crate) fn read(pid: u32) -> Option<Self> {
        let pid = libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 1)?;
        super::process_identity(pid as u32)?;
        let loaded = kernel_loaded(pid)?;
        Some(Self { pid, loaded })
    }

    pub(crate) fn current(&self) -> bool {
        kernel_loaded(self.pid) == Some(self.loaded)
    }

    fn admits(&self, helper: Option<Loaded>) -> bool {
        helper.is_some_and(|helper| helper.cdhash == self.loaded.cdhash)
    }
}

fn checked(error: libc::c_int) -> io::Result<()> {
    if error == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error))
    }
}

struct SpawnAttrs(libc::posix_spawnattr_t);

impl SpawnAttrs {
    fn new() -> io::Result<Self> {
        let mut value = std::ptr::null_mut();
        checked(unsafe { libc::posix_spawnattr_init(&mut value) })?;
        let mut attrs = Self(value);
        checked(unsafe {
            libc::posix_spawnattr_setflags(
                &mut attrs.0,
                (libc::POSIX_SPAWN_START_SUSPENDED
                    | libc::POSIX_SPAWN_CLOEXEC_DEFAULT
                    | libc::POSIX_SPAWN_SETPGROUP
                    | libc::POSIX_SPAWN_SETSIGDEF
                    | libc::POSIX_SPAWN_SETSIGMASK) as libc::c_short,
            )
        })?;
        checked(unsafe { libc::posix_spawnattr_setpgroup(&mut attrs.0, 0) })?;
        // If the writer dies before releasing the helper, Darwin sends HUP
        // and CONT to its stopped orphan group. HUP must terminate it even
        // when the writer inherited an ignored or blocked HUP.
        unsafe {
            let mut defaults = 0;
            libc::sigaddset(&mut defaults, libc::SIGHUP);
            checked(libc::posix_spawnattr_setsigdefault(&mut attrs.0, &defaults))?;
            let mut mask = 0;
            checked(libc::pthread_sigmask(
                libc::SIG_SETMASK,
                std::ptr::null(),
                &mut mask,
            ))?;
            libc::sigdelset(&mut mask, libc::SIGHUP);
            checked(libc::posix_spawnattr_setsigmask(&mut attrs.0, &mask))?;
        }
        Ok(attrs)
    }
}

impl Drop for SpawnAttrs {
    fn drop(&mut self) {
        unsafe { libc::posix_spawnattr_destroy(&mut self.0) };
    }
}

struct FileActions(libc::posix_spawn_file_actions_t);

impl FileActions {
    fn new() -> io::Result<Self> {
        let mut value = std::ptr::null_mut();
        checked(unsafe { libc::posix_spawn_file_actions_init(&mut value) })?;
        Ok(Self(value))
    }

    fn add_output(&mut self, fd: &OwnedFd, output: libc::c_int) -> io::Result<()> {
        checked(unsafe {
            libc::posix_spawn_file_actions_adddup2(&mut self.0, fd.as_raw_fd(), output)
        })
    }
}

impl Drop for FileActions {
    fn drop(&mut self) {
        unsafe { libc::posix_spawn_file_actions_destroy(&mut self.0) };
    }
}

fn higher_fd(socket: &UnixStream) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: fcntl returned a new descriptor owned by this call.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn string(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

fn pointers(values: &[CString]) -> Vec<*mut libc::c_char> {
    values
        .iter()
        .map(|value| value.as_ptr().cast_mut())
        .chain(std::iter::once(std::ptr::null_mut()))
        .collect()
}

fn queue_env(command: &Command) -> io::Result<Vec<CString>> {
    let mut env: BTreeMap<_, _> = std::env::vars_os().collect();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            env.insert(key.to_owned(), value.to_owned());
        } else {
            env.remove(key);
        }
    }
    env.into_iter()
        .filter(|(key, _)| {
            let key = key.as_bytes();
            // The checked main image must start with the normal system loader.
            !key.starts_with(b"DYLD_") && !key.starts_with(b"HGS_")
        })
        .map(|(mut key, value)| {
            key.push("=");
            key.push(value);
            string(&key)
        })
        .collect()
}

pub(crate) struct Pending {
    pid: libc::pid_t,
    group: helper_group::Group<'static>,
    output: Option<(UnixStream, UnixStream)>,
    loaded: Loaded,
}

impl Pending {
    /// Only called with the internally constructed Codex queue Command.
    /// Its program, arguments, cwd and environment overrides are authoritative.
    pub(crate) fn prepare(
        command: &Command,
        image: &Image,
        until: Instant,
    ) -> Result<Self, RunError> {
        fn launch(
            command: &Command,
            until: Instant,
        ) -> io::Result<(libc::pid_t, UnixStream, UnixStream)> {
            let program = string(command.get_program())?;
            let mut args = vec![program.clone()];
            args.extend(
                command
                    .get_args()
                    .map(string)
                    .collect::<io::Result<Vec<_>>>()?,
            );
            let env = queue_env(command)?;
            let argv = pointers(&args);
            let envp = pointers(&env);
            let turn = spawn_turn_before(until, &|| false).map_err(|_| io::ErrorKind::TimedOut)?;
            helper_group::admit()?;
            let (stdout, out) = zerus_client_core::unix_spawn::socket_pair_in_turn(&turn)?;
            let (stderr, err) = zerus_client_core::unix_spawn::socket_pair_in_turn(&turn)?;
            let (out_fd, err_fd) = (higher_fd(&out)?, higher_fd(&err)?);
            let mut files = FileActions::new()?;
            let attrs = SpawnAttrs::new()?;
            checked(unsafe {
                libc::posix_spawn_file_actions_addopen(
                    &mut files.0,
                    libc::STDIN_FILENO,
                    c"/dev/null".as_ptr(),
                    libc::O_RDONLY,
                    0,
                )
            })?;
            files.add_output(&out_fd, libc::STDOUT_FILENO)?;
            files.add_output(&err_fd, libc::STDERR_FILENO)?;
            if let Some(cwd) = command.get_current_dir() {
                let cwd = string(cwd.as_os_str())?;
                checked(unsafe {
                    libc::posix_spawn_file_actions_addchdir_np(&mut files.0, cwd.as_ptr())
                })?;
            }
            if Instant::now() >= until {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut pid = 0;
            // SAFETY: all C strings, argv/env arrays, attributes and file
            // actions remain live through spawn. The child is stopped in exec.
            checked(unsafe {
                libc::posix_spawn(
                    &mut pid,
                    program.as_ptr(),
                    &files.0,
                    &attrs.0,
                    argv.as_ptr(),
                    envp.as_ptr(),
                )
            })?;
            Ok((pid, stdout, stderr))
        }

        if !image.current() {
            return Err(RunError::Stopped);
        }
        let (pid, stdout, stderr) = launch(command, until).map_err(RunError::Spawn)?;
        let mut group = helper_group::Group::suspended(pid);
        let qualified = (|| {
            // Observe the original child's stop without reaping it and freeing
            // its PID/group. No pathname or version response grants this proof.
            loop {
                if group.exited().ok()?.is_some() || Instant::now() >= until {
                    return None;
                }
                let stopped = child_stopped(pid)?;
                if stopped {
                    let loaded = kernel_loaded(pid)?;
                    return (image.admits(Some(loaded)) && image.current()).then_some(loaded);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        let Some(loaded) = qualified else {
            let _ = group.stop();
            return Err(RunError::Stopped);
        };
        Ok(Self {
            pid,
            group,
            output: Some((stdout, stderr)),
            loaded,
        })
    }

    pub(crate) fn run(
        mut self,
        image: &Image,
        limits: [usize; 2],
        until: Instant,
    ) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), RunError> {
        if self.group.exited().map_err(RunError::Io)?.is_some()
            || !image.current()
            || child_stopped(self.pid) != Some(true)
            || kernel_loaded(self.pid) != Some(self.loaded)
            || Instant::now() >= until
        {
            return Err(RunError::Stopped);
        }
        // SAFETY: the unreaped, originally spawned child was just checked.
        if unsafe { libc::kill(self.pid, libc::SIGCONT) } != 0 {
            return Err(RunError::Io(io::Error::last_os_error()));
        }
        let (stdout, stderr) = self.output.take().expect("one original child");
        collect_bounded(&mut self.group, stdout, stderr, limits, until, &|| false)
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.group.stop();
    }
}

fn child_stopped(pid: libc::pid_t) -> Option<bool> {
    // The libc SDK type consists of numeric fields and fixed byte arrays.
    let mut child: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&child) as libc::c_int;
    if unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut child as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    } != size
        || child.pbi_pid != pid as u32
        || child.pbi_ppid != std::process::id()
        || child.pbi_pgid != pid as u32
        || child.pbi_uid != unsafe { libc::geteuid() }
        || child.pbi_ruid != unsafe { libc::getuid() }
    {
        return None;
    }
    Some(child.pbi_status == libc::SSTOP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_matching_loaded_code_admits_the_original_queue_helper() {
        let original = Loaded {
            birth: 10,
            version: 1,
            cdhash: [1; 20],
        };
        let image = Image {
            pid: 10,
            loaded: original,
        };
        let child = Loaded {
            birth: 11,
            ..original
        };
        assert!(image.admits(Some(child)));
        assert!(!image.admits(Some(Loaded {
            cdhash: [2; 20],
            ..child
        })));
        assert!(!image.admits(None));
    }
}
