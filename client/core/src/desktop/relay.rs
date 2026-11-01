//! Select the desktop service CLI separately from the ordinary agent runner.
//! A returned path routes a command; it is not frontend or notification authority.
use std::path::{Path, PathBuf};

#[cfg(any(test, target_os = "macos"))]
const UNAVAILABLE: &str = "paired_desktop_cli_unavailable";

/// Off the UI thread. A paired Mac app uses only its verified own bundled hgs;
/// a broken purported pair stays unavailable, never falling through to PATH or
/// an explicit ordinary-CLI override. Source/unpaired and other platforms retain
/// the caller's ordinary selection. Recheck for each new service exchange.
pub fn select(ordinary: &Path) -> Result<PathBuf, &'static str> {
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe().map_err(|_| UNAVAILABLE)?;
        select_mac(&executable, ordinary, || {
            #[cfg(feature = "macos-native-peer")]
            {
                crate::macos_notify::peer::installation::select_own()
                    .map(|selection| selection.service_executable().to_owned())
                    .map_err(|_| UNAVAILABLE)
            }
            #[cfg(not(feature = "macos-native-peer"))]
            Err(UNAVAILABLE)
        })
    }
    #[cfg(not(target_os = "macos"))]
    Ok(ordinary.into())
}

#[cfg(any(test, target_os = "macos"))]
fn select_mac(
    executable: &Path,
    ordinary: &Path,
    verify_own: impl FnOnce() -> Result<PathBuf, &'static str>,
) -> Result<PathBuf, &'static str> {
    let fallback = || Ok(ordinary.to_owned());
    // This shape/presence check distinguishes legacy unpaired source installs.
    // It establishes NO trust; only select_own may select a purported pair.
    let Some(macos) = executable.parent() else {
        return fallback();
    };
    let Some(contents) = macos.parent() else {
        return fallback();
    };
    let Some(bundle) = contents.parent() else {
        return fallback();
    };
    if !executable.is_absolute()
        || executable.file_name().is_none_or(|v| v != "hgs-tray")
        || macos.file_name().is_none_or(|v| v != "MacOS")
        || contents.file_name().is_none_or(|v| v != "Contents")
        || bundle.extension().is_none_or(|v| v != "app")
    {
        return fallback();
    }
    let candidate = macos.join("hgs");
    match std::fs::symlink_metadata(&candidate) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A local pair whose helper disappeared must not become a legacy
            // install. Presence only triggers verification, never supplies trust.
            let record = contents.join("Resources/zerus-local-pair.json");
            match std::fs::symlink_metadata(record) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return fallback(),
                Err(_) => return Err(UNAVAILABLE),
                Ok(_) => {}
            }
        }
        Err(_) => return Err(UNAVAILABLE),
        Ok(_) => {}
    }
    let verified = verify_own()?;
    if verified != candidate {
        return Err(UNAVAILABLE);
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_routing_requires_native_own_selection_and_ignores_the_ordinary_override() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("zerus-relay-{}-{nonce}", std::process::id()));
        let code = root.join("Example.app/Contents/MacOS/hgs-tray");
        std::fs::create_dir_all(code.parent().unwrap()).unwrap();
        let bundled = code.with_file_name("hgs");
        let ordinary = root.join("stable/hgs");
        // No hgs: legacy source/unpaired app keeps the existing resolver.
        assert_eq!(
            select_mac(&code, &ordinary, || panic!("unpaired")),
            Ok(ordinary.clone())
        );
        std::fs::write(
            &bundled,
            b"synthetic code; native verification is MODELLED below",
        )
        .unwrap();
        assert_eq!(
            select_mac(&code, &ordinary, || Ok(bundled.clone())),
            Ok(bundled.clone())
        );
        assert_eq!(
            select_mac(&code, &ordinary, || Err(UNAVAILABLE)),
            Err(UNAVAILABLE)
        );
        assert_eq!(
            select_mac(&code, &ordinary, || Ok(ordinary.clone())),
            Err(UNAVAILABLE)
        );
        std::fs::remove_file(&bundled).unwrap();
        let record = code
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Resources/zerus-local-pair.json");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(record, b"synthetic, not authority").unwrap();
        assert_eq!(
            select_mac(&code, &ordinary, || Err(UNAVAILABLE)),
            Err(UNAVAILABLE)
        );
        assert_eq!(
            select_mac(&root.join("target/hgs-tray"), &ordinary, || panic!(
                "source"
            )),
            Ok(ordinary.clone())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_platforms_keep_the_callers_exact_selection() {
        assert_eq!(
            select(Path::new("explicit-fixture-hgs")),
            Ok("explicit-fixture-hgs".into())
        );
    }
}
