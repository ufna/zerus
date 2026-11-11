use super::*;

fn own(parent: u32) -> Inspection {
    Inspection::Own(Process {
        parent,
        birth: "birth".into(),
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
fn readable_top_keeps_its_claim_and_a_no_parent_top_also_finishes() {
    let chain = walk(
        2,
        |_| Ok(own(0)),
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
    assert!(walk(2, |_| Ok(own(0)), denied).is_ok());
}

#[test]
fn unreadable_middle_unknown_parent_loop_and_cap_refuse() {
    assert!(walk(3, |pid| Ok(if pid == 3 { own(2) } else { own(0) }), denied).is_err());
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
    let chain = walk(1, |pid| Ok(own(if pid == 64 { 0 } else { pid + 1 })), empty).unwrap();
    assert_eq!(chain.len(), 64);
}

#[test]
fn a_replaced_process_during_environment_inspection_refuses() {
    let mut calls = 0;
    assert!(walk(
        1,
        |_| {
            calls += 1;
            Ok(own(if calls == 1 { 0 } else { 2 }))
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
        assert!(walk(2, |_| Ok(own(0)), |_| Ok(bytes.to_vec())).is_err());
    }
}
