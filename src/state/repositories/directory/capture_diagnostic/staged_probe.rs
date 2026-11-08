//! Separate disposable post-staging experiment. Never Captured/actor authority.
#![cfg(all(test, windows))]
use super::{Acquisition, Directory, Held, Inventory};
use crate::{
    platform,
    platform::persist_diagnostic::{handle_info, EntryDecision, HandleInfo, Object},
};
use serde::Serialize;
use std::{fs::File, io, time::Instant};

const HELD: usize = 32;
const ROWS: usize = 96; // reserves the existing observer's <=32 event rows
const STAGES: usize = 8;
const REPARSE: u32 = 0x400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Root,
    FactsDirectory,
    Ancestor,
    Fact,
    Writer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Entry,
    BeforeDrop,
    AfterDrop,
    AfterProducer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Privacy {
    Allowed,
    NotRequired,
    NotChecked,
    Denied,
    Unreadable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(in crate::state) struct Failure {
    stage: &'static str,
    code: Option<i32>,
}
impl Failure {
    fn new(stage: &'static str) -> Self {
        Self { stage, code: None }
    }
    fn query(stage: &'static str, error: &io::Error) -> Self {
        Self {
            stage,
            code: error.raw_os_error(),
        }
    }
    pub fn io(self) -> io::Error {
        io::Error::other(self.stage)
    }
}
type Result<T> = std::result::Result<T, Failure>;

// BEGIN PORTABLE STAGED POLICY. Depends only on metadata DTOs, not Windows I/O.
fn binding(before: &HandleInfo, actual: &HandleInfo) -> bool {
    before.handle == actual.handle
        && before.object == actual.object
        && before.granted_access == actual.granted_access
        && before.mode == actual.mode
        && before.directory == actual.directory
        && before.attributes & REPARSE == 0
        && actual.attributes & REPARSE == 0
}
fn evaluate(
    role: Role,
    before: &HandleInfo,
    staged: Option<&HandleInfo>,
    actual: &HandleInfo,
    privacy: Privacy,
    provenance: bool,
) -> Result<()> {
    if !provenance {
        return Err(Failure::new("acquisition_or_origin"));
    }
    if !binding(before, actual) {
        return Err(Failure::new("original_binding_or_reparse"));
    }
    let directory = matches!(role, Role::Root | Role::FactsDirectory | Role::Ancestor);
    if actual.directory != directory {
        return Err(Failure::new("object_kind"));
    }
    if role == Role::Ancestor {
        if privacy != Privacy::NotRequired {
            return Err(Failure::new("ancestor_guard_shape"));
        }
    } else if privacy != Privacy::Allowed {
        return Err(Failure::new("private_original_object"));
    }
    if role == Role::Root {
        // Only this NEW temporal contract takes one entry baseline for R.
        // The full old result is retained separately; it is never made true.
        if let Some(staged) = staged {
            if actual != staged {
                return Err(Failure::new("full_staged_root_metadata"));
            }
        }
    } else if actual != before {
        return Err(Failure::new("full_old_non_root_metadata"));
    }
    Ok(())
}
fn evaluate_checked(
    phase: Phase,
    role: Role,
    before: &HandleInfo,
    staged: Option<&HandleInfo>,
    actual: &HandleInfo,
    privacy: Privacy,
    provenance: bool,
) -> Result<()> {
    if phase != Phase::Entry && role == Role::Root && staged.is_none() {
        return Err(Failure::new("missing_staged_root_view"));
    }
    evaluate(role, before, staged, actual, privacy, provenance)
}
fn green(
    observer: bool,
    producer: bool,
    entry: bool,
    dropped: bool,
    post_drop: bool,
    final_check: bool,
    complete: bool,
) -> bool {
    observer && producer && entry && dropped && post_drop && final_check && complete
}
// Resource operations are supplied by the ORIGINAL owned context. This pure
// driver contains unwind and preserves a real drop when post-checks fail.
fn guarded_drop<C>(
    context: &mut C,
    until: Instant,
    pre: fn(&mut C, Instant) -> Result<()>,
    drop_r: fn(&mut C),
    post: fn(&mut C, Instant) -> Result<()>,
) -> Result<()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pre(context, until)?;
        if Instant::now() >= until {
            return Err(Failure::new("deadline_immediately_before_drop"));
        }
        drop_r(context);
        post(context, until)?;
        if Instant::now() >= until {
            return Err(Failure::new("deadline_after_retained_verification"));
        }
        Ok(())
    }))
    .unwrap_or_else(|_| Err(Failure::new("release_guard_panicked")))
}
// END PORTABLE STAGED POLICY

struct Node {
    held: Held,
    role: Role,
    index: usize,
}
#[derive(Clone, Debug, Serialize)]
struct Check {
    phase: Phase,
    index: usize,
    role: Role,
    actual: Option<HandleInfo>,
    old_changed: Vec<&'static str>,
    staged_changed: Vec<&'static str>,
    old_full_equal: Option<bool>,
    privacy: Privacy,
    error: Option<Failure>,
}
#[derive(Clone, Debug, Serialize)]
struct Stage {
    phase: Phase,
    expected: usize,
    checked: usize,
    not_checked: usize,
    ok: bool,
    error: Option<Failure>,
    deadline_live_after: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(in crate::state) struct Evidence {
    pre_creation: Vec<Inventory>,
    post_staging: Vec<Option<HandleInfo>>,
    checks: Vec<Check>,
    stages: Vec<Stage>,
    entry_attempted: bool,
    entry_complete: bool,
    release_attempted: bool,
    r_drop_performed: bool,
    post_drop_verified: bool,
    final_verified: bool,
    r_held: usize,
    retained_held: usize,
    overflow: bool,
    failure: Option<Failure>,
}
impl Evidence {
    pub fn complete(&self) -> bool {
        !self.overflow
            && self.entry_complete
            && self.failure.is_none()
            && self
                .stages
                .iter()
                .all(|s| s.ok && s.checked == s.expected && s.deadline_live_after)
    }
    pub fn positive(&self, observer: bool, producer: bool) -> bool {
        green(
            observer,
            producer,
            self.entry_complete,
            self.r_drop_performed,
            self.post_drop_verified,
            self.final_verified,
            self.complete(),
        )
    }
    pub fn physically_dropped(&self) -> bool {
        self.r_drop_performed
    }
    pub fn entry_complete(&self) -> bool {
        self.entry_complete
    }
}

pub(in crate::state) struct StagedProbe {
    roots: Vec<Node>,
    retained: Vec<Node>,
    evidence: Evidence,
    until: Option<Instant>,
}
impl StagedProbe {
    /// Only actual fields consumed immediately after Captured::verify. The
    /// fresh fixture uses the original open R and original created D; no
    /// caller label, path or boolean stands in for that acquisition chain.
    pub fn fresh(root: Directory, directory: Directory, fact: File, writer: File) -> Result<Self> {
        if !directory._ancestors.is_empty() {
            return Err(Failure::new("fresh_created_directory_required"));
        }
        let root_parts = root
            .into_capture_diagnostic_parts("staged-root", Acquisition::OpenedDirectory)
            .map_err(|e| Failure::query("consume_original_root", &e))?;
        let directory_parts = directory
            .into_capture_diagnostic_parts("staged-facts", Acquisition::CreatedDirectory)
            .map_err(|e| Failure::query("consume_original_facts_directory", &e))?;
        if root_parts.is_empty() || directory_parts.len() != 1 {
            return Err(Failure::new("original_directory_roster"));
        }
        let root_object = root_parts[0].inventory.native.object;
        let mut nodes = Vec::new();
        for (n, held) in root_parts.into_iter().enumerate() {
            nodes.push(Node {
                held,
                role: if n == 0 { Role::Root } else { Role::Ancestor },
                index: n,
            });
        }
        let index = nodes.len();
        nodes.push(Node {
            held: directory_parts.into_iter().next().unwrap(),
            role: Role::FactsDirectory,
            index,
        });
        for (role, file, origin) in [
            (Role::Fact, fact, "staged-original-fact"),
            (Role::Writer, writer, "staged-independent-writer"),
        ] {
            let held = Held::new(file, origin.into(), Acquisition::PrivateReader)
                .map_err(|e| Failure::query("consume_original_reader", &e))?;
            let index = nodes.len();
            nodes.push(Node { held, role, index });
        }
        if nodes.len() > HELD
            || nodes
                .iter()
                .filter(|n| n.held.inventory.native.object == root_object)
                .count()
                != 1
        {
            return Err(Failure::new("fresh_complete_root_roster"));
        }
        if !nodes.iter().all(Self::provenance) {
            return Err(Failure::new("unknown_original_acquisition"));
        }
        let pre_creation = nodes
            .iter()
            .map(|n| n.held.inventory.clone())
            .collect::<Vec<_>>();
        let post_staging = vec![None; nodes.len()];
        let (roots, retained) = nodes.into_iter().partition(|n| n.role == Role::Root);
        let mut this = Self {
            roots,
            retained,
            until: None,
            evidence: Evidence {
                pre_creation,
                post_staging,
                checks: vec![],
                stages: vec![],
                entry_attempted: false,
                entry_complete: false,
                release_attempted: false,
                r_drop_performed: false,
                post_drop_verified: false,
                final_verified: false,
                r_held: 1,
                retained_held: 0,
                overflow: false,
                failure: None,
            },
        };
        this.evidence.retained_held = this.retained.len();
        Ok(this)
    }
    fn provenance(node: &Node) -> bool {
        // Consistency check of an immutable acquisition receipt, not proof
        // manufactured from labels. Custody comes from the actual verified
        // Directory/File objects consumed by the closed fresh constructor.
        let expected = match node.role {
            Role::Root | Role::Ancestor => Acquisition::OpenedDirectory,
            Role::FactsDirectory => Acquisition::CreatedDirectory,
            Role::Fact | Role::Writer => Acquisition::PrivateReader,
        }
        .masks();
        let record = &node.held.inventory;
        !record.duplicated_original
            && record.requested_access == expected.0
            && record.requested_share == expected.1
            && record.acquisition == expected.2
    }
    fn privacy(node: &Node) -> (Privacy, Option<Failure>) {
        let result = match node.role {
            Role::Ancestor => return (Privacy::NotRequired, None),
            Role::Root | Role::FactsDirectory => {
                platform::verify_private_directory(&node.held.file)
            }
            Role::Fact | Role::Writer => platform::verify_private_file(&node.held.file),
        };
        match result {
            Ok(true) => (Privacy::Allowed, None),
            Ok(false) => (Privacy::Denied, None),
            Err(error) => (
                Privacy::Unreadable,
                Some(Failure::query("same_handle_privacy", &error)),
            ),
        }
    }
    fn clock(&self, until: Instant, stage: &'static str) -> Result<()> {
        if self.until != Some(until) {
            return Err(Failure::new("original_deadline_changed"));
        }
        if Instant::now() >= until {
            return Err(Failure::new(stage));
        }
        Ok(())
    }
    fn row_count(&self) -> usize {
        self.evidence.pre_creation.len()
            + self
                .evidence
                .post_staging
                .iter()
                .filter(|x| x.is_some())
                .count()
            + self.evidence.checks.len()
            + self.evidence.stages.len()
    }
    fn check(&mut self, phase: Phase, until: Instant, entry: bool) -> Result<()> {
        if self.evidence.stages.len() >= STAGES || self.row_count() + 1 >= ROWS {
            self.evidence.overflow = true;
            return Err(Failure::new("staged_report_bound"));
        }
        let expected = self.roots.len() + self.retained.len();
        let index = self.evidence.stages.len();
        self.evidence.stages.push(Stage {
            phase,
            expected,
            checked: 0,
            not_checked: expected,
            ok: false,
            error: None,
            deadline_live_after: false,
        });
        let result = (|| {
            self.clock(until, "deadline_before_inventory")?;
            for n in 0..expected {
                self.clock(until, "deadline_before_original_query")?;
                if self.row_count() + if entry { 2 } else { 1 } > ROWS {
                    self.evidence.overflow = true;
                    return Err(Failure::new("staged_report_bound"));
                }
                let node = if n < self.roots.len() {
                    &self.roots[n]
                } else {
                    &self.retained[n - self.roots.len()]
                };
                let actual = handle_info(&node.held.file);
                let metadata_error = actual
                    .as_ref()
                    .err()
                    .map(|e| Failure::query("same_handle_metadata", e));
                let (privacy, security_error) = if metadata_error.is_some() {
                    (Privacy::NotChecked, None)
                } else if Instant::now() >= until {
                    (
                        Privacy::NotChecked,
                        Some(Failure::new("deadline_after_metadata_before_privacy")),
                    )
                } else {
                    Self::privacy(node)
                };
                let staged = self.evidence.post_staging[node.index].as_ref();
                let verdict = if let Some(error) = metadata_error.or(security_error) {
                    Err(error)
                } else {
                    evaluate_checked(
                        phase,
                        node.role,
                        &node.held.inventory.native,
                        if entry { None } else { staged },
                        actual.as_ref().unwrap(),
                        privacy,
                        Self::provenance(node),
                    )
                };
                let after = actual.as_ref().ok().cloned();
                let check = Check {
                    phase,
                    index: node.index,
                    role: node.role,
                    old_changed: after
                        .as_ref()
                        .map(|a| super::differences(&node.held.inventory.native, a))
                        .unwrap_or_default(),
                    staged_changed: after
                        .as_ref()
                        .and_then(|a| staged.map(|s| super::differences(s, a)))
                        .unwrap_or_default(),
                    old_full_equal: after.as_ref().map(|a| *a == node.held.inventory.native),
                    actual: after.clone(),
                    privacy,
                    error: verdict.err(),
                };
                let stage = &mut self.evidence.stages[index];
                stage.checked += 1;
                stage.not_checked = expected - stage.checked;
                self.evidence.checks.push(check);
                if entry {
                    self.evidence.post_staging[node.index] = after;
                }
                verdict?;
                self.clock(until, "deadline_after_original_guards")?;
            }
            self.clock(until, "deadline_after_inventory")
        })();
        let stage = &mut self.evidence.stages[index];
        stage.ok = result.is_ok();
        stage.error = result.err();
        stage.deadline_live_after = Instant::now() < until;
        result
    }
    pub fn enter(&mut self, until: Instant) -> io::Result<EntryDecision> {
        if self.evidence.entry_attempted {
            return Err(Failure::new("entry_already_consumed").io());
        }
        self.evidence.entry_attempted = true;
        self.until = Some(until);
        let result = self.check(Phase::Entry, until, true);
        match result {
            Ok(())
                if self.evidence.post_staging.iter().all(Option::is_some)
                    && !self.evidence.overflow
                    && Instant::now() < until =>
            {
                self.evidence.entry_complete = true;
                Ok(EntryDecision::Complete)
            }
            Ok(()) => {
                self.evidence.failure = Some(Failure::new("entry_incomplete"));
                Ok(EntryDecision::Incomplete)
            }
            Err(error) => {
                self.evidence.failure = Some(error);
                Err(error.io())
            }
        }
    }
    pub fn release(&mut self, until: Instant) -> io::Result<()> {
        if self.evidence.release_attempted {
            return Err(Failure::new("release_already_attempted").io());
        }
        self.evidence.release_attempted = true;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.release_inner(until)))
                .unwrap_or_else(|_| Err(Failure::new("release_guard_panicked")));
        if let Err(error) = result {
            self.evidence.failure = Some(error);
            return Err(error.io());
        }
        Ok(())
    }
    fn release_inner(&mut self, until: Instant) -> Result<()> {
        if !self.evidence.entry_complete
            || self.evidence.overflow
            || self.evidence.failure.is_some()
            || self.roots.len() != 1
            || self.evidence.r_drop_performed
        {
            return Err(Failure::new("no_complete_staged_admission"));
        }
        guarded_drop(
            self,
            until,
            Self::before_drop,
            Self::drop_r,
            Self::after_drop,
        )
    }
    fn before_drop(&mut self, until: Instant) -> Result<()> {
        self.check(Phase::BeforeDrop, until, false)?;
        self.clock(until, "deadline_immediately_before_drop")
    }
    fn drop_r(&mut self) {
        self.roots.clear();
        // Before any later fallible check. A failed post-check cannot undo or
        // relabel the actual disposal of this complete original R cohort.
        self.evidence.r_drop_performed = true;
        self.evidence.r_held = 0;
    }
    fn after_drop(&mut self, until: Instant) -> Result<()> {
        self.check(Phase::AfterDrop, until, false)?;
        self.clock(until, "deadline_after_retained_verification")?;
        self.evidence.post_drop_verified = true;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        let result = match self.until {
            Some(until) => self.check(Phase::AfterProducer, until, false),
            None => Err(Failure::new("entry_not_reached")),
        };
        self.evidence.final_verified = result.is_ok();
        if let Err(error) = result {
            self.evidence.failure.get_or_insert(error);
        }
        result
    }
    pub fn evidence(&self) -> Evidence {
        self.evidence.clone()
    }
}

// Pure controls: no filesystem, handles or Windows/native calls. The metadata
// DTOs and BEGIN/END policy block are the exact portable extraction seam.
#[cfg(test)]
mod controls {
    use super::*;
    fn root() -> HandleInfo {
        HandleInfo {
            handle: 1,
            object: Object { volume: 2, file: 3 },
            length: 0,
            granted_access: 0x120089,
            mode: 0x20,
            attributes: 0x10,
            directory: true,
        }
    }
    #[test]
    fn old_full_oracle_stays_red_while_the_separate_entry_records_new_root_metadata() {
        let before = root();
        let mut after = before.clone();
        after.length = 4096;
        assert!(super::super::verify_value(&before, &after).is_err());
        assert_eq!(super::super::differences(&before, &after), ["length"]);
        assert!(evaluate(Role::Root, &before, None, &after, Privacy::Allowed, true).is_ok());
        assert!(evaluate(
            Role::Root,
            &before,
            Some(&after),
            &after,
            Privacy::Allowed,
            true
        )
        .is_ok());
    }
    #[test]
    fn every_post_entry_field_refuses_without_resampling() {
        type Change = (&'static str, fn(&mut HandleInfo));
        let changes: [Change; 8] = [
            ("handle", |x| x.handle += 1),
            ("volume", |x| x.object.volume += 1),
            ("file", |x| x.object.file += 1),
            ("length", |x| x.length += 1),
            ("access", |x| x.granted_access += 1),
            ("mode", |x| x.mode += 1),
            ("attributes", |x| x.attributes += 1),
            ("directory", |x| x.directory = false),
        ];
        let before = root();
        let mut staged = before.clone();
        staged.length = 4096;
        for (_, change) in changes {
            let mut actual = staged.clone();
            change(&mut actual);
            assert!(evaluate(
                Role::Root,
                &before,
                Some(&staged),
                &actual,
                Privacy::Allowed,
                true
            )
            .is_err());
            assert_eq!(staged.length, 4096); // the failed check does not advance the view
        }
    }
    #[test]
    fn every_non_root_object_keeps_the_old_full_oracle() {
        for role in [
            Role::FactsDirectory,
            Role::Ancestor,
            Role::Fact,
            Role::Writer,
        ] {
            let mut before = root();
            if matches!(role, Role::Fact | Role::Writer) {
                before.directory = false;
                before.attributes = 0x20;
            }
            let privacy = if role == Role::Ancestor {
                Privacy::NotRequired
            } else {
                Privacy::Allowed
            };
            let mut changed = before.clone();
            changed.length += 1;
            assert!(evaluate(role, &before, Some(&changed), &changed, privacy, true).is_err());
            assert!(evaluate(role, &before, Some(&changed), &before, privacy, true).is_ok());
        }
    }
    #[test]
    fn privacy_reparse_acquisition_and_original_binding_fail_closed() {
        let before = root();
        for privacy in [
            Privacy::Denied,
            Privacy::Unreadable,
            Privacy::NotRequired,
            Privacy::NotChecked,
        ] {
            assert!(evaluate(Role::Root, &before, None, &before, privacy, true).is_err());
        }
        assert!(evaluate(Role::Root, &before, None, &before, Privacy::Allowed, false).is_err());
        let mut reparse = before.clone();
        reparse.attributes |= REPARSE;
        assert!(evaluate(Role::Root, &before, None, &reparse, Privacy::Allowed, true).is_err());
        for change in [
            |x: &mut HandleInfo| x.handle += 1,
            |x: &mut HandleInfo| x.object.file += 1,
            |x: &mut HandleInfo| x.mode += 1,
            |x: &mut HandleInfo| x.granted_access += 1,
        ] {
            let mut actual = before.clone();
            change(&mut actual);
            assert!(evaluate(Role::Root, &before, None, &actual, Privacy::Allowed, true).is_err());
        }
        assert!(evaluate(
            Role::Ancestor,
            &before,
            None,
            &before,
            Privacy::NotRequired,
            true
        )
        .is_ok());
    }
    #[test]
    fn no_missing_guard_can_make_a_green_staged_witness() {
        assert!(green(true, true, true, true, true, true, true));
        for missing in 0..7 {
            let mut guards = [true; 7];
            guards[missing] = false;
            assert!(!green(
                guards[0], guards[1], guards[2], guards[3], guards[4], guards[5], guards[6]
            ));
        }
    }
    #[derive(Default)]
    struct Custody {
        dropped: bool,
        pre_error: bool,
        pre_panic: bool,
        post_error: bool,
        checks: usize,
    }
    fn pre(context: &mut Custody, _: Instant) -> Result<()> {
        context.checks += 1;
        if context.pre_panic {
            panic!("private pre-drop guard fixture");
        }
        if context.pre_error {
            return Err(Failure::new("pre_guard_fixture"));
        }
        Ok(())
    }
    fn drop_r(context: &mut Custody) {
        context.dropped = true;
    }
    fn post(context: &mut Custody, _: Instant) -> Result<()> {
        context.checks += 1;
        if context.post_error {
            return Err(Failure::new("post_guard_fixture"));
        }
        Ok(())
    }
    #[test]
    fn failed_or_panicking_pre_drop_checks_never_dispose_originals() {
        for panic in [false, true] {
            let mut context = Custody {
                pre_error: !panic,
                pre_panic: panic,
                ..Custody::default()
            };
            assert!(guarded_drop(
                &mut context,
                Instant::now() + std::time::Duration::from_secs(1),
                pre,
                drop_r,
                post
            )
            .is_err());
            assert!(!context.dropped);
            assert_eq!(context.checks, 1);
        }
    }
    #[test]
    fn a_post_drop_failure_keeps_the_physical_drop_truth() {
        let mut context = Custody {
            post_error: true,
            ..Custody::default()
        };
        assert!(guarded_drop(
            &mut context,
            Instant::now() + std::time::Duration::from_secs(1),
            pre,
            drop_r,
            post
        )
        .is_err());
        assert!(context.dropped);
        assert_eq!(context.checks, 2);
    }
    #[test]
    fn an_expired_original_budget_never_becomes_a_new_drop_budget() {
        let mut context = Custody::default();
        assert!(guarded_drop(
            &mut context,
            Instant::now() - std::time::Duration::from_secs(1),
            pre,
            drop_r,
            post
        )
        .is_err());
        assert!(!context.dropped);
    }

    #[test]
    fn a_missing_staged_root_snapshot_never_falls_back_to_entry_rules() {
        let before = root();
        assert!(evaluate_checked(
            Phase::BeforeDrop,
            Role::Root,
            &before,
            None,
            &before,
            Privacy::Allowed,
            true
        )
        .is_err());
        assert!(evaluate_checked(
            Phase::Entry,
            Role::Root,
            &before,
            None,
            &before,
            Privacy::Allowed,
            true
        )
        .is_ok());
    }
}
