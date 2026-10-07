//! Share the logged-in Mac user's security context with native Claude children.
//! The per-user LaunchAgent transfers an OS session capability, never credentials.

#[cfg(target_os = "macos")]
extern "C" {
    fn hgs_macos_session_join() -> std::ffi::c_int;
    fn hgs_macos_session_serve() -> std::ffi::c_int;
    fn hgs_macos_keychain_state() -> std::ffi::c_int;
}

pub fn join() -> bool {
    #[cfg(target_os = "macos")]
    { unsafe { hgs_macos_session_join() == 0 } }
    #[cfg(not(target_os = "macos"))]
    { true }
}

pub fn keychain_locked() -> Option<bool> {
    #[cfg(target_os = "macos")]
    { match unsafe { hgs_macos_keychain_state() } { 1 => Some(true), 2 => Some(false), _ => None } }
    #[cfg(not(target_os = "macos"))]
    { None }
}

pub fn serve() -> i32 {
    #[cfg(target_os = "macos")]
    { unsafe { hgs_macos_session_serve() } }
    #[cfg(not(target_os = "macos"))]
    { 1 }
}
