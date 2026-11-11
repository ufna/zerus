//! Bounded process-table walk. An unreadable environment is tolerated only at
//! the verified top of this user's tree, whose parent is foreign or absent.
use super::{claim, environment, unreadable};
use zerus_team_plan::Problem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Process {
    pub(super) parent: u32,
    pub(super) birth: String,
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
        if chain.iter().any(|row| row.pid == pid) || parent == pid {
            return Err(unreadable());
        }
        let next = if parent == 0 {
            Inspection::Foreign
        } else {
            inspect(parent)?
        };
        let top = matches!(next, Inspection::Foreign);
        let claim = match environment(pid) {
            Ok(bytes) => claim(&bytes)?,
            Err(_) if top => None,
            Err(_) => return Err(unreadable()),
        };
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
        if ancestor.top
            && ancestor.identity.parent != 0
            && !matches!(inspect(ancestor.identity.parent)?, Inspection::Foreign)
        {
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
    if uids[1] != unsafe { libc::geteuid() } {
        return Ok(Inspection::Foreign);
    }
    let stat = bounded(&format!("/proc/{pid}/stat"))?;
    let (_, tail) = stat.rsplit_once(')').ok_or_else(unreadable)?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    if fields.len() < 20 || matches!(fields[0], "Z" | "X") {
        return Err(unreadable());
    }
    Ok(Inspection::Own(Process {
        parent: fields[1].parse().map_err(|_| unreadable())?,
        birth: fields[19].to_owned(),
    }))
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
    // KERN_PROC_PID exposes the process-table start timeval even when full
    // proc_pidinfo/environment access is denied to an unprivileged caller.
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROC,
        libc::KERN_PROC_PID,
        pid as i32,
    ];
    let mut table = [0u64; 128];
    let mut bytes = std::mem::size_of_val(&table);
    // SAFETY: the table, MIB and size cover the writable buffers.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            4,
            table.as_mut_ptr().cast(),
            &mut bytes,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || !(16..=std::mem::size_of_val(&table)).contains(&bytes) || info.pbsi_status == 5 {
        return Err(unreadable());
    }
    Ok(Inspection::Own(Process {
        parent: info.pbsi_ppid,
        birth: format!("{}:{}", table[0], table[1]),
    }))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn inspect(_pid: u32) -> Result<Inspection, Problem> {
    Err(unreadable())
}

#[cfg(test)]
mod tests;
