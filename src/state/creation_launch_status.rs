//! Dormant, bounded original launch observation, before generic CLI bootstrap.
//! No producer, listing, repair, writer, process, transport or current authority.
use crate::cli::{Error, Result};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::Write;

const COMMAND: &str = "creation-launch-status";
const CAPABILITY: &str = "creation-launch-status-v1";
#[cfg(any(target_os = "linux", target_os = "macos"))]
const UNAVAILABLE: &str =
    "The original launch observation is unavailable; review the original operation.";
const INVALID: &str = "Use --expect-machine M creation-launch-status --launch-id ID --json.";

fn at(args: &[String], index: usize, word: &str) -> bool {
    args.get(index).is_some_and(|arg| arg == word)
}
// Only command positions select this entry. An unrelated native argv or
// session tag containing the reserved word must retain ordinary routing.
fn reserved_by(at: impl Fn(usize, &str) -> bool, offset: usize) -> bool {
    at(offset, COMMAND)
        || (at(offset, "--help") && at(offset + 1, CAPABILITY))
        || (at(offset, "--expect-machine")
            && (at(offset + 1, COMMAND)
                || at(offset + 2, COMMAND)
                || (at(offset + 2, "--client")
                    && ((at(offset + 3, COMMAND) && at(offset + 4, "--launch-id"))
                        || at(offset + 4, COMMAND)))))
}
fn reserved(args: &[String], offset: usize) -> bool {
    reserved_by(|index, word| at(args, index, word), offset)
}
#[derive(Default)]
struct Argument {
    keyword: Option<&'static str>,
    remote: bool,
    text: Option<String>,
}
fn remote_refusal() -> Error {
    Error::new(
        2,
        "This observation requires a bounded target-side invocation.",
    )
}
// This bounds only our retained probe, NOT args_os construction, the runtime,
// loader or native process-argv storage. It is no original-scope witness.
fn os_arguments(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<Result<Vec<String>>> {
    let mut slots: [Argument; 10] = std::array::from_fn(|_| Argument::default());
    let mut count = 0;
    let mut retained = 0;
    let mut invalid = false;
    // argv0 is skipped as OS data, without Unicode conversion or identity use.
    for argument in args.into_iter().skip(1).take(10) {
        let slot = &mut slots[count];
        slot.keyword = [
            COMMAND,
            CAPABILITY,
            "--help",
            "--expect-machine",
            "--client",
            "--launch-id",
        ]
        .into_iter()
        .find(|word| argument == std::ffi::OsStr::new(word));
        slot.remote = argument.as_encoded_bytes().first() == Some(&b'@');
        let bytes = argument.as_encoded_bytes().len();
        if bytes <= 256 && bytes <= 1024 - retained {
            if let Some(text) = argument.to_str() {
                // Check both caps BEFORE making any retained String copy.
                slot.text = Some(text.to_owned());
                retained += bytes;
            } else {
                invalid = true;
            }
        } else {
            invalid = true;
        }
        count += 1;
    }
    let at = |index: usize, word: &str| {
        slots
            .get(index)
            .is_some_and(|slot| slot.keyword == Some(word))
    };
    let remote = slots[0].remote && reserved_by(at, 1);
    if !remote && !reserved_by(at, 0) {
        return None;
    }
    if remote {
        return Some(Err(remote_refusal()));
    }
    if invalid || count == slots.len() {
        return Some(Err(Error::new(2, INVALID)));
    }
    Some(Ok(slots
        .into_iter()
        .take(count)
        .map(|slot| slot.text.unwrap())
        .collect()))
}
/// First main entry, before existing Unicode argument conversion/preparation.
pub(crate) fn early_os(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<Result<i32>> {
    let selected = os_arguments(args)?;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = selected;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Some(Err(unsupported()));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Some(selected.and_then(|args| {
        // Exactly the same parser/observation, once; never generic fallback.
        early(&args).unwrap_or_else(|| Err(Error::new(2, INVALID)))
    }))
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn unsupported() -> Error {
    Error::new(
        2,
        "Original launch observation is unsupported on this platform.",
    )
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn canonical(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Request {
    machine: String,
    launch: String,
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn request(args: &[String]) -> Option<Request> {
    let (machine, launch) = match args {
        [guard, machine, command, flag, launch, json]
            if guard == "--expect-machine"
                && command == COMMAND
                && flag == "--launch-id"
                && json == "--json" =>
        {
            (machine, launch)
        }
        [guard, machine, client, origin, command, flag, launch, json]
            if guard == "--expect-machine"
                && client == "--client"
                && !origin.is_empty()
                && origin.len() <= 80
                && !origin.chars().any(|c| c.is_control() || c.is_whitespace())
                && command == COMMAND
                && flag == "--launch-id"
                && json == "--json" =>
        {
            (machine, launch)
        }
        _ => return None,
    };
    (canonical(machine) && canonical(launch)).then(|| Request {
        machine: machine.clone(),
        launch: launch.clone(),
    })
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fail() -> Error {
    Error::new(1, UNAVAILABLE)
}
/// Reserved forms return before every generic loader and environment mutation.
pub(crate) fn early(args: &[String]) -> Option<Result<i32>> {
    let remote = args.first().is_some_and(|arg| arg.starts_with('@')) && reserved(args, 1);
    if !remote && !reserved(args, 0) {
        return None;
    }
    if remote {
        return Some(Err(remote_refusal()));
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Some(Err(unsupported()))
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Some((|| {
        if matches!(args, [help, capability] if help == "--help" && capability == CAPABILITY) {
            std::io::stdout()
                .lock()
                .write_all(b"creation-launch-status-v1\n")
                .map_err(|_| fail())?;
            return Ok(0);
        }
        let request = request(args).ok_or_else(|| Error::new(2, INVALID))?;
        let reply = unix::observe(&request).map_err(|_| fail())?;
        let bytes = unix::encode(&reply).map_err(|_| fail())?;
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|_| fail())?;
        Ok(0)
    })())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
    use serde::Serialize;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::fs::{self, File, Metadata};
    use std::io::{self, Read};
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    };
    use std::path::{Component, Path, PathBuf};

    const RAW: usize = 1 << 20;
    // Conservative reservations, not measured syscall counts. Exact1.99.1 can
    // perform statx + availability probe + legacy stat, and audited libc statx
    // can add fstatat on ENOSYS. Further target paths withhold qualification.
    const META_COST: usize = 5;
    const SMALL: usize = 8 << 10;
    const ENCODED: usize = 16 << 10;
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Fault;
    type Checked<T> = std::result::Result<T, Fault>;

    // AW: private source overlay only; no build, execution, or sink binding.
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    mod accounting {
        use std::marker::PhantomData;

        const CAPACITY: usize = 16_384;
        const READY: u32 = 2;
        const REQUEST: u32 = 1;
        const REFUSED: u32 = 3;
        const WALK_UPDATED: u32 = 4;
        const DONE: u32 = 5;
        const ERROR: u32 = 6;
        const WAIT_CONSUME: u32 = 7;
        const CHILD_OK: u32 = 8;
        const CHILD_ERROR: u32 = 9;
        const CHILD_REFUSED: u32 = 10;
        const CHILD_SCALAR: u32 = 11;
        const NORMAL_FROZEN: u32 = 1 << 31;
        const NOMINATION: u32 = 31;

        #[derive(Clone, Copy)]
        #[repr(C)]
        struct Event {
            sequence: u64,
            ordinal: u64,
            reservation: u64,
            operation: u64,
            site: u32,
            kind: u16,
            flags: u16,
            counts: [u32; 3],
            value: i32,
            auxiliary: u64,
        }
        impl Event {
            const ZERO: Self = Self {
                sequence: 0,
                ordinal: 0,
                reservation: 0,
                operation: 0,
                site: 0,
                kind: 0,
                flags: 0,
                counts: [0; 3],
                value: 0,
                auxiliary: 0,
            };
            fn encode(&self) -> [u8; 64] {
                let mut b = [0; 64];
                b[0..8].copy_from_slice(&self.sequence.to_le_bytes());
                b[8..16].copy_from_slice(&self.ordinal.to_le_bytes());
                b[16..24].copy_from_slice(&self.reservation.to_le_bytes());
                b[24..32].copy_from_slice(&self.operation.to_le_bytes());
                b[32..36].copy_from_slice(&self.site.to_le_bytes());
                b[36..38].copy_from_slice(&self.kind.to_le_bytes());
                b[38..40].copy_from_slice(&self.flags.to_le_bytes());
                for (i, c) in self.counts.iter().enumerate() {
                    b[40 + 4 * i..44 + 4 * i].copy_from_slice(&c.to_le_bytes());
                }
                b[52..56].copy_from_slice(&self.value.to_le_bytes());
                b[56..64].copy_from_slice(&self.auxiliary.to_le_bytes());
                b
            }
        }
        #[derive(Clone, Copy)]
        #[repr(C)]
        struct Frame {
            id: u64,
            tag: u32,
            zero: u32,
            a: u64,
            b: u64,
        }
        impl Frame {
            const ZERO: Self = Self {
                id: 0,
                tag: 0,
                zero: 0,
                a: 0,
                b: 0,
            };
            fn site(&self) -> u32 {
                self.tag & 255
            }
            fn phase(&self) -> u32 {
                self.tag >> 8
            }
            fn phase_to(&mut self, phase: u32) {
                self.tag = self.site() | (phase << 8);
            }
            fn low(&self) -> u32 {
                self.b as u32
            }
            fn high(&self) -> u32 {
                (self.b >> 32) as u32
            }
        }
        #[derive(Clone, Copy)]
        #[repr(C)]
        struct Globals {
            cursor: u32,
            depth: u32,
            next_r: u64,
            next_a: u64,
            counts: [u32; 3],
            fault: u32,
            frozen_n: u32,
            status: u32,
        }
        #[repr(C)]
        struct State {
            frames: [Frame; 8],
            g: Globals,
        }
        #[repr(C)]
        struct Delta {
            replacements: [Frame; 2],
            indices: [u32; 2],
            used: u32,
            zero: u32,
            g: Globals,
        }
        impl Delta {
            fn edit<'a>(&'a mut self, state: &State, index: usize) -> Option<&'a mut Frame> {
                if index >= 8 {
                    return None;
                }
                for i in 0..self.used as usize {
                    if self.indices[i] as usize == index {
                        return self.replacements.get_mut(i);
                    }
                }
                if self.used >= 2 {
                    return None;
                }
                let i = self.used as usize;
                self.indices[i] = u32::try_from(index).ok()?;
                self.replacements[i] = *state.frames.get(index)?;
                self.used += 1;
                self.replacements.get_mut(i)
            }
            fn push(&mut self, state: &State, frame: Frame) -> Option<()> {
                let i = usize::try_from(self.g.depth).ok()?;
                *self.edit(state, i)? = frame;
                self.g.depth = self.g.depth.checked_add(1)?;
                Some(())
            }
            fn pop(&mut self, state: &State) -> Option<()> {
                self.g.depth = self.g.depth.checked_sub(1)?;
                *self.edit(state, self.g.depth as usize)? = Frame::ZERO;
                Some(())
            }
        }
        // Compile-time assertions are source only: none has been compiled.
        const _: [(); 64] = [(); std::mem::size_of::<Event>()];
        const _: [(); 32] = [(); std::mem::size_of::<Frame>()];
        const _: [(); 48] = [(); std::mem::size_of::<Globals>()];
        const _: [(); 304] = [(); std::mem::size_of::<State>()];
        const _: [(); 128] = [(); std::mem::size_of::<Delta>()];

        #[derive(Clone, Copy, PartialEq, Eq)]
        pub(super) enum Class {
            Instrumented = 1,
            Scalar = 2,
        }
        // Fixed prospective fixture labels are borrowed, never a decoded permit.
        pub(super) struct Context {
            pub(super) ordinal: u64,
            pub(super) class: Class,
        }
        pub(super) struct Recorder {
            events: Box<[Event; CAPACITY]>,
            context: &'static Context,
            state: State,
            _invoking_thread: PhantomData<*mut ()>,
        }
        const _: [(); 320] = [(); std::mem::size_of::<Recorder>()];
        const _: [(); 320] = [(); std::mem::size_of::<Option<Recorder>>()];
        const _: [(); 512] = [(); std::mem::size_of::<Option<Recorder>>()
            + std::mem::size_of::<Delta>()
            + std::mem::size_of::<Event>()];
        impl Recorder {
            pub(super) fn new(context: &'static Context) -> Option<Self> {
                if context.ordinal == 0 {
                    return None;
                }
                Some(Self {
                    events: Box::new([Event::ZERO; CAPACITY]),
                    context,
                    state: State {
                        frames: [Frame::ZERO; 8],
                        g: Globals {
                            cursor: 0,
                            depth: 0,
                            next_r: 1,
                            next_a: 1,
                            counts: [0; 3],
                            fault: 0,
                            frozen_n: 0,
                            status: 0,
                        },
                    },
                    _invoking_thread: PhantomData,
                })
            }
            pub(super) fn fault(&mut self, code: u32) {
                if self.state.g.fault == 0 {
                    self.state.g.fault = if (1..=8).contains(&code) { code } else { 4 };
                    self.state.g.frozen_n = self.state.g.cursor;
                }
            }
            pub(super) fn nominate(&mut self, site: u32) {
                if self.state.g.fault != 0 {
                    return;
                }
                if !(2..=7).contains(&site) || self.state.g.status != 0 {
                    self.fault(3);
                    return;
                }
                self.state.g.status = site;
            }
            pub(super) fn nominated(&self) -> u32 {
                self.state.g.status & NOMINATION
            }
            pub(super) fn source_ready(&self) -> bool {
                self.context.class == Class::Instrumented
                    && self.state.g.cursor == 0
                    && self.state.g.depth == 0
                    && self.state.g.fault == 0
                    && self.state.g.status == 0
                    && self.state.g.next_r == 1
                    && self.state.g.next_a == 1
                    && self.state.g.counts == [0; 3]
            }
            pub(super) fn clear_nomination(&mut self) {
                if self.state.g.fault == 0 {
                    self.state.g.status &= !NOMINATION;
                }
            }
            pub(super) fn charge_request(&mut self, site: u32, cost: usize, c: [usize; 3]) {
                let flags = match site {
                    1 => 1,
                    2 | 3 => 2,
                    4 | 5 => 3,
                    6 | 7 => 4,
                    _ => {
                        self.fault(4);
                        return;
                    }
                };
                let parent = if site == 2 {
                    self.state.frames[0].id
                } else {
                    0
                };
                self.emit(site, 1, flags, cost, parent, c);
            }
            pub(super) fn charge_outcome(
                &mut self,
                site: u32,
                cost: usize,
                accepted: bool,
                c: [usize; 3],
            ) {
                let flags = match site {
                    1 => 1,
                    2 | 3 => 2,
                    4 | 5 => 3,
                    6 | 7 => 4,
                    _ => {
                        self.fault(4);
                        return;
                    }
                };
                let parent = if site == 2 {
                    self.state.frames[0].id
                } else {
                    0
                };
                self.emit(site, if accepted { 2 } else { 3 }, flags, cost, parent, c);
                if !accepted {
                    self.finish(site, 3, c, None);
                    if site == 2 {
                        self.fault(3);
                    } // U01: never fabricate parent outcome.
                }
            }
            pub(super) fn walk_increment(&mut self, c: [usize; 3]) {
                let child = self.state.frames[0].a;
                let cost = self.state.frames[0].low() as usize;
                self.emit(1, 4, 1, 1, child, c);
                self.emit(1, 2, 1, cost, 0, c);
            }
            pub(super) fn api_begin(&mut self, site: u32, q: usize, c: [usize; 3]) {
                let flags = match site {
                    8..=10 => 2,
                    11 | 12 => 3,
                    13 | 14 => 4,
                    16 => 5,
                    _ => {
                        self.fault(4);
                        return;
                    }
                };
                let Ok(q) = u64::try_from(q) else {
                    self.fault(2);
                    return;
                };
                self.emit(site, 5, flags, 0, q, c);
            }
            pub(super) fn api_return(&mut self, site: u32, class: usize, n: u64, c: [usize; 3]) {
                let flags = match site {
                    8..=10 => 2,
                    11 | 12 => 3,
                    13 | 14 => 4,
                    16 => 5,
                    _ => {
                        self.fault(4);
                        return;
                    }
                };
                self.emit(site, 6, flags, class, n, c);
                let result = if class == 2 { 2 } else { 1 };
                match site {
                    8 | 9 if class == 2 => {
                        self.finish(2, 2, c, None);
                        self.finish(1, 2, c, None);
                    }
                    10 => self.finish(3, result, c, None),
                    11..=14 => self.finish(site - 7, result, c, None),
                    _ => (),
                }
            }
            pub(super) fn byte_check(
                &mut self,
                length: usize,
                remaining: usize,
                rejected: bool,
                c: [usize; 3],
            ) {
                let Ok(r) = u64::try_from(remaining) else {
                    self.fault(2);
                    return;
                };
                self.emit(15, 7, if rejected { 6 } else { 5 }, length, r, c);
                if rejected {
                    self.finish(15, 3, c, Some(remaining));
                }
            }
            pub(super) fn consumed(&mut self, n: usize, remaining: usize, c: [usize; 3]) {
                let Ok(r) = u64::try_from(remaining) else {
                    self.fault(2);
                    return;
                };
                self.emit(17, 8, 5, n, r, c);
            }
            pub(super) fn invocation_return(&mut self, ok: bool, c: [usize; 3]) {
                self.emit(18, 9, 0, if ok { 1 } else { 2 }, 0, c);
            }
            pub(super) fn seal_scalar(&mut self) {
                if self.state.g.fault != 0 {
                    return;
                }
                if self.context.class != Class::Scalar
                    || self.state.g.depth != 0
                    || self.state.g.status != 0
                {
                    self.fault(3);
                    return;
                }
                // No event or original-return fact is invented for class2.
                self.state.g.frozen_n = self.state.g.cursor;
                self.state.g.status = NORMAL_FROZEN;
            }
            fn emit(
                &mut self,
                site: u32,
                kind: u16,
                flags: u16,
                value: usize,
                auxiliary: u64,
                actual: [usize; 3],
            ) {
                if self.state.g.fault != 0 {
                    return;
                }
                if self.state.g.status & NORMAL_FROZEN != 0 {
                    self.fault(3);
                    return;
                }
                if self.state.g.cursor as usize >= CAPACITY {
                    self.fault(1);
                    return;
                }
                if !(1..=18).contains(&site) || !(1..=10).contains(&kind) || flags > 6 {
                    self.fault(4);
                    return;
                }
                let (Ok(w), Ok(o), Ok(m), Ok(value)) = (
                    u32::try_from(actual[0]),
                    u32::try_from(actual[1]),
                    u32::try_from(actual[2]),
                    i32::try_from(value),
                ) else {
                    self.fault(2);
                    return;
                };
                if w > 16 || o > 1024 || m > 4096 {
                    self.fault(2);
                    return;
                }
                let mut event = Event {
                    sequence: u64::from(self.state.g.cursor),
                    ordinal: self.context.ordinal,
                    counts: [w, o, m],
                    site,
                    kind,
                    flags,
                    value,
                    auxiliary,
                    ..Event::ZERO
                };
                let mut delta = Delta {
                    replacements: [Frame::ZERO; 2],
                    indices: [0; 2],
                    used: 0,
                    zero: 0,
                    g: self.state.g,
                };
                if self.plan(&mut event, &mut delta).is_none() {
                    self.fault(if site > 18 || site == 0 { 4 } else { 3 });
                    return;
                }
                // Validate every index and cursor increment BEFORE any commit mutation.
                let Some(next) = self.state.g.cursor.checked_add(1) else {
                    self.fault(2);
                    return;
                };
                if delta.used > 2
                    || delta.g.depth > 8
                    || delta.indices[..delta.used as usize].iter().any(|i| *i >= 8)
                {
                    self.fault(3);
                    return;
                }
                let index = self.state.g.cursor as usize;
                self.events[index] = event;
                for i in 0..delta.used as usize {
                    self.state.frames[delta.indices[i] as usize] = delta.replacements[i];
                }
                self.state.g = delta.g;
                self.state.g.cursor = next; // LAST; not hardware/crash atomicity.
            }
            fn plan(&self, e: &mut Event, d: &mut Delta) -> Option<()> {
                let depth = usize::try_from(d.g.depth).ok()?;
                if depth > 8 {
                    return None;
                }
                let same = e.counts == d.g.counts;
                match (e.site, e.kind, e.flags) {
                    (1, 1, 1) | (2 | 3, 1, 2) | (4 | 5, 1, 3) | (6 | 7, 1, 4) | (15, 7, 5 | 6) => {
                        if !same || e.value < 0 {
                            return None;
                        }
                        let cost = u32::try_from(e.value).ok()?;
                        let parent = if e.site == 2 {
                            if depth != 1 {
                                return None;
                            }
                            let f = self.state.frames.first()?;
                            if f.site() != 1
                                || f.phase() != REQUEST
                                || f.low() != cost
                                || cost > 33
                                || d.g.counts[0] >= 16
                                || e.auxiliary != f.id
                            {
                                return None;
                            }
                            f.id
                        } else {
                            if depth != 0 {
                                return None;
                            }
                            0
                        };
                        match e.site {
                            1 if cost == 0
                                || cost
                                    > if self.context.class == Class::Scalar {
                                        34
                                    } else {
                                        33
                                    } =>
                            {
                                return None;
                            }
                            3 if cost != 1 => return None,
                            4 | 5 if cost != 5 => return None,
                            6 | 7 if cost != 6 => return None,
                            15 if e.auxiliary > 1_048_576
                                || (e.flags == 5) != (u64::from(cost) <= e.auxiliary) =>
                            {
                                return None;
                            }
                            _ => (),
                        }
                        if e.site != 15 && e.auxiliary != parent {
                            return None;
                        }
                        e.reservation = d.g.next_r;
                        d.g.next_r = d.g.next_r.checked_add(1)?;
                        if e.site == 2 {
                            d.edit(&self.state, 0)?.a = e.reservation;
                        }
                        let frame = if e.site == 15 {
                            Frame {
                                id: e.reservation,
                                tag: 15 | ((if e.flags == 5 { READY } else { REFUSED }) << 8),
                                zero: 0,
                                a: u64::from(cost) | (e.auxiliary << 32),
                                b: 0,
                            }
                        } else {
                            Frame {
                                id: e.reservation,
                                tag: e.site | (REQUEST << 8),
                                zero: 0,
                                a: parent,
                                b: u64::from(cost),
                            }
                        };
                        d.push(&self.state, frame)?;
                    }
                    (1, 4, 1) => {
                        if depth != 2 || e.value != 1 {
                            return None;
                        }
                        let parent = self.state.frames.first()?;
                        let child = self.state.frames.get(1)?;
                        if parent.site() != 1
                            || parent.phase() != REQUEST
                            || child.site() != 2
                            || child.phase() != READY
                            || parent.a != child.id
                            || e.auxiliary != child.id
                        {
                            return None;
                        }
                        let mut expected = d.g.counts;
                        expected[0] = expected[0].checked_add(1)?;
                        if e.counts != expected {
                            return None;
                        }
                        e.reservation = parent.id;
                        d.edit(&self.state, 0)?.phase_to(WALK_UPDATED);
                    }
                    (1, 2 | 3, 1) | (2 | 3, 2 | 3, 2) | (4 | 5, 2 | 3, 3) | (6 | 7, 2 | 3, 4) => {
                        let index = if e.site == 1 {
                            0
                        } else {
                            depth.checked_sub(1)?
                        };
                        let f = self.state.frames.get(index)?;
                        if f.site() != e.site
                            || e.value < 0
                            || u32::try_from(e.value).ok()? != f.low()
                        {
                            return None;
                        }
                        e.reservation = f.id;
                        if e.auxiliary != if e.site == 2 { f.a } else { 0 } {
                            return None;
                        }
                        if e.site == 1 {
                            if !same {
                                return None;
                            }
                            if e.kind == 2 {
                                if f.phase() != WALK_UPDATED || depth != 2 {
                                    return None;
                                }
                            } else if !((f.phase() == REQUEST
                                && (f.low() > 33 || d.g.counts[0] >= 16))
                                || f.phase() == CHILD_REFUSED)
                            {
                                return None;
                            }
                        } else {
                            if f.phase() != REQUEST {
                                return None;
                            }
                            let axis = if e.site <= 3 { 1 } else { 2 };
                            let ceiling = if axis == 1 { 1024u32 } else { 4096 };
                            let fits = f.low() <= ceiling.checked_sub(d.g.counts[axis])?;
                            if (e.kind == 2) != fits {
                                return None;
                            }
                            let mut expected = d.g.counts;
                            if fits {
                                expected[axis] = expected[axis].checked_add(f.low())?;
                            }
                            if e.counts != expected {
                                return None;
                            }
                        }
                        d.edit(&self.state, index)?.phase_to(if e.kind == 2 {
                            READY
                        } else {
                            REFUSED
                        });
                    }
                    (8 | 9 | 10, 5, 2) | (11 | 12, 5, 3) | (13 | 14, 5, 4) | (16, 5, 5) => {
                        if !same || e.value != 0 {
                            return None;
                        }
                        let index = depth.checked_sub(1)?;
                        let f = self.state.frames.get(index)?;
                        let group = match e.site {
                            8 | 9 => 2,
                            10 => 3,
                            11 => 4,
                            12 => 5,
                            13 => 6,
                            14 => 7,
                            16 => 15,
                            _ => return None,
                        };
                        if f.site() != group || f.phase() != READY {
                            return None;
                        }
                        if group == 2 {
                            if depth != 2
                                || self.state.frames[0].phase() != READY
                                || (e.site == 8) != (f.high() == 0)
                                || f.high() >= f.low()
                            {
                                return None;
                            }
                        } else if group != 15 && f.high() != 0 {
                            return None;
                        }
                        if group == 15 {
                            let l = f.a as u32;
                            let t = f.low();
                            let q = 8192u32.min(l.checked_sub(t)?.checked_add(1)?);
                            if e.auxiliary != u64::from(q) {
                                return None;
                            }
                        } else if e.auxiliary != 0 {
                            return None;
                        }
                        e.reservation = f.id;
                        e.operation = d.g.next_a;
                        d.g.next_a = d.g.next_a.checked_add(1)?;
                        d.push(
                            &self.state,
                            Frame {
                                id: e.operation,
                                tag: e.site | (REQUEST << 8),
                                zero: 0,
                                a: f.id,
                                b: e.auxiliary,
                            },
                        )?;
                    }
                    (8 | 9 | 10, 6, 2) | (11 | 12, 6, 3) | (13 | 14, 6, 4) | (16, 6, 5) => {
                        if !same || depth < 2 {
                            return None;
                        }
                        let oi = depth - 1;
                        let gi = depth - 2;
                        let op = self.state.frames.get(oi)?;
                        let group = self.state.frames.get(gi)?;
                        if op.site() != e.site || op.phase() != REQUEST || op.a != group.id {
                            return None;
                        }
                        e.reservation = group.id;
                        e.operation = op.id;
                        if e.site == 16 {
                            if group.site() != 15 || group.phase() != READY {
                                return None;
                            }
                            let n = u32::try_from(e.auxiliary).ok()?;
                            let t = group.low();
                            let l = group.a as u32;
                            if !matches!(e.value, 1 | 2 | 4)
                                || (e.value == 1 && (n == 0 || u64::from(n) > op.b))
                                || (e.value != 1 && n != 0)
                            {
                                return None;
                            }
                            let k = group.high().checked_add(1)?;
                            {
                                let f = d.edit(&self.state, gi)?;
                                f.b = u64::from(t) | (u64::from(k) << 32);
                                f.phase_to(if e.value == 1 && n <= l.checked_sub(t)? {
                                    WAIT_CONSUME
                                } else if e.value == 4 && t == l {
                                    DONE
                                } else {
                                    ERROR
                                });
                            }
                            if e.value == 1 && n <= l.checked_sub(t)? {
                                let f = d.edit(&self.state, oi)?;
                                f.phase_to(WAIT_CONSUME);
                                f.b = u64::from(n);
                            } else {
                                d.pop(&self.state)?;
                            }
                        } else {
                            if e.site <= 12 {
                                if !matches!(e.value, 1 | 2)
                                    || (e.value == 2 && e.auxiliary != 0)
                                    || (e.site <= 10 && e.auxiliary > i32::MAX as u64)
                                    || (e.site > 10 && e.auxiliary != 0)
                                {
                                    return None;
                                }
                            } else if !matches!(e.value, 2 | 5 | 6) || e.auxiliary != 0 {
                                return None;
                            }
                            let k = group.high().checked_add(1)?;
                            {
                                let f = d.edit(&self.state, gi)?;
                                f.b = u64::from(f.low()) | (u64::from(k) << 32);
                                f.phase_to(if e.value == 2 {
                                    ERROR
                                } else if f.site() == 2 {
                                    READY
                                } else {
                                    DONE
                                });
                            }
                            d.pop(&self.state)?;
                        }
                    }
                    (17, 8, 5) => {
                        if !same || depth != 2 {
                            return None;
                        }
                        let op = self.state.frames.get(1)?;
                        let span = self.state.frames.first()?;
                        if span.site() != 15
                            || span.phase() != WAIT_CONSUME
                            || op.site() != 16
                            || op.phase() != WAIT_CONSUME
                            || op.a != span.id
                            || e.value <= 0
                            || u64::try_from(e.value).ok()? != op.b
                        {
                            return None;
                        }
                        let n = u32::try_from(e.value).ok()?;
                        let t = span.low().checked_add(n)?;
                        let l = span.a as u32;
                        let r0 = (span.a >> 32) as u32;
                        if t > l || e.auxiliary != u64::from(r0.checked_sub(t)?) {
                            return None;
                        }
                        e.reservation = span.id;
                        e.operation = op.id;
                        {
                            let f = d.edit(&self.state, 0)?;
                            f.b = u64::from(t) | (u64::from(f.high()) << 32);
                            f.phase_to(READY);
                        }
                        d.pop(&self.state)?;
                    }
                    (1, 10, 1) | (2 | 3, 10, 2) | (4 | 5, 10, 3) | (6 | 7, 10, 4) | (15, 10, 5) => {
                        if !same {
                            return None;
                        }
                        let index = depth.checked_sub(1)?;
                        let f = self.state.frames.get(index)?;
                        if f.site() != e.site {
                            return None;
                        }
                        let scalar = e.value == 4 && self.context.class == Class::Scalar;
                        let result = if e.site == 1 {
                            match f.phase() {
                                CHILD_OK => 1,
                                CHILD_ERROR => 2,
                                REFUSED | CHILD_REFUSED => 3,
                                CHILD_SCALAR => 4,
                                _ => return None,
                            }
                        } else if f.phase() == REFUSED {
                            3
                        } else if scalar && f.phase() == READY && f.high() == 0 {
                            4
                        } else if f.phase() == DONE
                            || (e.site == 2 && f.phase() == READY && f.high() == f.low())
                        {
                            1
                        } else if f.phase() == ERROR {
                            2
                        } else {
                            return None;
                        };
                        if e.value != result || (result == 4 && !scalar) {
                            return None;
                        }
                        let aux = if e.site == 15 {
                            u64::from(((f.a >> 32) as u32).checked_sub(f.low())?)
                        } else if result == 3 {
                            0
                        } else if e.site <= 2 {
                            u64::from(f.low().checked_sub(f.high())?)
                        } else if e.site == 3 && scalar {
                            1
                        } else {
                            0
                        };
                        if e.auxiliary != aux {
                            return None;
                        }
                        e.reservation = f.id;
                        if e.site == 2 {
                            if depth != 2
                                || self.state.frames[0].site() != 1
                                || self.state.frames[0].a != f.id
                                || f.a != self.state.frames[0].id
                                || self.state.frames[0].low() != f.low()
                                || match result {
                                    1 | 2 | 4 => self.state.frames[0].phase() != READY,
                                    3 => self.state.frames[0].phase() != REQUEST,
                                    _ => true,
                                }
                            {
                                return None;
                            }
                            let parent = d.edit(&self.state, 0)?;
                            parent.b = u64::from(parent.low()) | (u64::from(f.high()) << 32);
                            parent.phase_to(match result {
                                1 => CHILD_OK,
                                2 => CHILD_ERROR,
                                3 => CHILD_REFUSED,
                                4 => CHILD_SCALAR,
                                _ => return None,
                            });
                        }
                        d.pop(&self.state)?;
                    }
                    (18, 9, 0) => {
                        if !same
                            || depth != 0
                            || self.context.class != Class::Instrumented
                            || self.state.g.status & NOMINATION != 0
                            || !matches!(e.value, 1 | 2)
                            || e.auxiliary != 0
                        {
                            return None;
                        }
                        d.g.status = NORMAL_FROZEN;
                    }
                    _ => return None,
                }
                d.g.counts = e.counts;
                Some(())
            }
            pub(super) fn finish(
                &mut self,
                site: u32,
                result: usize,
                actual: [usize; 3],
                byte_remaining: Option<usize>,
            ) {
                if self.state.g.fault != 0 {
                    return;
                }
                let Some(i) = self.state.g.depth.checked_sub(1) else {
                    self.fault(3);
                    return;
                };
                let Some(f) = self.state.frames.get(i as usize) else {
                    self.fault(3);
                    return;
                };
                let aux = if site == 15 {
                    match byte_remaining.and_then(|v| u64::try_from(v).ok()) {
                        Some(v) => v,
                        None => {
                            self.fault(2);
                            return;
                        }
                    }
                } else if result == 3 {
                    0
                } else if site <= 2 {
                    match f.low().checked_sub(f.high()) {
                        Some(v) => u64::from(v),
                        None => {
                            self.fault(3);
                            return;
                        }
                    }
                } else if site == 3 && result == 4 {
                    1
                } else {
                    0
                };
                let flags = match site {
                    1 => 1,
                    2 | 3 => 2,
                    4 | 5 => 3,
                    6 | 7 => 4,
                    15 => 5,
                    _ => {
                        self.fault(4);
                        return;
                    }
                };
                self.emit(site, 10, flags, result, aux, actual);
            }
            fn safe_n(&self) -> Option<usize> {
                let n = usize::try_from(self.state.g.cursor).ok()?;
                if n > CAPACITY
                    || (self.state.g.fault == 0 && self.state.g.status & NORMAL_FROZEN == 0)
                    || (self.state.g.fault != 0 && self.state.g.frozen_n != self.state.g.cursor)
                {
                    None
                } else {
                    Some(n)
                }
            }
            pub(super) fn event_block(&self, index: usize) -> Option<[u8; 64]> {
                if index >= self.safe_n()? {
                    return None;
                }
                Some(self.events.get(index)?.encode())
            }
            pub(super) fn prefix_digest(&self) -> Option<[u8; 32]> {
                use sha2::{Digest, Sha256};
                let mut digest = Sha256::new();
                for event in self.events.get(..self.safe_n()?)? {
                    digest.update(event.encode());
                }
                Some(digest.finalize().into())
            }
        }

        // Typed pure encoding inputs; no Deserialize, FD, path, or acquisition.
        pub(super) struct Header {
            pub(super) identity: [u8; 32],
            pub(super) input: [u8; 32],
            pub(super) overlay: [u8; 32],
            pub(super) build: [u8; 32],
            pub(super) artifact: [u8; 32],
        }
        // Accepted schema/base/historical reference, never a permit or stock bridge.
        const CONTROL_SCHEMA: [u8; 32] = [
            0x42, 0x79, 0xd9, 0x57, 0x65, 0x2e, 0x92, 0xbd, 0x49, 0x00, 0xa2, 0x14, 0xf4, 0xde,
            0xe7, 0x09, 0xf5, 0x39, 0x4f, 0x76, 0x8a, 0x03, 0xa5, 0x4b, 0x1c, 0x60, 0x81, 0xaf,
            0x31, 0x12, 0xcb, 0x6d,
        ];
        const EVENT_SCHEMA: [u8; 32] = [
            0xb8, 0xae, 0x54, 0x78, 0x42, 0xe0, 0x81, 0xde, 0x7c, 0xbd, 0xfe, 0xbb, 0x6f, 0xd4,
            0x30, 0x2f, 0xb2, 0x50, 0xf2, 0x44, 0x4e, 0x56, 0xb1, 0x30, 0xd1, 0x9b, 0xc8, 0xd1,
            0x2a, 0xd4, 0x90, 0xcb,
        ];
        const BASE: [u8; 20] = [
            0xcf, 0x3d, 0xc4, 0x2d, 0x4c, 0xc6, 0xf4, 0x7d, 0x51, 0xb2, 0xfa, 0xae, 0xa6, 0x62,
            0x04, 0x63, 0x84, 0x4a, 0xd2, 0x88,
        ];
        const ORIGINAL: [u8; 32] = [
            0x58, 0x43, 0x1d, 0x97, 0xf9, 0x12, 0x91, 0x5e, 0xd9, 0x64, 0xd8, 0xdc, 0x34, 0xc3,
            0xa8, 0xc2, 0x55, 0x73, 0xad, 0x36, 0xfd, 0xbc, 0x5d, 0xec, 0xfb, 0x50, 0x67, 0x31,
            0x9e, 0xbf, 0x14, 0x81,
        ];
        impl Recorder {
            pub(super) fn header_block(&self, h: &Header) -> Option<[u8; 512]> {
                self.safe_n()?;
                let mut b = [0; 512];
                b[..8].copy_from_slice(b"ZLSAW01\0");
                b[8..12].copy_from_slice(&1u32.to_le_bytes());
                b[12..16].copy_from_slice(&512u32.to_le_bytes());
                b[16..20].copy_from_slice(&64u32.to_le_bytes());
                b[20..24].copy_from_slice(&16_384u32.to_le_bytes());
                b[24..32].copy_from_slice(&self.context.ordinal.to_le_bytes());
                b[32..36].copy_from_slice(&(self.context.class as u32).to_le_bytes());
                for (i, v) in [
                    &h.identity,
                    &h.input,
                    &CONTROL_SCHEMA,
                    &EVENT_SCHEMA,
                    &h.overlay,
                    &h.build,
                    &h.artifact,
                ]
                .iter()
                .enumerate()
                {
                    b[40 + 32 * i..72 + 32 * i].copy_from_slice(*v);
                }
                b[264..284].copy_from_slice(&BASE);
                b[284..316].copy_from_slice(&ORIGINAL);
                Some(b)
            }
            pub(super) fn footer_block(
                &self,
                returned: Option<bool>,
                actual: Option<[usize; 3]>,
            ) -> Option<[u8; 256]> {
                let n = u32::try_from(self.safe_n()?).ok()?;
                let mut b = [0; 256];
                b[..8].copy_from_slice(b"ZLSAWEN1");
                b[8..12].copy_from_slice(&1u32.to_le_bytes());
                b[12..16].copy_from_slice(&256u32.to_le_bytes());
                b[16..20].copy_from_slice(&n.to_le_bytes());
                let fault = self.state.g.fault;
                let (observed, result, counts) = if self.context.class == Class::Scalar {
                    (0u32, 0u32, [0u32; 3])
                } else {
                    let counts = actual
                        .and_then(|c| {
                            Some([
                                u32::try_from(c[0]).ok()?,
                                u32::try_from(c[1]).ok()?,
                                u32::try_from(c[2]).ok()?,
                            ])
                        })
                        .filter(|c| c[0] <= 16 && c[1] <= 1024 && c[2] <= 4096)
                        .unwrap_or([u32::MAX; 3]);
                    if fault == 0 {
                        let last = self.events.get(n.checked_sub(1)? as usize)?;
                        if self.state.g.depth != 0
                            || self.state.g.status & NORMAL_FROZEN == 0
                            || returned.map(|ok| if ok { 1 } else { 2 }) != Some(last.value)
                            || counts != self.state.g.counts
                        {
                            return None;
                        }
                    }
                    (
                        u32::from(returned.is_some()),
                        match returned {
                            Some(true) => 1,
                            Some(false) => 2,
                            None => 0,
                        },
                        counts,
                    )
                };
                if self.context.class == Class::Scalar && fault == 0 && self.state.g.depth != 0 {
                    return None;
                }
                b[20..24].copy_from_slice(&fault.to_le_bytes());
                b[24..32].copy_from_slice(
                    &(if n == 0 { u64::MAX } else { u64::from(n) - 1 }).to_le_bytes(),
                );
                b[32..36].copy_from_slice(&observed.to_le_bytes());
                b[36..40].copy_from_slice(&result.to_le_bytes());
                for (i, c) in counts.iter().enumerate() {
                    b[40 + 4 * i..44 + 4 * i].copy_from_slice(&c.to_le_bytes());
                }
                b[56..88].copy_from_slice(&self.prefix_digest()?);
                Some(b)
            }
        }
        // No inhabitant, constructor, File, or backend exists in this slice.
        pub(super) enum BoundSink {}
        pub(super) struct UnboundSink;
        enum LocalState {
            DataFinalized,
            Unavailable,
        }
        enum SyncState {
            NotAttempted,
            Success,
            ErrorUnknown,
        }
        enum CloseState {
            ConsumedSuccess,
            ConsumedErrorUnknown,
        }
        enum OriginalResult {
            Ok,
            Err,
            NotObserved,
        }
        struct PrefixFields {
            records: u32,
            digest: [u8; 32],
            expected: u32,
        }
        // All fields private, no decoder/constructor/binder in this slice.
        pub(super) struct FinalizationOutcome {
            state: LocalState,
            identity: [u8; 32],
            input: [u8; 32],
            prefix: Option<PrefixFields>,
            known_prefix_bytes: u32,
            binary_total: Option<u32>,
            unknown_tail: bool,
            writes_attempted: u32,
            header_complete: bool,
            footer_complete: bool,
            initial_identity: Option<[u8; 32]>,
            post_identity: Option<[u8; 32]>,
            sync: SyncState,
            close: CloseState,
            original_result: OriginalResult,
            first_observer_fault: u32,
            first_overall_fault: u32,
            precheck_error: bool,
            encode_error: bool,
            write_error: bool,
            sync_error: bool,
            postcheck_error: bool,
            close_error: bool,
        }
        impl FinalizationOutcome {
            pub(super) fn record_admission(&self) -> bool {
                false
            }
            pub(super) fn application_source_budget_qualified(&self) -> bool {
                false
            }
            pub(super) fn whole_entry_qualified(&self) -> bool {
                false
            }
            pub(super) fn current_permission(&self) -> bool {
                false
            }
            pub(super) fn schema(&self) -> u32 {
                1
            }
            pub(super) fn scope(&self) -> &'static str {
                "private_test_artifact_only"
            }
            pub(super) fn delivery_status(&self) -> &'static str {
                "not_qualified_by_this_outcome"
            }
        }
        // An unbound refusal is NOT a completed File finalization/close outcome.
        pub(super) struct SinkRefusal;
        impl SinkRefusal {
            pub(super) fn reason(&self) -> &'static str {
                "report_sink_unbound"
            }
            pub(super) fn finalization_outcome(&self) -> Option<FinalizationOutcome> {
                None
            }
            pub(super) fn record_admission(&self) -> bool {
                false
            }
            pub(super) fn application_source_budget_qualified(&self) -> bool {
                false
            }
            pub(super) fn whole_entry_qualified(&self) -> bool {
                false
            }
            pub(super) fn current_permission(&self) -> bool {
                false
            }
        }
        impl UnboundSink {
            pub(super) fn refuse(self) -> SinkRefusal {
                SinkRefusal
            }
        }
        // Prospective pure-control SOURCE ONLY. None of these tests has run.
        #[cfg(test)]
        mod prospective_controls {
            use super::*;
            static SCALAR: Context = Context {
                ordinal: 1,
                class: Class::Scalar,
            };
            static INVALID: Context = Context {
                ordinal: 0,
                class: Class::Scalar,
            };

            #[test]
            fn canonical_event_encoding_uses_explicit_offsets() {
                let e = Event {
                    sequence: 7,
                    ordinal: 1,
                    reservation: 2,
                    operation: 3,
                    site: 16,
                    kind: 6,
                    flags: 5,
                    counts: [1, 2, 3],
                    value: 1,
                    auxiliary: 8192,
                };
                let b = e.encode();
                assert_eq!(&b[..8], &7u64.to_le_bytes());
                assert_eq!(&b[32..36], &16u32.to_le_bytes());
                assert_eq!(&b[52..56], &1i32.to_le_bytes());
                assert_eq!(&b[56..], &8192u64.to_le_bytes());
            }
            #[test]
            fn scalar_metadata_pairs_are_not_original_invocations() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(4, 5, [0, 0, 0]);
                r.charge_outcome(4, 5, true, [0, 0, 5]);
                r.api_begin(11, 0, [0, 0, 5]);
                r.api_return(11, 2, 0, [0, 0, 5]);
                assert_eq!(r.state.g.cursor, 5);
                r.seal_scalar();
                let footer = r.footer_block(None, None).expect("closed scalar prefix");
                assert_eq!(&footer[32..52], &[0u8; 20]);
                r.invocation_return(true, [0, 0, 5]);
                assert_eq!(r.state.g.fault, 3);
                assert_eq!(r.state.g.cursor, 5);
            }
            #[test]
            fn missing_pair_and_first_fault_remain_sticky() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.byte_check(0, 1_048_576, false, [0; 3]);
                r.api_begin(16, 1, [0; 3]);
                assert!(r.footer_block(None, None).is_none());
                r.emit(99, 1, 1, 0, 0, [0; 3]);
                assert_eq!(r.state.g.fault, 4);
                assert_eq!(r.state.g.frozen_n, 2);
                r.fault(7);
                r.api_return(16, 4, 0, [0; 3]);
                assert_eq!(r.state.g.fault, 4);
                assert_eq!(r.state.g.cursor, 2);
            }
            #[test]
            fn capacity_control_reaches_the_real_fixed_boundary() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                for _ in 0..4096 {
                    r.byte_check(0, 1_048_576, false, [0; 3]);
                    r.api_begin(16, 1, [0; 3]);
                    r.api_return(16, 4, 0, [0; 3]);
                    r.finish(15, 1, [0; 3], Some(1_048_576));
                }
                assert_eq!(r.state.g.cursor, 16_384);
                assert_eq!(r.state.g.fault, 0);
                r.byte_check(0, 1_048_576, false, [0; 3]);
                assert_eq!(r.state.g.fault, 1);
                assert_eq!(r.state.g.frozen_n, 16_384);
            }
            #[test]
            fn unbound_sink_refuses_without_finalization_or_authority() {
                let refusal = UnboundSink.refuse();
                assert!(!refusal.record_admission());
                assert!(!refusal.application_source_budget_qualified());
                assert!(!refusal.whole_entry_qualified());
                assert!(!refusal.current_permission());
                assert!(Recorder::new(&INVALID).is_none());
            }
            // G1 R1 documentary witnesses: prospective SOURCE only, never run.
            #[test]
            fn scalar_walk_refuses_finish_before_parent_increment_and_acceptance() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(1, 1, [0, 0, 0]);
                r.charge_request(2, 1, [0, 0, 0]);
                r.charge_outcome(2, 1, true, [0, 1, 0]);
                // Exact invalid row3: parent kind4 and kind2 are both missing.
                r.finish(2, 4, [0, 1, 0], None);
                assert_eq!(r.state.g.fault, 3);
                assert_eq!(r.state.g.cursor, 3);
                assert_eq!(r.state.g.frozen_n, 3);
                assert_eq!(r.state.frames[0].phase(), REQUEST);
                assert_eq!(r.state.g.counts, [0, 1, 0]); // synthetic, not original Counts
                r.finish(1, 4, [0, 1, 0], None);
                r.seal_scalar();
                assert_eq!(r.state.g.cursor, 3);
                assert_eq!(r.state.g.fault, 3);
                let footer = r.footer_block(None, None).expect("faulted raw prefix only");
                assert_eq!(&footer[20..24], &3u32.to_le_bytes());
            }
            #[test]
            fn scalar_walk_refuses_finish_after_increment_without_parent_acceptance() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(1, 1, [0, 0, 0]);
                r.charge_request(2, 1, [0, 0, 0]);
                r.charge_outcome(2, 1, true, [0, 1, 0]);
                r.emit(1, 4, 1, 1, 2, [1, 1, 0]);
                // Parent kind2 alone is missing; kind4 is insufficient.
                r.finish(2, 4, [1, 1, 0], None);
                assert_eq!(r.state.g.fault, 3);
                assert_eq!(r.state.g.cursor, 4);
                assert_eq!(r.state.g.frozen_n, 4);
                assert_eq!(r.state.frames[0].phase(), WALK_UPDATED);
                assert_eq!(r.state.g.counts, [1, 1, 0]); // synthetic, not original Counts
            }
            #[test]
            fn scalar_walk_valid_contrast_includes_parent_increment_then_acceptance() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(1, 1, [0, 0, 0]);
                r.charge_request(2, 1, [0, 0, 0]);
                r.charge_outcome(2, 1, true, [0, 1, 0]);
                r.walk_increment([1, 1, 0]); // kind4 then kind2, exactly once
                r.finish(2, 4, [1, 1, 0], None);
                r.finish(1, 4, [1, 1, 0], None);
                r.seal_scalar();
                assert_eq!(r.state.g.fault, 0);
                assert_eq!(r.state.g.cursor, 7);
                assert_eq!(r.state.g.depth, 0);
                let footer = r.footer_block(None, None).expect("closed scalar contrast");
                assert_eq!(&footer[20..24], &0u32.to_le_bytes());
                assert_eq!(&footer[32..52], &[0u8; 20]); // required-unused, not zero IO
            }
            #[test]
            fn scalar_walk_normal_completion_retains_accepted_parent_order() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(1, 1, [0, 0, 0]);
                r.charge_request(2, 1, [0, 0, 0]);
                r.charge_outcome(2, 1, true, [0, 1, 0]);
                r.walk_increment([1, 1, 0]);
                r.api_begin(8, 0, [1, 1, 0]); // synthetic pair, not a native open
                r.api_return(8, 1, 7, [1, 1, 0]); // synthetic FD scalar, no ownership
                r.finish(2, 1, [1, 1, 0], None);
                r.finish(1, 1, [1, 1, 0], None);
                r.seal_scalar();
                assert_eq!(r.state.g.fault, 0);
                assert_eq!(r.state.g.cursor, 9);
                assert_eq!(r.state.g.depth, 0);
                assert_eq!(r.state.g.counts, [1, 1, 0]);
            }
            #[test]
            fn scalar_walk_error_completion_retains_accepted_parent_and_spent_slots() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                r.charge_request(1, 1, [0, 0, 0]);
                r.charge_request(2, 1, [0, 0, 0]);
                r.charge_outcome(2, 1, true, [0, 1, 0]);
                r.walk_increment([1, 1, 0]);
                r.api_begin(8, 0, [1, 1, 0]);
                r.api_return(8, 2, 0, [1, 1, 0]); // synthetic error plus real core finishes
                r.seal_scalar();
                assert_eq!(r.state.g.fault, 0);
                assert_eq!(r.state.g.cursor, 9);
                assert_eq!(r.state.g.depth, 0);
                assert_eq!(r.state.g.counts, [1, 1, 0]); // no refund
            }
            #[test]
            fn scalar_refused_child_preserves_u01_without_parent_promotion() {
                let mut r = Recorder::new(&SCALAR).expect("prospective scalar context");
                for opens in 0..1024 {
                    r.charge_request(3, 1, [0, opens, 0]);
                    r.charge_outcome(3, 1, true, [0, opens + 1, 0]);
                    r.finish(3, 4, [0, opens + 1, 0], None);
                }
                r.charge_request(1, 1, [0, 1024, 0]);
                r.charge_request(2, 1, [0, 1024, 0]);
                r.charge_outcome(2, 1, false, [0, 1024, 0]);
                assert_eq!(r.state.g.fault, 3); // original U01 helper policy
                assert_eq!(r.state.g.cursor, 3076);
                assert_eq!(r.state.g.frozen_n, 3076);
                assert_eq!(r.state.g.depth, 1); // parent still pending, no fabricated finish
                assert_eq!(r.state.frames[0].phase(), CHILD_REFUSED);
                assert_eq!(r.state.g.counts, [0, 1024, 0]); // no walk increment/refund
            }
        }
    }

    #[derive(Default)]
    struct Counts {
        walks: usize,
        opens: usize,
        metadata: usize,
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        accounting: Option<accounting::Recorder>,
    }
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    impl Counts {
        fn aw_actual(&self) -> [usize; 3] {
            [self.walks, self.opens, self.metadata]
        }
        fn aw_nominate(&mut self, site: u32) {
            if let Some(w) = self.accounting.as_mut() {
                w.nominate(site);
            }
        }
        fn aw_site(&self) -> u32 {
            match self.accounting.as_ref() {
                Some(w) => w.nominated(),
                None => 0,
            }
        }
        fn aw_clear(&mut self) {
            if let Some(w) = self.accounting.as_mut() {
                w.clear_nomination();
            }
        }
        fn aw_fault(&mut self, code: u32) {
            if let Some(w) = self.accounting.as_mut() {
                w.fault(code);
            }
        }
        fn aw_request(&mut self, site: u32, cost: usize) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.charge_request(site, cost, c);
            }
        }
        fn aw_outcome(&mut self, site: u32, cost: usize, accepted: bool) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.charge_outcome(site, cost, accepted, c);
            }
        }
        fn aw_walk_increment(&mut self) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.walk_increment(c);
            }
        }
        fn aw_begin(&mut self, site: u32, q: usize) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.api_begin(site, q, c);
            }
        }
        fn aw_end(&mut self, site: u32, class: usize, n: u64) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.api_return(site, class, n, c);
            }
        }
        fn aw_byte_check(&mut self, length: usize, remaining: usize, rejected: bool) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.byte_check(length, remaining, rejected, c);
            }
        }
        fn aw_consumed(&mut self, n: usize, remaining: usize) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.consumed(n, remaining, c);
            }
        }
        fn aw_finish(&mut self, site: u32, result: usize, remaining: Option<usize>) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.finish(site, result, c, remaining);
            }
        }
        fn aw_return(&mut self, ok: bool) {
            let c = self.aw_actual();
            if let Some(w) = self.accounting.as_mut() {
                w.invocation_return(ok, c);
            }
        }
    }
    impl Counts {
        fn walk(&mut self, path: &Path) -> Checked<()> {
            let cost = path
                .components()
                .filter(|c| matches!(c, Component::Normal(_)))
                .count()
                + 1;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            self.aw_request(1, cost); // AW-H01
            if cost > 33 || self.walks >= 16 {
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                self.aw_outcome(1, cost, false);
                return Err(Fault);
            }
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            self.aw_nominate(2);
            self.open(cost)?;
            self.walks += 1;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            self.aw_walk_increment(); // AW-H03
            Ok(())
        }
        fn open(&mut self, cost: usize) -> Checked<()> {
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            let site = self.aw_site();
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            self.aw_request(site, cost); // AW-H02
            if cost > 1024 - self.opens {
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                {
                    self.aw_outcome(site, cost, false);
                    self.aw_clear();
                }
                return Err(Fault);
            }
            self.opens += cost;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            {
                self.aw_outcome(site, cost, true);
                self.aw_clear();
            }
            Ok(())
        }
        fn meta(&mut self, cost: usize) -> Checked<()> {
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            let site = self.aw_site();
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            self.aw_request(site, cost); // AW-H04
            if cost > 4096 - self.metadata {
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                {
                    self.aw_outcome(site, cost, false);
                    self.aw_clear();
                }
                return Err(Fault);
            }
            self.metadata += cost;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            {
                self.aw_outcome(site, cost, true);
                self.aw_clear();
            }
            Ok(())
        }
    }
    #[derive(PartialEq, Eq)]
    struct Stamp {
        device: u64,
        inode: u64,
        mode: u32,
        uid: u32,
        gid: u32,
        links: u64,
        length: u64,
        modified: (i64, i64),
        changed: (i64, i64),
    }
    impl Stamp {
        fn of(meta: &Metadata) -> Self {
            Self {
                device: meta.dev(),
                inode: meta.ino(),
                mode: meta.mode(),
                uid: meta.uid(),
                gid: meta.gid(),
                links: meta.nlink(),
                length: meta.len(),
                modified: (meta.mtime(), meta.mtime_nsec()),
                changed: (meta.ctime(), meta.ctime_nsec()),
            }
        }
    }
    fn path_stamp(path: &Path, counts: &mut Counts) -> Checked<Option<Stamp>> {
        valid_path(path)?;
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_nominate(4);
        counts.meta(META_COST)?;
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_begin(11, 0); // AW-H10
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                let result = Ok(Some(Stamp::of(&meta)));
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts.aw_end(11, 1, 0);
                result
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let result = Ok(None);
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts.aw_end(11, 2, 0);
                result
            }
            Err(_) => {
                let result = Err(Fault);
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts.aw_end(11, 2, 0);
                result
            }
        }
    }
    fn file_stamp(file: &File, counts: &mut Counts) -> Checked<Stamp> {
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_nominate(5);
        counts.meta(META_COST)?;
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_begin(12, 0); // AW-H11
        let result = file
            .metadata()
            .map(|meta| Stamp::of(&meta))
            .map_err(|_| Fault);
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_end(12, if result.is_ok() { 1 } else { 2 }, 0);
        result
    }
    // Existing Directory's read-only pattern, private to this command. Its
    // generic OpenOptions root open retries EINTR; no such wrapper is used here.
    struct ReadDirectory {
        path: PathBuf,
        file: File,
    }
    fn adopted(
        fd: libc::c_int,
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts: &mut Counts,
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        site: u32,
    ) -> Checked<File> {
        if fd < 0 {
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_end(site, 2, 0); // AW-H08 no-FD arm
            return Err(Fault);
        }
        // SAFETY: every caller passes its one newly acquired owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        // The existing adoption above precedes all observer work; no new unsafe.
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        {
            match u32::try_from(fd) {
                Ok(original_fd) => counts.aw_end(site, 1, u64::from(original_fd)),
                Err(_) => counts.aw_fault(2),
            }
        }
        Ok(file)
    }
    fn component(name: &std::ffi::OsStr) -> Checked<std::ffi::CString> {
        let bytes = name.as_encoded_bytes();
        if bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
            return Err(Fault);
        }
        // Bounds/NUL were checked before this owned copy.
        std::ffi::CString::new(bytes).map_err(|_| Fault)
    }
    impl ReadDirectory {
        fn open(path: &Path, counts: &mut Counts) -> Checked<Self> {
            valid_path(path)?;
            // Reserve EVERY potential open before beginning this finite walk.
            counts.walk(path)?;
            let flags = libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_NOCTTY
                | libc::O_CLOEXEC;
            // SAFETY: fixed absolute root, read-only flags, no variadic mode.
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_begin(8, 0); // AW-H05
            let mut file = adopted(
                unsafe { libc::open(c"/".as_ptr(), flags) },
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts,
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                8,
            )?;
            for part in path.components() {
                match part {
                    Component::RootDir => (),
                    Component::Normal(name) => {
                        let name = component(name)?;
                        // SAFETY: owned live parent and one bounded NUL-free
                        // component. EINTR/error refuses, with NO retry/fallback.
                        #[cfg(all(
                            test,
                            target_os = "linux",
                            target_arch = "x86_64",
                            target_env = "gnu"
                        ))]
                        counts.aw_begin(9, 0); // AW-H06; after component conversion
                        file = adopted(
                            unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) },
                            #[cfg(all(
                                test,
                                target_os = "linux",
                                target_arch = "x86_64",
                                target_env = "gnu"
                            ))]
                            counts,
                            #[cfg(all(
                                test,
                                target_os = "linux",
                                target_arch = "x86_64",
                                target_env = "gnu"
                            ))]
                            9,
                        )?;
                    }
                    _ => return Err(Fault),
                }
            }
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            {
                counts.aw_finish(2, 1, None);
                counts.aw_finish(1, 1, None);
            } // AW-H09
            Ok(Self {
                path: path.into(),
                file,
            })
        }
        fn private(&self, counts: &mut Counts) -> Checked<bool> {
            // The existing predicate has one metadata API and a possible eUID
            // query. Charge both BEFORE calling, even if it short-circuits.
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_nominate(6);
            counts.meta(META_COST + 1)?;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_begin(13, 0); // AW-H12
            let result = crate::platform::verify_private_directory(&self.file).map_err(|_| Fault);
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_end(
                13,
                match &result {
                    Ok(false) => 5,
                    Ok(true) => 6,
                    Err(_) => 2,
                },
                0,
            );
            result
        }
        fn verify(&self, counts: &mut Counts) -> Checked<()> {
            let current = Self::open(&self.path, counts)?;
            let held = file_stamp(&self.file, counts)?;
            let now = file_stamp(&current.file, counts)?;
            if (held.device, held.inode) != (now.device, now.inode) {
                return Err(Fault);
            }
            Ok(())
        }
        fn read_private(&self, leaf: &str, counts: &mut Counts) -> Checked<File> {
            if leaf.is_empty() || leaf.contains(['/', '\\', ':']) || matches!(leaf, "." | "..") {
                return Err(Fault);
            }
            let name = component(std::ffi::OsStr::new(leaf))?;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_nominate(3);
            counts.open(1)?;
            // SAFETY: held parent plus one fixed bounded leaf. No retry, no
            // std open wrapper, no create, and immediate owned FD adoption.
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_begin(10, 0); // AW-H07
            let file = adopted(
                unsafe {
                    libc::openat(
                        self.file.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY
                            | libc::O_NOFOLLOW
                            | libc::O_NONBLOCK
                            | libc::O_NOCTTY
                            | libc::O_CLOEXEC,
                    )
                },
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts,
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                10,
            )?;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_nominate(7);
            counts.meta(META_COST + 1)?;
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_begin(14, 0); // AW-H13
            let result = crate::platform::verify_private_file(&file).map_err(|_| Fault);
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_end(
                14,
                match &result {
                    Ok(false) => 5,
                    Ok(true) => 6,
                    Err(_) => 2,
                },
                0,
            );
            if !result? {
                return Err(Fault);
            }
            Ok(file)
        }
    }
    struct Root {
        directory: ReadDirectory,
        stamp: Stamp,
    }
    impl Root {
        fn pin(path: &Path, counts: &mut Counts) -> Checked<Self> {
            valid_path(path)?;
            let stamp = path_stamp(path, counts)?.ok_or(Fault)?;
            let directory = ReadDirectory::open(path, counts)?;
            if !directory.private(counts)? {
                return Err(Fault);
            }
            directory.verify(counts)?;
            if path_stamp(path, counts)?.as_ref() != Some(&stamp) {
                return Err(Fault);
            }
            Ok(Self { directory, stamp })
        }
        fn finish(&self, counts: &mut Counts) -> Checked<()> {
            if !self.directory.private(counts)? {
                return Err(Fault);
            }
            self.directory.verify(counts)?;
            if path_stamp(&self.directory.path, counts)?.as_ref() != Some(&self.stamp) {
                return Err(Fault);
            }
            Ok(())
        }
        fn path(&self, leaf: &str) -> PathBuf {
            self.directory.path.join(leaf)
        }
        fn open(&self, leaf: &str, counts: &mut Counts) -> Checked<File> {
            valid_path(&self.path(leaf))?;
            self.directory.read_private(leaf, counts)
        }
    }
    struct ByteBudget {
        remaining: usize,
    }
    impl ByteBudget {
        fn new() -> Self {
            Self { remaining: RAW }
        }
        fn consume(&mut self, bytes: usize) -> Checked<()> {
            if bytes > self.remaining {
                return Err(Fault);
            }
            self.remaining -= bytes;
            Ok(())
        }
    }
    // One stack buffer serves both bounded initial reads and final hash reads.
    // Never retain a sentinel, retry an interrupted read or parse a prefix.
    fn bounded(
        file: &mut File,
        length: usize,
        budget: &mut ByteBudget,
        retain: bool,
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts: &mut Counts,
    ) -> Checked<(Vec<u8>, [u8; 32])> {
        if length > budget.remaining {
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_byte_check(length, budget.remaining, true); // AW-H14
            return Err(Fault);
        }
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_byte_check(length, budget.remaining, false);
        let mut bytes = if retain {
            Vec::with_capacity(length)
        } else {
            Vec::new()
        };
        let mut hash = Sha256::new();
        let mut read = 0usize;
        let mut scratch = [0u8; SMALL];
        loop {
            let maximum = scratch.len().min(length.saturating_sub(read) + 1);
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_begin(16, maximum); // AW-H15
            let result = file.read(&mut scratch[..maximum]).map_err(|_| Fault);
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            {
                match &result {
                    Ok(n) => match u64::try_from(*n) {
                        Ok(n64) => counts.aw_end(16, if *n == 0 { 4 } else { 1 }, n64),
                        Err(_) => counts.aw_fault(2),
                    },
                    Err(_) => {
                        counts.aw_end(16, 2, 0);
                        counts.aw_finish(15, 2, Some(budget.remaining));
                    }
                }
            }
            let count = result?;
            if count == 0 {
                break;
            }
            if count > length - read {
                #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
                counts.aw_finish(15, 2, Some(budget.remaining)); // AW-H16 sentinel
                return Err(Fault);
            }
            let result = budget.consume(count);
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            match &result {
                Ok(()) => counts.aw_consumed(count, budget.remaining),
                Err(_) => counts.aw_fault(3), // U03, no synthetic consume/finish
            }
            result?;
            hash.update(&scratch[..count]);
            if retain {
                bytes.extend_from_slice(&scratch[..count]);
            }
            read += count;
        }
        if read != length {
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts.aw_finish(15, 2, Some(budget.remaining)); // AW-H17 short EOF
            return Err(Fault);
        }
        #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        counts.aw_finish(15, 1, Some(budget.remaining)); // actual exact EOF
        Ok((bytes, hash.finalize().into()))
    }
    struct Image {
        file: File,
        stamp: Stamp,
        bytes: Vec<u8>,
        hash: [u8; 32],
    }
    fn capture(
        root: &Root,
        leaf: &str,
        cap: usize,
        bytes: &mut ByteBudget,
        counts: &mut Counts,
    ) -> Checked<Option<Image>> {
        let Some(stamp) = path_stamp(&root.path(leaf), counts)? else {
            return Ok(None);
        };
        if stamp.length > cap as u64 || stamp.length > bytes.remaining as u64 {
            return Err(Fault);
        }
        let mut file = root.open(leaf, counts)?;
        if file_stamp(&file, counts)? != stamp {
            return Err(Fault);
        }
        let (raw, hash) = bounded(
            &mut file,
            stamp.length as usize,
            bytes,
            true,
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts,
        )?;
        if file_stamp(&file, counts)? != stamp
            || path_stamp(&root.path(leaf), counts)?.as_ref() != Some(&stamp)
        {
            return Err(Fault);
        }
        Ok(Some(Image {
            file,
            stamp,
            bytes: raw,
            hash,
        }))
    }
    fn recheck(
        root: &Root,
        leaf: &str,
        image: Option<&Image>,
        bytes: &mut ByteBudget,
        counts: &mut Counts,
    ) -> Checked<()> {
        let stamp = path_stamp(&root.path(leaf), counts)?;
        let Some(image) = image else {
            return if stamp.is_none() { Ok(()) } else { Err(Fault) };
        };
        if stamp.as_ref() != Some(&image.stamp) || file_stamp(&image.file, counts)? != image.stamp {
            return Err(Fault);
        }
        let mut file = root.open(leaf, counts)?;
        if file_stamp(&file, counts)? != image.stamp {
            return Err(Fault);
        }
        let (_, hash) = bounded(
            &mut file,
            image.stamp.length as usize,
            bytes,
            false,
            #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
            counts,
        )?;
        if hash != image.hash
            || file_stamp(&file, counts)? != image.stamp
            || file_stamp(&image.file, counts)? != image.stamp
            || path_stamp(&root.path(leaf), counts)?.as_ref() != Some(&image.stamp)
        {
            return Err(Fault);
        }
        Ok(())
    }
    // Refuse duplicate JSON keys before any Value/Store can erase ambiguity.
    // This deliberately also refuses duplicate opaque fields in this reader.
    struct Unique;
    impl<'de> DeserializeSeed<'de> for Unique {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            decoder: D,
        ) -> std::result::Result<(), D::Error> {
            decoder.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Unique {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unambiguous JSON")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<(), M::Error> {
            let mut keys = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key) {
                    return Err(serde::de::Error::custom("ambiguous JSON"));
                }
                map.next_value_seed(Unique)?;
            }
            Ok(())
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<(), S::Error> {
            while seq.next_element_seed(Unique)?.is_some() {}
            Ok(())
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
    }
    fn unique(bytes: &[u8]) -> Checked<()> {
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        Unique.deserialize(&mut decoder).map_err(|_| Fault)?;
        decoder.end().map_err(|_| Fault)
    }
    fn json(image: &Image) -> Checked<Value> {
        unique(&image.bytes)?;
        let value: Value = serde_json::from_slice(&image.bytes).map_err(|_| Fault)?;
        if !value.is_object()
            || ["schema", "version"]
                .iter()
                .any(|key| value.get(key).is_some_and(|v| v.as_u64() != Some(1)))
        {
            return Err(Fault);
        }
        Ok(value)
    }
    fn text<'a>(value: &'a Value, key: &str) -> Checked<&'a str> {
        value.get(key).and_then(Value::as_str).ok_or(Fault)
    }
    fn name(value: &str) -> bool {
        let parts: Vec<_> = value.split('/').collect();
        !value.is_empty()
            && value.len() <= 512
            && (2..=3).contains(&parts.len())
            && parts.iter().all(|part| {
                !part.is_empty()
                    && !part.contains([':', '.'])
                    && !part.chars().any(|c| c.is_control() || c.is_whitespace())
            })
    }
    fn finite(value: &Value, key: &str) -> Checked<f64> {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or(Fault)
    }
    #[derive(Serialize)]
    #[serde(tag = "status", rename_all = "snake_case")]
    pub(super) enum Reply {
        Unknown {
            schema: u32,
            machine_id: String,
            launch_id: String,
        },
        ObservedBinding {
            schema: u32,
            machine_id: String,
            launch_id: String,
            name: String,
            agent: String,
            run_id: String,
            conversation_id: String,
        },
        ObservedEnd {
            schema: u32,
            machine_id: String,
            launch_id: String,
            name: String,
            code: i32,
            ended_at: f64,
            binding_unresolved: bool,
        },
    }
    fn unknown(request: &Request) -> Reply {
        Reply::Unknown {
            schema: 1,
            machine_id: request.machine.clone(),
            launch_id: request.launch.clone(),
        }
    }
    struct Locations {
        identity: PathBuf,
        state: PathBuf,
    }
    fn valid_path(path: &Path) -> Checked<()> {
        if !path.is_absolute()
            || path.as_os_str().len() > 4096
            || path
                .components()
                .filter(|c| matches!(c, Component::Normal(_)))
                .count()
                > 32
            || path
                .components()
                .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        {
            return Err(Fault);
        }
        Ok(())
    }
    const CWD_BUFFER: usize = 4096;
    fn cwd_path(bytes: &[u8]) -> Checked<PathBuf> {
        if bytes.is_empty() || bytes.len() >= CWD_BUFFER || bytes.contains(&0) {
            return Err(Fault);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Fault)?;
        let path = PathBuf::from(text);
        valid_path(&path)?;
        Ok(path)
    }
    #[cfg(any(target_os = "linux", test))]
    fn linux_cwd_reply(buffer: &[u8; CWD_BUFFER], written: libc::c_long) -> Checked<PathBuf> {
        let count = usize::try_from(written).map_err(|_| Fault)?;
        if count == 0 || count > buffer.len() || buffer[count - 1] != 0 {
            return Err(Fault);
        }
        // Linux's return includes the ONE terminal NUL. No prior NUL is valid.
        cwd_path(&buffer[..count - 1])
    }
    #[cfg(any(target_os = "macos", test))]
    fn mac_cwd_reply(buffer: &[u8; CWD_BUFFER], api_range: usize) -> Checked<PathBuf> {
        // F_GETPATH returns no byte count. Its SDK MAXPATHLEN, not our larger
        // allocation or zero initialization, bounds where a real NUL may occur.
        let written = buffer.get(..api_range).ok_or(Fault)?;
        let terminal = written.iter().position(|byte| *byte == 0).ok_or(Fault)?;
        cwd_path(&written[..terminal])
    }
    #[cfg(target_os = "linux")]
    fn bounded_cwd(counts: &mut Counts) -> Checked<PathBuf> {
        counts.meta(1)?;
        let mut buffer = [0xa5; CWD_BUFFER];
        // SAFETY: buffer is writable for exactly its supplied length. Calling
        // the kernel directly avoids libc getcwd's growing/path-walk fallback.
        let written = unsafe { libc::syscall(libc::SYS_getcwd, buffer.as_mut_ptr(), buffer.len()) };
        // Any error, including EINTR, refuses. Never retry or use another API.
        linux_cwd_reply(&buffer, written)
    }
    #[cfg(target_os = "macos")]
    fn bounded_cwd(counts: &mut Counts) -> Checked<PathBuf> {
        use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
        const _: () = assert!(libc::MAXPATHLEN > 0 && libc::MAXPATHLEN as usize <= CWD_BUFFER);
        counts.open(1)?;
        // SAFETY: fixed NUL-terminated dot; no caller path or ownership alias.
        let fd = unsafe {
            libc::open(
                c".".as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(Fault);
        }
        // SAFETY: this is the one newly owned descriptor from successful open.
        let file = unsafe { File::from_raw_fd(fd) };
        let result = (|| {
            counts.meta(META_COST)?;
            let before = file.metadata().map_err(|_| Fault)?;
            if !before.is_dir() || before.nlink() == 0 {
                return Err(Fault);
            }
            let mut buffer = [0xa5; CWD_BUFFER];
            counts.meta(1)?;
            // SAFETY: SDK requires MAXPATHLEN writable bytes; our fixed buffer
            // satisfies that compile-time checked bound and stays alive here.
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) } != 0 {
                return Err(Fault);
            }
            let path = mac_cwd_reply(&buffer, libc::MAXPATHLEN as usize)?;
            counts.meta(META_COST)?;
            let after = file.metadata().map_err(|_| Fault)?;
            if !after.is_dir() || after.nlink() == 0 || Stamp::of(&before) != Stamp::of(&after) {
                return Err(Fault);
            }
            Ok(path)
        })();
        // Consume ownership BEFORE close: ambiguous close/EINTR is never retried
        // by us or File::drop after the descriptor number could have been reused.
        let fd = file.into_raw_fd();
        // SAFETY: fd is this still-owned transient descriptor; close exactly once.
        if unsafe { libc::close(fd) } != 0 {
            return Err(Fault);
        }
        // No source root can be opened until this exact close has succeeded.
        result
    }
    fn locations(counts: &mut Counts) -> Checked<Locations> {
        let mut aggregate = 0usize;
        let mut variable = |key| -> Checked<Option<String>> {
            match std::env::var(key) {
                Ok(value) => {
                    if value.is_empty() || value.len() > 4096 {
                        return Err(Fault);
                    }
                    aggregate += value.len();
                    if aggregate > 16 << 10 {
                        return Err(Fault);
                    }
                    Ok(Some(value))
                }
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => Err(Fault),
            }
        };
        let home = variable("HOME")?.ok_or(Fault)?;
        let config = variable("HGS_CONFIG_DIR")?
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&home).join(".config/hgs"));
        let state = variable("HGS_STATE_DIR")?;
        let xdg = variable("XDG_STATE_HOME")?;
        let cwd = bounded_cwd(counts)?;
        let current = cwd.to_str().ok_or(Fault)?;
        if current.len() > 4096 || aggregate + current.len() > 16 << 10 {
            return Err(Fault);
        }
        let state = state.map(PathBuf::from).unwrap_or_else(|| {
            xdg.map(PathBuf::from)
                .unwrap_or_else(|| Path::new(&home).join(".local/state"))
                .join("hgs/sessions")
        });
        let absolute = |path: PathBuf| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        let config = absolute(config);
        let state = absolute(state);
        valid_path(&config)?;
        valid_path(&state)?;
        let identity = config.join("swarm");
        valid_path(&identity)?;
        Ok(Locations { identity, state })
    }
    fn selected(request: &Request, path: &Path, counts: &mut Counts) -> Checked<Reply> {
        let state = Root::pin(path, counts)?;
        let launches = Root::pin(&path.join("launches"), counts)?;
        let mut bytes = ByteBudget::new();
        let start_leaf = format!("{}.json", request.launch);
        let end_leaf = format!("{}.end.json", request.launch);
        let start = capture(&launches, &start_leaf, SMALL, &mut bytes, counts)?;
        let end = capture(&launches, &end_leaf, SMALL, &mut bytes, counts)?;
        let start_value = start.as_ref().map(json).transpose()?;
        let end_value = end.as_ref().map(json).transpose()?;
        let mut routes = None;
        let mut route_absent = None;
        let mut binding = None;
        let mut binding_leaf = None;
        if let Some(start) = &start_value {
            if text(start, "launch_id")? != request.launch || !name(text(start, "name")?) {
                return Err(Fault);
            }
            finite(start, "started_at")?;
            let leaf = format!("{:x}.json", Sha256::digest(text(start, "name")?.as_bytes()));
            let route_path = path.join("name_routes");
            match path_stamp(&route_path, counts)? {
                None => route_absent = Some(route_path),
                Some(_) => {
                    let root = Root::pin(&route_path, counts)?;
                    if path_stamp(&root.path(&leaf), counts)?.is_some() {
                        return Err(Fault);
                    }
                    routes = Some(root);
                }
            }
            binding = capture(&state, &leaf, RAW, &mut bytes, counts)?;
            binding_leaf = Some(leaf);
        }
        let result = if let Some(binding) = &binding {
            let value = json(binding)?;
            let original_name = text(start_value.as_ref().ok_or(Fault)?, "name")?;
            let agent = text(&value, "agent")?;
            let run = text(&value, "run_id")?;
            let conversation = text(&value, "conversation_id")?;
            if value.get("version").and_then(Value::as_u64) != Some(1)
                || text(&value, "launch_id")? != request.launch
                || text(&value, "name")? != original_name
                || !matches!(agent, "claude" | "codex")
                || original_name.split('/').next() != Some(agent)
                || !canonical(run)
                || !canonical(conversation)
                || [
                    "expected_id",
                    "creation_pending",
                    "rename_shadow",
                    "rename",
                    "renaming",
                ]
                .iter()
                .any(|key| value.get(key).is_some())
                || end.is_some()
            {
                return Err(Fault);
            }
            Reply::ObservedBinding {
                schema: 1,
                machine_id: request.machine.clone(),
                launch_id: request.launch.clone(),
                name: original_name.into(),
                agent: agent.into(),
                run_id: run.into(),
                conversation_id: conversation.into(),
            }
        } else if let Some(end) = &end_value {
            let observed_name = text(end, "name")?;
            if text(end, "launch_id")? != request.launch
                || !name(observed_name)
                || end.get("draining").is_some()
                || end.get("error").is_some()
                || end.get("output").is_some_and(|v| !v.is_string())
                || start_value.as_ref().is_some_and(|start| {
                    start.get("name").and_then(Value::as_str) != Some(observed_name)
                })
            {
                return Err(Fault);
            }
            let code = end
                .get("code")
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(Fault)?;
            Reply::ObservedEnd {
                schema: 1,
                machine_id: request.machine.clone(),
                launch_id: request.launch.clone(),
                name: observed_name.into(),
                code,
                ended_at: finite(end, "ended_at")?,
                binding_unresolved: true,
            }
        } else {
            unknown(request)
        };
        let mut rechecked = ByteBudget::new();
        recheck(
            &launches,
            &start_leaf,
            start.as_ref(),
            &mut rechecked,
            counts,
        )?;
        recheck(&launches, &end_leaf, end.as_ref(), &mut rechecked, counts)?;
        if let Some(leaf) = &binding_leaf {
            recheck(&state, leaf, binding.as_ref(), &mut rechecked, counts)?;
            if let Some(root) = &routes {
                if path_stamp(&root.path(leaf), counts)?.is_some() {
                    return Err(Fault);
                }
            }
        }
        if let Some(path) = route_absent {
            if path_stamp(&path, counts)?.is_some() {
                return Err(Fault);
            }
        }
        if let Some(root) = routes {
            root.finish(counts)?;
        }
        launches.finish(counts)?;
        state.finish(counts)?;
        Ok(result)
    }
    pub(super) fn observe(request: &Request) -> Checked<Reply> {
        let mut counts = Counts::default();
        let locations = locations(&mut counts)?;
        observe_at(request, &locations, &mut counts)
    }
    fn observe_at(request: &Request, locations: &Locations, counts: &mut Counts) -> Checked<Reply> {
        let root = Root::pin(&locations.identity, counts)?;
        let identity =
            capture(&root, "catalog.json", RAW, &mut ByteBudget::new(), counts)?.ok_or(Fault)?;
        unique(&identity.bytes)?;
        if crate::swarm::existing_node_id(&identity.bytes).map_err(|_| Fault)? != request.machine {
            return Err(Fault);
        }
        let reply =
            selected(request, &locations.state, counts).unwrap_or_else(|_| unknown(request));
        recheck(
            &root,
            "catalog.json",
            Some(&identity),
            &mut ByteBudget::new(),
            counts,
        )?;
        root.finish(counts)?;
        Ok(reply)
    }
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    struct AccountingFixture {
        request: Request,
        locations: Locations,
        counts: Counts,
        _sink: accounting::BoundSink,
    }
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    fn accounting_prepare_unbound(
        sink: accounting::UnboundSink,
    ) -> std::result::Result<AccountingFixture, accounting::SinkRefusal> {
        // No source acquisition/backend/constructor; no Boolean upgrades this.
        Err(sink.refuse())
    }
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    #[test]
    fn accounting_source_fixture_remains_unbound() {
        match accounting_prepare_unbound(accounting::UnboundSink) {
            Err(refusal) => {
                assert_eq!(refusal.reason(), "report_sink_unbound");
                assert!(refusal.finalization_outcome().is_none());
                assert!(!refusal.record_admission());
            }
            Ok(AccountingFixture {
                request,
                locations,
                mut counts,
                _sink,
            }) => {
                // The uninhabited sink has no constructor in this slice. This
                // prospective direct test-call branch remains unreachable and
                // unqualified until separately reviewed real fixture binding.
                assert_eq!(counts.aw_actual(), [0; 3]);
                assert!(counts.accounting.as_ref().is_some_and(|w| w.source_ready()));
                let result = observe_at(&request, &locations, &mut counts);
                // AW-H19 AFTER actual return only.
                counts.aw_return(result.is_ok());
                // No unwrap/catch/retry, fake finish, or sink finalizer follows.
            }
        }
    }
    pub(super) fn encode(reply: &Reply) -> Checked<Vec<u8>> {
        struct Bounded(Vec<u8>);
        impl Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.len() > ENCODED - 1 - self.0.len() {
                    return Err(io::Error::other("encoded bound"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut out = Bounded(Vec::with_capacity(ENCODED));
        serde_json::to_writer(&mut out, reply).map_err(|_| Fault)?;
        out.0.push(b'\n');
        Ok(out.0)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;
        use std::os::unix::fs::{symlink, PermissionsExt};

        #[test]
        fn cwd_kernel_length_nul_unicode_and_component_bounds_are_strict() {
            let mut buffer = [0xa5; CWD_BUFFER];
            for written in [-1, 0, 4097] {
                assert!(linux_cwd_reply(&buffer, written).is_err());
            }
            buffer[..3].copy_from_slice(b"/x\0");
            assert_eq!(linux_cwd_reply(&buffer, 3).unwrap(), Path::new("/x"));
            assert!(linux_cwd_reply(&buffer, 4).is_err());
            buffer[..5].copy_from_slice(b"/x\0y\0");
            assert!(linux_cwd_reply(&buffer, 5).is_err());
            buffer[..3].copy_from_slice(&[b'/', 0xff, 0]);
            assert!(linux_cwd_reply(&buffer, 3).is_err());
            assert!(cwd_path(b"(unreachable)/x").is_err());
            assert!(cwd_path(b"relative").is_err());
            assert!(cwd_path(format!("/{}", vec!["x"; 33].join("/")).as_bytes()).is_err());
            buffer.fill(b'x');
            buffer[0] = b'/';
            buffer[CWD_BUFFER - 1] = 0;
            assert!(linux_cwd_reply(&buffer, CWD_BUFFER as libc::c_long).is_ok());
            buffer[CWD_BUFFER - 1] = b'x';
            assert!(linux_cwd_reply(&buffer, CWD_BUFFER as libc::c_long).is_err());
        }
        #[test]
        fn mac_cwd_requires_a_real_terminator_within_the_api_output_range() {
            let mut buffer = [0xa5; CWD_BUFFER];
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
            assert!(mac_cwd_reply(&buffer, 0).is_err());
            assert!(mac_cwd_reply(&buffer, CWD_BUFFER + 1).is_err());
            buffer[..2].copy_from_slice(b"/x");
            buffer[1024] = 0; // Outside MAXPATHLEN cannot supply the terminator.
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
            buffer[2] = 0;
            assert_eq!(mac_cwd_reply(&buffer, 1024).unwrap(), Path::new("/x"));
            buffer[1] = 0xff;
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
        }
        fn id(n: u8) -> String {
            format!("00000000-1111-4111-8111-{n:012}")
        }
        fn args(words: &[&str]) -> Vec<String> {
            words.iter().map(|s| (*s).into()).collect()
        }
        fn private_dir(path: &Path) {
            fs::create_dir_all(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fn write(path: &Path, bytes: &[u8]) {
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        struct Fixture {
            _root: tempfile::TempDir,
            locations: Locations,
            request: Request,
        }
        impl Fixture {
            fn new() -> Self {
                let root = tempfile::tempdir().unwrap();
                let identity = root.path().join("config/swarm");
                let state = root.path().join("state");
                for path in [
                    &identity,
                    &state,
                    &state.join("launches"),
                    &state.join("name_routes"),
                ] {
                    private_dir(path);
                }
                write(
                    &identity.join("catalog.json"),
                    &serde_json::to_vec(&json!({
                        "node_id":id(1),"catalog":{"schema":1,"swarm_id":id(9),"operations":{}}
                    }))
                    .unwrap(),
                );
                Self {
                    _root: root,
                    locations: Locations { identity, state },
                    request: Request {
                        machine: id(1),
                        launch: id(2),
                    },
                }
            }
            fn start_path(&self) -> PathBuf {
                self.locations
                    .state
                    .join("launches")
                    .join(format!("{}.json", id(2)))
            }
            fn end_path(&self) -> PathBuf {
                self.locations
                    .state
                    .join("launches")
                    .join(format!("{}.end.json", id(2)))
            }
            fn leaf(&self) -> String {
                format!("{:x}.json", Sha256::digest(b"codex/fixture/original"))
            }
            fn binding_path(&self) -> PathBuf {
                self.locations.state.join(self.leaf())
            }
            fn start(&self) {
                write(
                    &self.start_path(),
                    &serde_json::to_vec(&json!({
                        "launch_id":id(2),"name":"codex/fixture/original","started_at":1.0
                    }))
                    .unwrap(),
                );
            }
            fn binding(&self) -> Value {
                json!({
                    "version":1,"name":"codex/fixture/original","agent":"codex",
                    "launch_id":id(2),"run_id":id(3),"conversation_id":id(4),
                    "opaque":{"secret":"PRIVATE BODY"}
                })
            }
            fn end(&self) -> Value {
                json!({
                    "launch_id":id(2),"name":"codex/fixture/original","code":0,
                    "ended_at":2.0,"output":"PRIVATE NATIVE OUTPUT"
                })
            }
            fn put_binding(&self, value: &Value) {
                write(&self.binding_path(), &serde_json::to_vec(value).unwrap());
            }
            fn put_end(&self, value: &Value) {
                write(&self.end_path(), &serde_json::to_vec(value).unwrap());
            }
            fn read(&self) -> (Value, Counts) {
                let mut counts = Counts::default();
                let reply = observe_at(&self.request, &self.locations, &mut counts).unwrap();
                let encoded = encode(&reply).unwrap();
                assert!(encoded.len() <= ENCODED);
                assert!(!String::from_utf8_lossy(&encoded).contains("PRIVATE"));
                (serde_json::from_slice(&encoded).unwrap(), counts)
            }
        }
        #[test]
        fn positional_recognition_does_not_hijack_native_arguments_or_origin_labels() {
            for value in [
                args(&["codex", "fixture", "-n", COMMAND]),
                args(&["sh", COMMAND]),
                args(&[
                    "--expect-machine",
                    &id(1),
                    "--client",
                    COMMAND,
                    "dsh",
                    "fixture",
                ]),
                args(&["--expect-machine", &id(1), "dsh", COMMAND]),
                args(&["--help", "unrelated", CAPABILITY]),
            ] {
                assert!(!reserved(&value, 0));
                assert!(early(&value).is_none());
            }
            for value in [
                args(&[COMMAND]),
                args(&["--expect-machine", COMMAND, "--launch-id", &id(2), "--json"]),
                args(&[
                    "--expect-machine",
                    &id(1),
                    "--client",
                    COMMAND,
                    "--launch-id",
                    &id(2),
                    "--json",
                ]),
                args(&["--help", CAPABILITY, "extra"]),
            ] {
                assert!(reserved(&value, 0));
                assert!(request(&value).is_none());
            }
            let remote = args(&[
                "@fixture",
                "--expect-machine",
                &id(1),
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(reserved(&remote, 1));
            assert!(early(&remote).unwrap().is_err());
        }
        #[test]
        fn exact_guard_grammar_canonical_ids_and_nonidentity_origin() {
            let plain = args(&[
                "--expect-machine",
                &id(1),
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(request(&plain).is_some());
            let forwarded = args(&[
                "--expect-machine",
                &id(1),
                "--client",
                COMMAND,
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(request(&forwarded).is_some());
            for (index, value) in [
                (1, "00000000-0000-0000-0000-000000000000"),
                (1, "ABCDEF00-1111-4111-8111-000000000001"),
                (4, "not-a-uuid"),
                (3, "--request-id"),
                (5, "--dry-run"),
            ] {
                let mut changed = plain.clone();
                changed[index] = value.into();
                assert!(request(&changed).is_none());
            }
            for origin in ["", "two words", "\n"] {
                let mut changed = forwarded.clone();
                changed[3] = origin.into();
                assert!(request(&changed).is_none());
            }
            let mut extra = plain;
            extra.push("--json".into());
            assert!(request(&extra).is_none());
        }
        #[test]
        fn actual_original_binding_is_closed_historical_body_free_evidence() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let (reply, counts) = f.read();
            assert_eq!(reply["status"], "observed_binding");
            assert_eq!(reply.as_object().unwrap().len(), 8);
            assert_eq!(reply["machine_id"], id(1));
            assert_eq!(reply["launch_id"], id(2));
            assert_eq!(reply["run_id"], id(3));
            assert_eq!(reply["conversation_id"], id(4));
            assert!(counts.walks <= 16 && counts.opens <= 1024 && counts.metadata <= 4096);
        }
        #[test]
        fn missing_start_never_selects_a_current_or_independent_name_binding() {
            let f = Fixture::new();
            f.put_binding(&f.binding());
            assert_eq!(f.read().0["status"], "unknown");
            f.put_end(&f.end());
            let (reply, _) = f.read();
            assert_eq!(reply["status"], "observed_end");
            assert_eq!(reply["binding_unresolved"], true);
            assert_eq!(reply.as_object().unwrap().len(), 8);
            fs::remove_file(f.end_path()).unwrap();
            f.start();
            fs::remove_file(f.binding_path()).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn matching_end_or_any_present_route_refuses_a_binding() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            f.put_end(&f.end());
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.end_path()).unwrap();
            write(
                &f.locations.state.join("name_routes").join(f.leaf()),
                b"not even parsed",
            );
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn provider_identity_pending_and_future_binding_fields_refuse() {
            let f = Fixture::new();
            f.start();
            for (key, value) in [
                ("agent", json!("kimi")),
                ("name", json!("codex/fixture/reused")),
                ("launch_id", json!(id(8))),
                ("run_id", Value::Null),
                ("conversation_id", json!("ses_not_a_codex_id")),
                ("version", json!(2)),
                ("expected_id", Value::Null),
                ("creation_pending", json!(false)),
                ("rename_shadow", json!(false)),
                ("rename", json!({})),
            ] {
                let mut binding = f.binding();
                binding[key] = value;
                f.put_binding(&binding);
                assert_eq!(f.read().0["status"], "unknown", "{key}");
            }
        }
        #[test]
        fn end_requires_the_actual_completed_encoding_and_explicit_valid_fields() {
            let f = Fixture::new();
            for (key, value) in [
                ("name", Value::Null),
                ("name", json!("")),
                ("code", json!(2147483648_i64)),
                ("code", json!(1.0)),
                ("code", Value::Null),
                ("ended_at", json!(-1)),
                ("ended_at", Value::Null),
                ("launch_id", json!(id(8))),
                ("draining", json!(false)),
                ("draining", Value::Null),
                ("error", json!("")),
                ("error", Value::Null),
                ("output", json!({})),
            ] {
                let mut end = f.end();
                end[key] = value;
                f.put_end(&end);
                assert_eq!(f.read().0["status"], "unknown", "{key}");
            }
            f.start();
            let mut end = f.end();
            end["name"] = json!("codex/fixture/other");
            f.put_end(&end);
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn duplicate_bootstrap_identity_refuses_before_selected_directory_walks() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let raw = serde_json::to_string(
                &json!({"node_id":id(1),"catalog":{"schema":1,"swarm_id":id(9),"operations":{}}}),
            )
            .unwrap();
            write(
                &f.locations.identity.join("catalog.json"),
                raw.replacen("\"node_id\":", "\"node_id\":null,\"node_id\":", 1)
                    .as_bytes(),
            );
            let mut counts = Counts::default();
            assert!(observe_at(&f.request, &f.locations, &mut counts).is_err());
            assert_eq!(counts.walks, 2);
            write(
                &f.locations.identity.join("catalog.json"),
                &vec![b' '; RAW + 1],
            );
            let mut counts = Counts::default();
            assert!(observe_at(&f.request, &f.locations, &mut counts).is_err());
            assert_eq!(counts.walks, 2);
        }
        #[test]
        fn duplicate_sources_and_combined_selected_or_per_leaf_bounds_refuse() {
            let f = Fixture::new();
            f.start();
            let raw = serde_json::to_string(&f.binding()).unwrap().replacen(
                "\"run_id\":",
                "\"run_id\":null,\"run_id\":",
                1,
            );
            write(&f.binding_path(), raw.as_bytes());
            assert_eq!(f.read().0["status"], "unknown");
            let mut raw = serde_json::to_vec(&f.binding()).unwrap();
            raw.resize(RAW, b' ');
            write(&f.binding_path(), &raw);
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.binding_path()).unwrap();
            write(&f.end_path(), &vec![b' '; SMALL + 1]);
            assert_eq!(f.read().0["status"], "unknown");
            assert!(unique(br#"{"opaque":{"x":1,"x":2}}"#).is_err());
        }
        #[test]
        fn unsafe_link_directory_permissions_and_fifo_never_become_a_binding() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            fs::set_permissions(f.binding_path(), fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::set_permissions(f.binding_path(), fs::Permissions::from_mode(0o600)).unwrap();
            let other = f.locations.state.join("other");
            fs::hard_link(f.binding_path(), &other).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(&other).unwrap();
            fs::rename(f.binding_path(), &other).unwrap();
            symlink(&other, f.binding_path()).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.binding_path()).unwrap();
            private_dir(&f.binding_path());
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_dir(f.binding_path()).unwrap();
            use std::os::unix::ffi::OsStrExt;
            let fifo = std::ffi::CString::new(f.binding_path().as_os_str().as_bytes()).unwrap();
            // SAFETY: the fixture owns this NUL-terminated path; no existing entry.
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn actual_file_and_held_root_replacements_refuse_recheck() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let mut counts = Counts::default();
            let root = Root::pin(&f.locations.state, &mut counts).unwrap();
            let image = capture(&root, &f.leaf(), RAW, &mut ByteBudget::new(), &mut counts)
                .unwrap()
                .unwrap();
            let mut changed = f.binding();
            changed["opaque"] = json!("changed");
            f.put_binding(&changed);
            assert!(recheck(
                &root,
                &f.leaf(),
                Some(&image),
                &mut ByteBudget::new(),
                &mut counts
            )
            .is_err());
            let moved = f.locations.state.with_extension("moved");
            fs::rename(&f.locations.state, &moved).unwrap();
            private_dir(&f.locations.state);
            assert!(root.finish(&mut counts).is_err());
        }
        #[test]
        fn joined_leaf_and_path_limits_refuse_before_metadata_or_open() {
            let mut counts = Counts::default();
            let huge = PathBuf::from(format!("/{}", "x".repeat(4096)));
            assert!(path_stamp(&huge, &mut counts).is_err());
            assert_eq!(counts.metadata, 0);
            assert_eq!(counts.opens, 0);
            let deep = PathBuf::from(format!("/{}", vec!["x"; 33].join("/")));
            assert!(path_stamp(&deep, &mut counts).is_err());
            assert_eq!(counts.metadata, 0);
            assert_eq!(counts.opens, 0);
            let f = Fixture::new();
            let root = Root::pin(&f.locations.state, &mut counts).unwrap();
            let before = counts.opens;
            assert!(root.open(&"x".repeat(4096), &mut counts).is_err());
            assert_eq!(counts.opens, before);
        }
        #[test]
        fn large_siblings_do_not_grow_indexed_open_counts_and_hard_counters_refuse() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let (_, before) = f.read();
            for n in 0..2048 {
                write(
                    &f.locations.state.join(format!("{n:064x}.json")),
                    b"unrelated",
                );
                write(
                    &f.locations
                        .state
                        .join("launches")
                        .join(format!("unrelated-{n}.json")),
                    b"unrelated",
                );
            }
            let (reply, after) = f.read();
            assert_eq!(reply["status"], "observed_binding");
            assert_eq!(
                (before.walks, before.opens, before.metadata),
                (after.walks, after.opens, after.metadata)
            );
            let mut counts = Counts::default();
            assert!(counts.open(1025).is_err());
            assert_eq!(counts.opens, 0);
            assert!(counts.meta(4097).is_err());
            assert_eq!(counts.metadata, 0);
            for _ in 0..16 {
                counts.walk(Path::new("/")).unwrap();
            }
            let old = counts.opens;
            assert!(counts.walk(Path::new("/")).is_err());
            assert_eq!(counts.opens, old);
            assert!(valid_path(Path::new("/safe/../other")).is_err());
            assert!(valid_path(Path::new("relative")).is_err());
            assert!(valid_path(&PathBuf::from(format!("/{}", vec!["x"; 33].join("/")))).is_err());
        }
    }
}

#[cfg(test)]
mod argument_tests {
    use super::*;
    use std::ffi::OsString;
    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }
    fn os(words: &[String]) -> Vec<OsString> {
        std::iter::once(OsString::from("arbitrary argv0"))
            .chain(words.iter().map(OsString::from))
            .collect()
    }
    #[test]
    fn os_probe_and_typed_reserved_positions_share_the_exact_predicate() {
        let rows = [
            vec![COMMAND],
            vec!["--help", CAPABILITY],
            vec!["--help", CAPABILITY, "extra"],
            vec![
                "--expect-machine",
                "M",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec![
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec!["--expect-machine", COMMAND],
            vec![
                "--expect-machine",
                "M",
                "--client",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec![
                "@target",
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec!["@target", "--help", CAPABILITY],
            vec!["dsh", COMMAND],
            vec!["a", COMMAND],
            vec!["--help", "other", COMMAND],
            vec![
                "--expect-machine",
                "M",
                "--client",
                COMMAND,
                "dsh",
                "/fixture",
            ],
            vec!["@target", "dsh", COMMAND],
            vec!["dsh", "--client", COMMAND],
        ];
        for row in rows {
            let args = strings(&row);
            let remote = args.first().is_some_and(|arg| arg.starts_with('@')) && reserved(&args, 1);
            let expected = remote || reserved(&args, 0);
            let selected = os_arguments(os(&args));
            assert_eq!(selected.is_some(), expected, "fixture index parity");
            if expected && !remote {
                assert_eq!(selected.unwrap().unwrap(), args);
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn invalid_os_identity_origin_launch_trailing_and_argv0_never_leak() {
        use std::os::unix::ffi::OsStringExt;
        for words in [
            strings(&[
                "--expect-machine",
                "M",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ]),
            strings(&[
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ]),
        ] {
            let indexes: &[usize] = if words.len() == 6 {
                &[1, 4]
            } else {
                &[1, 3, 6]
            };
            for index in indexes {
                let mut args = os(&words);
                args[index + 1] = OsString::from_vec(vec![0xff]);
                let err = os_arguments(args).unwrap().unwrap_err();
                assert_eq!(err.message, INVALID);
                assert!(err.message.len() + 6 <= 256);
            }
            let mut args = os(&words);
            args.push(OsString::from_vec(vec![0xff]));
            assert_eq!(os_arguments(args).unwrap().unwrap_err().message, INVALID);
            let mut args = os(&words);
            args[0] = OsString::from_vec(vec![0xff]);
            assert_eq!(os_arguments(args).unwrap().unwrap(), words);
        }
        let mut decoy = os(&strings(&["dsh", COMMAND]));
        decoy.push(OsString::from_vec(vec![0xff]));
        assert!(os_arguments(decoy).is_none());
    }
    #[test]
    fn overcap_and_tenth_item_refuse_without_yielding_an_eleventh_argument() {
        use std::{cell::Cell, rc::Rc};
        let words = strings(&[
            "--expect-machine",
            "M",
            COMMAND,
            "--launch-id",
            "ID",
            "--json",
        ]);
        for index in [1, 4] {
            let mut args = os(&words);
            args[index + 1] = OsString::from("X".repeat(257));
            assert_eq!(os_arguments(args).unwrap().unwrap_err().message, INVALID);
        }
        let mut aggregate = os(&strings(&[
            "--expect-machine",
            &"M".repeat(256),
            "--client",
            &"O".repeat(256),
            COMMAND,
            "--launch-id",
            &"I".repeat(256),
            "--json",
        ]));
        assert!(os_arguments(aggregate.clone()).unwrap().is_ok());
        aggregate.push(OsString::from("T".repeat(256)));
        assert_eq!(aggregate.len(), 10); // argv0 plus nine arguments, below ten.
        assert_eq!(
            os_arguments(aggregate).unwrap().unwrap_err().message,
            INVALID
        );
        let mut args = os(&words);
        args.extend((0..4).map(|_| OsString::from("extra")));
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut iter = args.into_iter();
        let bounded = std::iter::from_fn(move || {
            observed.set(observed.get() + 1);
            assert!(
                observed.get() <= 11,
                "eleventh argument after argv0 must stay unread"
            );
            iter.next()
        });
        assert_eq!(os_arguments(bounded).unwrap().unwrap_err().message, INVALID);
        assert_eq!(calls.get(), 11); // argv0 plus the ten-item probe.
        let args = os(&strings(&["--expect-machine", &"M".repeat(257), COMMAND]));
        assert!(os_arguments(args).unwrap().is_err());
    }
}
