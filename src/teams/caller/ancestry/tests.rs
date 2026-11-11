use super::*;

fn own(parent: u32) -> Inspection {
    Inspection::Own(Process {
        parent,
        birth: Some("birth".into()),
    })
}
fn empty(_: u32) -> std::io::Result<Vec<u8>> {
    Ok(Vec::new())
}
fn denied(_: u32) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
}

#[test]
fn unreadable_top_is_allowed_only_with_an_identified_foreign_parent() {
    let table = |pid| {
        Ok(if pid == 2 {
            own(1)
        } else {
            Inspection::Foreign
        })
    };
    let chain = walk(2, table, denied).unwrap();
    assert_eq!(chain.len(), 1);
    assert!(chain[0].top);
    assert!(walk(2, |_| Err(unreadable()), denied).is_err());
}

#[test]
fn readable_top_keeps_its_claim_but_parent_zero_refuses() {
    let chain = walk(
        2,
        |pid| {
            Ok(if pid == 2 {
                own(1)
            } else {
                Inspection::Foreign
            })
        },
        |_| Ok(b"HGS_SESSION=agent\0HGS_RUN_ID=run\0".to_vec()),
    )
    .unwrap();
    assert_eq!(
        chain[0]
            .claim
            .as_ref()
            .map(|c| (c.name.clone(), c.run.clone())),
        Some(("agent".into(), "run".into()))
    );
    assert!(walk(2, |_| Ok(own(0)), empty).is_err());
    assert!(walk(2, |_| Ok(own(0)), denied).is_err());
}

#[test]
fn a_foreign_parent_zero_is_still_refused() {
    // On macOS pid 0 is a foreign (root-owned) process, so the walk must
    // refuse a same-user parent of zero rather than treat it as a top.
    assert!(walk(
        2,
        |pid| Ok(if pid == 2 {
            own(0)
        } else {
            Inspection::Foreign
        }),
        empty
    )
    .is_err());
}

#[test]
fn unreadable_middle_unknown_parent_loop_and_cap_refuse() {
    assert!(walk(
        3,
        |pid| Ok(match pid {
            3 => own(2),
            2 => own(1),
            _ => Inspection::Foreign,
        }),
        denied
    )
    .is_err());
    assert!(walk(
        3,
        |pid| if pid == 3 {
            Ok(own(2))
        } else {
            Err(unreadable())
        },
        empty
    )
    .is_err());
    assert!(walk(3, |_| Ok(own(3)), empty).is_err());
    assert!(walk(3, |pid| Ok(own(pid + 1)), empty).is_err());
}

#[test]
fn exactly_sixty_four_processes_may_reach_the_verified_top() {
    let chain = walk(
        1,
        |pid| {
            Ok(if pid == 65 {
                Inspection::Foreign
            } else {
                own(pid + 1)
            })
        },
        empty,
    )
    .unwrap();
    assert!(walk(1, |pid| Ok(own(if pid == 64 { 0 } else { pid + 1 })), empty).is_err());
    assert_eq!(chain.len(), 64);
}

#[test]
fn a_replaced_process_during_environment_inspection_refuses() {
    let mut own_reads = 0;
    assert!(walk(
        2,
        |pid| {
            if pid == 1 {
                return Ok(Inspection::Foreign);
            }
            own_reads += 1;
            Ok(own(if own_reads == 1 { 1 } else { 3 }))
        },
        empty
    )
    .is_err());
}

#[test]
fn readable_partial_or_invalid_claims_do_not_disappear() {
    for bytes in [
        b"HGS_SESSION=agent\0".as_slice(),
        b"HGS_SESSION=\xff\0HGS_RUN_ID=run\0",
    ] {
        assert!(walk(
            2,
            |pid| Ok(if pid == 2 {
                own(1)
            } else {
                Inspection::Foreign
            }),
            |_| Ok(bytes.to_vec())
        )
        .is_err());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn effective_owner_not_real_owner_decides_foreign() {
    assert!(effective_owner("Uid:\t2000\t1000\t2000\t2000\n", 1000).unwrap());
    assert!(!effective_owner("Uid:\t1000\t2000\t1000\t1000\n", 1000).unwrap());
    assert!(effective_owner("Uid:\t1000\t1000\t1000\n", 1000).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn native_inspection_refuses_a_zombie_and_a_dead_process() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    struct Job(std::process::Child);
    impl Drop for Job {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let code="import os,sys; p=os.fork(); (os._exit(0) if p==0 else None); print(p,flush=True); sys.stdin.read(1); os.waitpid(p,0)";
    let mut job = Job(Command::new("python3")
        .arg("-c")
        .arg(code)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap());
    let mut text = String::new();
    BufReader::new(job.0.stdout.take().unwrap())
        .read_line(&mut text)
        .unwrap();
    let pid = text.trim().parse::<u32>().unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        if stat.rsplit_once(')').unwrap().1.split_whitespace().next() == Some("Z") {
            break;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(inspect(pid).is_err());
    job.0.stdin.take().unwrap().write_all(b"g").unwrap();
    assert!(job.0.wait().unwrap().success());
    assert!(inspect(u32::MAX).is_err());
}

#[test]
fn an_unavailable_native_birth_is_only_tolerated_for_an_unreadable_verified_top() {
    let table = |pid| {
        Ok(if pid == 2 {
            Inspection::Own(Process {
                parent: 1,
                birth: None,
            })
        } else {
            Inspection::Foreign
        })
    };
    assert!(walk(2, table, empty).is_err());
    assert!(walk(2, table, denied).is_ok());
}
