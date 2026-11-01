//! Per-user Windows activation, matching Qt's `sessions\n` / `show\n` pipe.
//! The private exclusive file is the lifetime lock; a foreign pipe cannot
//! grant ownership. Only the current SID can access a pipe we create.
use crate::platform::windows::{own_sid, owned, token_sid, wide, Security};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::{
    fs::OpenOptionsExt,
    io::{AsRawHandle, OwnedHandle},
};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex, PoisonError,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use unicode_casefold::UnicodeCaseFold;
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
    UI::WindowsAndMessaging::AllowSetForegroundWindow,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    Show,
    Sessions,
}
impl Activation {
    fn line(self) -> &'static [u8] {
        match self {
            Self::Show => b"show\n",
            Self::Sessions => b"sessions\n",
        }
    }
    fn parse(bytes: &[u8]) -> Option<Self> {
        match bytes.trim_ascii() {
            b"show" => Some(Self::Show),
            b"sessions" => Some(Self::Sessions),
            _ => None,
        }
    }
}

pub struct Listening {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    _lock: File,
}
impl Drop for Listening {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub struct Claimed {
    pub listening: Listening,
    pub activations: Receiver<Activation>,
}
pub enum Claim {
    Claimed(Claimed),
    Forwarded,
    Failed(String),
}
pub struct InstanceState(pub Mutex<Option<Listening>>);
impl InstanceState {
    pub fn release(&self) {
        drop(self.0.lock().unwrap_or_else(PoisonError::into_inner).take());
    }
}

fn key(name: &str, home: &Path) -> String {
    let digest = Sha256::digest(
        home.to_string_lossy()
            .replace('\\', "/")
            .case_fold()
            .collect::<String>()
            .as_bytes(),
    );
    format!("{name}-{:x}", digest)[..name.len() + 17].to_owned()
}

fn same_user_server(pipe: &OwnedHandle, sid: &str) -> io::Result<u32> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })?;
    let mut token = std::ptr::null_mut();
    if unsafe {
        OpenProcessToken(
            process.as_raw_handle(),
            windows_sys::Win32::Security::TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if token_sid(&owned(token)?)? != sid {
        return Err(io::Error::other(
            "Activation pipe belongs to a different Windows user",
        ));
    }
    Ok(pid)
}

fn forward(name: &str, activation: Activation) -> bool {
    let Ok(sid) = own_sid() else { return false };
    let pipe = wide(&format!(r"\\.\pipe\{name}"));
    let end = Instant::now() + Duration::from_millis(200);
    loop {
        let handle = unsafe {
            CreateFileW(
                pipe.as_ptr(),
                FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES | FILE_READ_ATTRIBUTES,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if let Ok(handle) = owned(handle) {
            let Ok(pid) = same_user_server(&handle, &sid) else {
                return false;
            };
            if activation == Activation::Sessions {
                unsafe {
                    AllowSetForegroundWindow(pid);
                }
            }
            let mode = PIPE_NOWAIT;
            if unsafe {
                SetNamedPipeHandleState(
                    handle.as_raw_handle(),
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } == 0
            {
                return false;
            }
            let bytes = activation.line();
            let mut written = 0;
            return unsafe {
                WriteFile(
                    handle.as_raw_handle(),
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            } != 0
                && written == bytes.len() as u32;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn listen(pipe: OwnedHandle, stop: Arc<AtomicBool>, sender: mpsc::SyncSender<Activation>) {
    while !stop.load(Ordering::Acquire) {
        let connected =
            unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0;
        if !connected && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let end = Instant::now() + Duration::from_millis(200);
        let mut bytes = Vec::with_capacity(32);
        while Instant::now() < end && !stop.load(Ordering::Acquire) && bytes.len() < 32 {
            let mut buffer = [0u8; 32];
            let mut read = 0;
            let ok = unsafe {
                ReadFile(
                    pipe.as_raw_handle(),
                    buffer.as_mut_ptr(),
                    (32 - bytes.len()) as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 && read > 0 {
                bytes.extend_from_slice(&buffer[..read as usize]);
                if let Some(lf) = bytes.iter().position(|b| *b == b'\n') {
                    if let Some(activation) = Activation::parse(&bytes[..=lf]) {
                        let _ = sender.try_send(activation);
                    }
                    break;
                }
            } else if unsafe { GetLastError() } != ERROR_NO_DATA {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            DisconnectNamedPipe(pipe.as_raw_handle());
        }
    }
}

pub fn claim(
    name: &str,
    _label: Option<&str>,
    activation: Activation,
    _reply: Option<()>,
) -> Claim {
    let home = crate::identity::home_dir();
    if home.as_os_str().is_empty() {
        return Claim::Failed("The Windows profile directory is unavailable".into());
    }
    let name = key(name, &home);
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"))
        .join("hgdev/hgs-tray");
    claim_at(&name, &directory, activation)
}
fn claim_at(name: &str, directory: &Path, activation: Activation) -> Claim {
    // Also recognizes an already-running Qt GUI using this activation protocol.
    if forward(name, activation) {
        return Claim::Forwarded;
    }
    if let Err(e) = std::fs::create_dir_all(directory) {
        return Claim::Failed(e.to_string());
    }
    let mut lock = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(directory.join(format!("{name}.instance-lock")))
    {
        Ok(lock) => lock,
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION as i32) => {
            let _ = forward(name, activation);
            return Claim::Forwarded;
        }
        Err(e) => return Claim::Failed(e.to_string()),
    };
    if let Err(e) = lock
        .set_len(0)
        .and_then(|()| writeln!(lock, "{}", std::process::id()))
    {
        return Claim::Failed(e.to_string());
    }
    let security = match Security::user_only() {
        Ok(s) => s,
        Err(e) => return Claim::Failed(e.to_string()),
    };
    let attrs = security.attributes();
    let pipe_name = wide(&format!(r"\\.\pipe\{name}"));
    let pipe = owned(unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            64,
            200,
            &attrs,
        )
    });
    let (tx, rx) = mpsc::sync_channel(32);
    let stop = Arc::new(AtomicBool::new(false));
    let thread = match pipe {
        Ok(pipe) => {
            let stopped = stop.clone();
            Some(std::thread::spawn(move || listen(pipe, stopped, tx)))
        }
        Err(e) => {
            eprintln!("zerus-client: cannot listen for a second Windows instance: {e}");
            None
        }
    };
    Claim::Claimed(Claimed {
        listening: Listening {
            stop,
            thread,
            _lock: lock,
        },
        activations: rx,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_key_keeps_qt_unicode_casefold_compatibility() {
        assert_eq!(
            key("hgs-tray", Path::new(r"C:\Users\STRAẞE")),
            key("hgs-tray", Path::new("c:/users/strasse"))
        );
    }
    #[test]
    fn activation_is_bounded_and_unknown_commands_do_not_open_a_window() {
        assert_eq!(
            Activation::parse(b" sessions\r\n"),
            Some(Activation::Sessions)
        );
        assert_eq!(Activation::parse(b"show\n"), Some(Activation::Show));
        assert_eq!(Activation::parse(b"quit\n"), None);
    }
    #[test]
    fn login_shortcut_plus_run_registration_never_opens_a_background_window() {
        let root =
            std::env::temp_dir().join(format!("zerus-background-test-{}", uuid::Uuid::new_v4()));
        let name = format!("zerus-background-{}", uuid::Uuid::new_v4());
        let options = crate::cli::Options {
            background: true,
            ..Default::default()
        };
        assert!(!options.opens_sessions(true));
        assert!(!crate::lifecycle::window_at_start(
            options.opens_sessions(true),
            true
        ));
        let Claim::Claimed(first) = claim_at(&name, &root, Activation::Show) else {
            panic!("first login start")
        };
        assert!(matches!(
            claim_at(&name, &root, Activation::Show),
            Claim::Forwarded
        ));
        assert_eq!(
            first
                .activations
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Activation::Show
        );
        drop(first);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_second_start_forwards_and_release_allows_a_new_owner() {
        let root = std::env::temp_dir().join(format!("zerus-pipe-test-{}", uuid::Uuid::new_v4()));
        let name = format!("zerus-test-{}", uuid::Uuid::new_v4());
        let Claim::Claimed(first) = claim_at(&name, &root, Activation::Show) else {
            panic!("first claim")
        };
        assert!(matches!(
            claim_at(&name, &root, Activation::Sessions),
            Claim::Forwarded
        ));
        assert_eq!(
            first
                .activations
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Activation::Sessions
        );
        drop(first);
        let Claim::Claimed(next) = claim_at(&name, &root, Activation::Show) else {
            panic!("released claim")
        };
        drop(next);
        std::fs::remove_dir_all(root).unwrap();
    }
}
