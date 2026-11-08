//! One read-only confirmation after the original Stop's not-running result.
//! Production callers exist only in the Windows native adapter. Linux tests
//! compile the same gate with observations, never COM or process authority.
use anyhow::{ensure, Result};
use std::time::Instant;

pub(super) struct Observation {
    pub before: i32,
    pub after: i32,
    pub matching: usize,
    pub original_exited: bool,
}

pub(super) fn confirm(
    stop_error: i32,
    until: Instant,
    fence: &mut impl FnMut() -> Result<()>,
    read: impl FnOnce() -> Result<Observation>,
) -> Result<()> {
    ensure!(
        stop_error == 0x8004130Bu32 as i32,
        "Stop was not confirmed."
    );
    ensure!(Instant::now() < until, "Original Stop deadline expired.");
    fence()?;
    let seen = read()?;
    ensure!(
        seen.before == 3 && seen.after == 3 && seen.matching == 0 && seen.original_exited,
        "Original Stop readback does not confirm a ready, empty task and original process exit."
    );
    fence()?;
    ensure!(Instant::now() < until, "Original Stop deadline expired.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Duration};
    const NOT_RUNNING: i32 = 0x8004130Bu32 as i32;
    fn until() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }
    fn ready() -> Observation {
        Observation {
            before: 3,
            after: 3,
            matching: 0,
            original_exited: true,
        }
    }
    #[test]
    fn only_the_stop_not_running_result_gets_one_fenced_readback() {
        let calls = Cell::new(0);
        let mut fences = 0;
        confirm(
            NOT_RUNNING,
            until(),
            &mut || {
                fences += 1;
                Ok(())
            },
            || {
                calls.set(calls.get() + 1);
                Ok(ready())
            },
        )
        .unwrap();
        assert_eq!((calls.get(), fences), (1, 2));
        for error in [0, -1, 0x80070005u32 as i32, NOT_RUNNING + 1] {
            assert!(confirm(
                error,
                until(),
                &mut || panic!("unrelated error"),
                || panic!("no readback")
            )
            .is_err());
        }
    }
    #[test]
    fn a_filtered_empty_list_never_overrides_state_or_kernel_exit() {
        for before in [-1, 0, 1, 2, 3, 4, 5] {
            for after in [-1, 0, 1, 2, 3, 4, 5] {
                for matching in [0, 1, 2] {
                    for original_exited in [false, true] {
                        let result = confirm(NOT_RUNNING, until(), &mut || Ok(()), || {
                            Ok(Observation {
                                before,
                                after,
                                matching,
                                original_exited,
                            })
                        });
                        assert_eq!(
                            result.is_ok(),
                            before == 3 && after == 3 && matching == 0 && original_exited
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn refusal_or_an_expired_original_budget_cannot_be_renewed() {
        assert!(confirm(
            NOT_RUNNING,
            Instant::now() - Duration::from_secs(1),
            &mut || panic!("expired"),
            || panic!("expired")
        )
        .is_err());
        assert!(confirm(
            NOT_RUNNING,
            until(),
            &mut || anyhow::bail!("caller changed"),
            || panic!("refused before read")
        )
        .is_err());
        assert!(
            confirm(NOT_RUNNING, until(), &mut || Ok(()), || anyhow::bail!(
                "definition, ACL or inventory unreadable"
            ))
            .is_err()
        );
        let mut fences = 0;
        assert!(confirm(
            NOT_RUNNING,
            until(),
            &mut || {
                fences += 1;
                ensure!(fences == 1, "authority changed after read");
                Ok(())
            },
            || Ok(ready())
        )
        .is_err());
        // Advance only after taking the original Instant, without a sleep.
        let end = Cell::new(Instant::now());
        let original = until();
        assert!(confirm(
            NOT_RUNNING,
            original,
            &mut || {
                ensure!(end.get() < original, "original budget spent");
                Ok(())
            },
            || {
                end.set(original);
                Ok(ready())
            }
        )
        .is_err());
    }
}
