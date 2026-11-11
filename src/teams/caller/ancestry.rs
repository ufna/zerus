//! Bounded process-table walk. An unreadable environment is tolerated only at
//! the verified top of this user's tree, whose parent is foreign. Parent zero
//! refuses, including a same-user process at a PID namespace boundary. An
//! environment that macOS withholds is unreadable; below the top its refusal
//! tells the person where their commands run.
use super::{claim, environment, is_withheld, unreadable, withheld_below_top};
use zerus_team_plan::Problem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Process {
    pub(super) parent: u32,
    pub(super) birth: Option<String>,
}

#[derive(Debug)]
pub(super) struct Ancestor {
    pub(super) pid: u32,
    pub(super) identity: Process,
    pub(super) claim: Option<super::Claim>,
    pub(super) top: bool,
}

#[derive(Clone)]
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
pub(super) enum Inspection {
    Own(Process),
    Foreign,
}

pub(super) fn ancestors() -> Result<Vec<Ancestor>, Problem> {
    walk(std::process::id(), inspect, environment)
}

/// A replacement, unknown parent, loop or cap is never a successful end.
pub(super) fn walk(
    mut pid: u32,
    mut inspect: impl FnMut(u32) -> Result<Inspection, Problem>,
    mut environment: impl FnMut(u32) -> std::io::Result<Vec<u8>>,
) -> Result<Vec<Ancestor>, Problem> {
    let mut chain: Vec<Ancestor> = Vec::new();
    let mut current = inspect(pid)?;
    for _ in 0..64 {
        let Inspection::Own(identity) = current else {
            return Err(unreadable());
        };
        let parent = identity.parent;
        if chain.iter().any(|row| row.pid == pid) || parent == pid || parent == 0 {
            return Err(unreadable());
        }
        let next = inspect(parent)?;
        let top = matches!(next, Inspection::Foreign);
        let claim = match environment(pid) {
            Ok(bytes) => {
                if identity.birth.is_none() {
                    return Err(unreadable());
                }
                claim(&bytes)?
            }
            Err(_) if top => None,
            Err(error) if is_withheld(&error) => return Err(withheld_below_top()),
            Err(_) => return Err(unreadable()),
        };
        if !top && identity.birth.is_none() {
            return Err(unreadable());
        }
        if !matches!(inspect(pid)?, Inspection::Own(ref now) if *now == identity) {
            return Err(unreadable());
        }
        chain.push(Ancestor {
            pid,
            identity,
            claim,
            top,
        });
        if top {
            return Ok(chain);
        }
        pid = parent;
        current = next;
    }
    Err(unreadable())
}

pub(super) fn unchanged(chain: &[Ancestor]) -> Result<(), Problem> {
    for ancestor in chain {
        if !matches!(inspect(ancestor.pid)?, Inspection::Own(ref now) if *now == ancestor.identity)
        {
            return Err(unreadable());
        }
        // T3: no deterministic test pins a credential change in the boundary
        // parent after the first walk; that requires a native race/injection seam.
        if ancestor.top && !matches!(inspect(ancestor.identity.parent)?, Inspection::Foreign) {
            return Err(unreadable());
        }
    }
    Ok(())
}

/// Credentials come from Uid in the process table. A nondumpable Linux process
/// has root-owned procfs entries even while its effective UID remains ours.
#[cfg(target_os = "linux")]
pub(super) fn inspect(pid: u32) -> Result<Inspection, Problem> {
    let status = bounded(&format!("/proc/{pid}/status"))?;
    if !effective_owner(&status, unsafe { libc::geteuid() })? {
        return Ok(Inspection::Foreign);
    }
    let stat = bounded(&format!("/proc/{pid}/stat"))?;
    let (_, tail) = stat.rsplit_once(')').ok_or_else(unreadable)?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    if fields.len() < 20 || matches!(fields[0], "Z" | "X") {
        return Err(unreadable());
    }
    let parent = fields[1].parse().map_err(|_| unreadable())?;
    let native = crate::platform::process_identity(pid);
    if native
        .as_ref()
        .is_some_and(|native| native.parent != parent)
    {
        return Err(unreadable());
    }
    // No synthetic birth: None is usable only at the verified foreign-parent
    // top with an unreadable environment, and never verifies an agent claim.
    Ok(Inspection::Own(Process {
        parent,
        birth: native.map(|native| native.birth),
    }))
}

#[cfg(target_os = "linux")]
fn effective_owner(status: &str, owner: u32) -> Result<bool, Problem> {
    let uids = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .ok_or_else(unreadable)?
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| unreadable())?;
    if uids.len() != 4 {
        return Err(unreadable());
    }
    Ok(uids[1] == owner)
}

#[cfg(target_os = "linux")]
fn bounded(path: &str) -> Result<String, Problem> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| unreadable())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    if bytes.len() > 65536 {
        return Err(unreadable());
    }
    String::from_utf8(bytes).map_err(|_| unreadable())
}

#[cfg(target_os = "macos")]
pub(super) fn inspect(pid: u32) -> Result<Inspection, Problem> {
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    // SAFETY: a correctly sized writable native structure.
    let count = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdshortinfo).cast(),
            size,
        )
    };
    if count != size || info.pbsi_pid != pid {
        return Err(unreadable());
    }
    if info.pbsi_uid != unsafe { libc::geteuid() } {
        return Ok(Inspection::Foreign);
    }
    if info.pbsi_status == 5 {
        return Err(unreadable());
    }
    let native = crate::platform::process_identity(pid);
    if native
        .as_ref()
        .is_some_and(|native| native.parent != info.pbsi_ppid)
    {
        return Err(unreadable());
    }
    Ok(Inspection::Own(Process {
        parent: info.pbsi_ppid,
        birth: native.map(|native| native.birth),
    }))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn inspect(_pid: u32) -> Result<Inspection, Problem> {
    Err(unreadable())
}

#[cfg(test)]
mod tests;
