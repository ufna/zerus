//! Test-only kernel inputs. They run the same admission and ancestry walk as
//! the proof's own model tests; no caller result or authority is injected.
use super::*;

pub(super) fn selected(name: &str) -> bool {
    std::env::var("HGS_TEAMS_TEST_CONTEXT").as_deref() == Ok(name)
}
pub(super) fn called() {
    if let Some(path) = std::env::var_os("HGS_TEAMS_TEST_PROOF_READS") {
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .unwrap(),
            "proof"
        )
        .unwrap();
    }
}
pub(super) fn opaque_ancestors() -> Result<Vec<ancestry::Ancestor>, Problem> {
    ancestry::walk(
        3,
        |pid| {
            Ok(match pid {
                3 | 2 => ancestry::Inspection::Own(ancestry::Process {
                    parent: pid - 1,
                    birth: Some("fixture-birth".into()),
                }),
                _ => ancestry::Inspection::Foreign,
            })
        },
        |_| Err(std::io::ErrorKind::PermissionDenied.into()),
    )
}
