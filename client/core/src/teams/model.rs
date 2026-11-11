//! What the Teams views follow, which read is due, and the view each reply
//! gives. Pure and clock-driven, so tests replay replies without `hgs`.
//!
//! A follow names the team list, what waits for the person and at most one
//! watched team: a team tab follows one team, and following another ends the
//! first. A followed read runs every [`POLL`] while the tab is visible; a
//! hidden tab reads nothing. The view is read again after the desktop's own
//! action ([`TeamsModel::refresh`]) and when the project's tasks topic changed
//! ([`TeamsModel::tasks_changed`]); a change that arrives while its read is in
//! flight is not lost: it marks the slot and the slot is read again at once
//! when the reply lands.
//!
//! A reply becomes an update only when the view it projects differs from the
//! last one sent for that key, so an unchanged team costs a read and nothing
//! more. Keys: `list`, `waiting` and `team:<id>`. One page is kept per key; a
//! page of a key is read on request with [`TeamsModel::page`] and replaces the
//! shown page, and the projected view names the next page's cursor.
use super::read::{TeamError, TeamRead};
use super::view::{self, TeamList, TeamView, WaitingView};
use crate::ComputerId;
use serde::Serialize;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often a followed read runs while the Team tab is visible.
pub const POLL: Duration = Duration::from_secs(10);
/// Page reads that may wait at once (one key each).
const QUEUED: usize = 8;

/// What the Teams views follow on this computer. A team tab follows one team
/// at a time: `team` is a single id, never a set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TeamsFollow {
    /// The teams of this computer.
    pub list: bool,
    /// What waits for the person.
    pub waiting: bool,
    /// The team whose tab is open.
    pub team: Option<String>,
}

/// One change of a Teams view, keyed as the module says.
#[derive(Clone, Debug, PartialEq)]
pub enum TeamsUpdate {
    List(Arc<TeamList>),
    Team(Arc<TeamView>),
    Waiting(Arc<WaitingView>),
    /// The read failed; the view keeps what it showed.
    Failed {
        code: String,
        message: String,
    },
    /// The view is no longer followed; its data is gone.
    Ended,
}

impl TeamsUpdate {
    /// The batch's `kind`: the view's name, `failed` or `ended`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::List(_) => "list",
            Self::Team(_) => "team",
            Self::Waiting(_) => "waiting",
            Self::Failed { .. } => "failed",
            Self::Ended => "ended",
        }
    }

    /// The batch's `payload`: the view model, `{code, message}` or null.
    pub fn payload(&self) -> Value {
        fn value<T: Serialize>(view: &T) -> Value {
            serde_json::to_value(view).unwrap_or(Value::Null)
        }
        match self {
            Self::List(view) => value(&**view),
            Self::Team(view) => value(&**view),
            Self::Waiting(view) => value(&**view),
            Self::Failed { code, message } => serde_json::json!({"code": code, "message": message}),
            Self::Ended => Value::Null,
        }
    }

    /// Whether it carries a view (not a failure or an end).
    pub fn is_view(&self) -> bool {
        !matches!(self, Self::Failed { .. } | Self::Ended)
    }
}

/// The key of a view about one team.
pub fn team_key(kind: &str, team: &str) -> String {
    format!("{kind}:{team}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Status,
    Queue,
    List,
    Waiting,
}

impl Slot {
    fn rank(self) -> u8 {
        match self {
            Self::Status => 0,
            Self::Queue => 1,
            Self::List => 2,
            Self::Waiting => 3,
        }
    }

    fn of(read: &TeamRead) -> Self {
        match read {
            TeamRead::List { .. } => Self::List,
            TeamRead::Waiting { .. } => Self::Waiting,
            TeamRead::Status { .. } => Self::Status,
            TeamRead::Queue { .. } => Self::Queue,
        }
    }
}

/// The replies a watched team's view is built from: its status (the team and
/// its members) and one page of its queue.
#[derive(Clone, Debug, Default)]
struct Parts {
    status: Option<Value>,
    queue: Option<Value>,
}

/// The Teams read model of this computer.
#[derive(Debug, Default)]
pub struct TeamsModel {
    follow: TeamsFollow,
    due: BTreeMap<Slot, Instant>,
    /// The digest of the last update sent per key.
    shown: BTreeMap<String, u64>,
    in_flight: BTreeSet<Slot>,
    /// Slots a refresh or a tasks change marked while their read was in
    /// flight: they are due at once when the reply lands.
    dirty: BTreeSet<Slot>,
    /// Page reads asked for by the view, run before the due ones.
    queued: VecDeque<TeamRead>,
    /// The watched team's replies, dropped when it is no longer watched.
    parts: Option<(String, Parts)>,
    hidden: bool,
}

/// Feeds the serialized payload straight into the hasher, without a string.
struct HashWriter<'a>(&'a mut DefaultHasher);

impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn digest(kind: &str, payload: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    let _ = serde_json::to_writer(HashWriter(&mut hasher), payload);
    hasher.finish()
}

impl TeamsModel {
    pub fn following(&self) -> &TeamsFollow {
        &self.follow
    }

    /// Follows `next` from now on: newly followed reads are due at once, and
    /// views no longer followed end (only the ones that were sent).
    pub fn follow(&mut self, next: TeamsFollow, now: Instant) -> Vec<(String, TeamsUpdate)> {
        let mut out = Vec::new();
        let previous = std::mem::replace(&mut self.follow, next);
        if previous.team != self.follow.team {
            if let Some(old) = &previous.team {
                self.end(&team_key("team", old), &mut out);
            }
            self.parts = None;
            self.due.remove(&Slot::Status);
            self.due.remove(&Slot::Queue);
            self.dirty.remove(&Slot::Status);
            self.dirty.remove(&Slot::Queue);
            self.queued
                .retain(|read| !matches!(read, TeamRead::Queue { .. }));
        }
        if !self.follow.list {
            self.end("list", &mut out);
            self.queued
                .retain(|read| !matches!(read, TeamRead::List { .. }));
        }
        if !self.follow.waiting {
            self.end("waiting", &mut out);
            self.queued
                .retain(|read| !matches!(read, TeamRead::Waiting { .. }));
        }
        let wanted = [
            (Slot::List, self.follow.list),
            (Slot::Waiting, self.follow.waiting),
            (Slot::Status, self.follow.team.is_some()),
            (Slot::Queue, self.follow.team.is_some()),
        ];
        for (slot, on) in wanted {
            if on {
                self.due.entry(slot).or_insert(now);
            } else {
                self.due.remove(&slot);
                self.dirty.remove(&slot);
                self.in_flight.remove(&slot);
            }
        }
        out
    }

    /// Whether the tab is hidden: a hidden tab reads nothing.
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// The window was hidden (`true`) or shown again. Hidden, nothing is read;
    /// shown, every followed read is due at once, so the views are current.
    pub fn set_hidden(&mut self, hidden: bool, now: Instant) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        if !hidden {
            self.refresh(now);
        }
    }

    /// Reads every followed view again at once: after the desktop's own action.
    /// A slot whose read is in flight is marked and read again at once when
    /// that reply lands, so the action never loses its reread.
    pub fn refresh(&mut self, now: Instant) {
        for slot in [Slot::Status, Slot::Queue, Slot::List, Slot::Waiting] {
            if self.due.contains_key(&slot) {
                self.mark(slot, now);
            }
        }
    }

    /// The project's tasks topic changed: the watched team's status and queue
    /// are read again (a report moves a node's state).
    pub fn tasks_changed(&mut self, now: Instant) {
        for slot in [Slot::Status, Slot::Queue] {
            if self.due.contains_key(&slot) {
                self.mark(slot, now);
            }
        }
    }

    /// Due now, or marked when its read is in flight.
    fn mark(&mut self, slot: Slot, now: Instant) {
        if self.in_flight.contains(&slot) {
            self.dirty.insert(slot);
        } else if let Some(due) = self.due.get_mut(&slot) {
            *due = now;
        }
    }

    /// Asks for one page of a followed key by its cursor: `list`, `waiting` or
    /// `team:<id>` (the queue's page). The page replaces the shown one.
    pub fn page(&mut self, key: &str, cursor: &str) -> bool {
        if cursor.is_empty()
            || cursor.len() > super::read::CURSOR_BYTES
            || cursor.starts_with('-')
            || cursor.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return false;
        }
        let read = match key {
            "list" if self.follow.list => TeamRead::List {
                after: Some(cursor.to_owned()),
            },
            "waiting" if self.follow.waiting => TeamRead::Waiting {
                after: Some(cursor.to_owned()),
            },
            _ => {
                let Some(team) = key.strip_prefix("team:") else {
                    return false;
                };
                if self.follow.team.as_deref() != Some(team) {
                    return false;
                }
                TeamRead::Queue {
                    team: team.to_owned(),
                    after: Some(cursor.to_owned()),
                }
            }
        };
        if self.queued.len() >= QUEUED || self.queued.contains(&read) {
            return false;
        }
        self.queued.push_back(read);
        true
    }

    /// The next read to run now, if any: an asked-for page first, then the
    /// watched team's status, its queue, the list and what waits for the
    /// person. Its slot is in flight until its reply is applied.
    pub fn next(&mut self, now: Instant) -> Option<TeamRead> {
        if self.hidden {
            return None;
        }
        let follow = self.follow.clone();
        let in_flight = self.in_flight.clone();
        let wanted = |read: &TeamRead| {
            !in_flight.contains(&Slot::of(read))
                && match read {
                    TeamRead::List { .. } => follow.list,
                    TeamRead::Waiting { .. } => follow.waiting,
                    TeamRead::Status { team } | TeamRead::Queue { team, .. } => {
                        follow.team.as_deref() == Some(team.as_str())
                    }
                }
        };
        if let Some(read) = self.queued.pop_front_if(|read| wanted(read)) {
            let slot = Slot::of(&read);
            self.in_flight.insert(slot);
            self.due.insert(slot, now + POLL);
            return Some(read);
        }
        let watched = self.follow.team.clone();
        let slot = self
            .due
            .iter()
            .filter(|(slot, due)| **due <= now && !self.in_flight.contains(slot))
            .min_by_key(|(slot, due)| (slot.rank(), **due))
            .map(|(slot, _)| *slot)?;
        let read = match slot {
            Slot::List => TeamRead::List { after: None },
            Slot::Waiting => TeamRead::Waiting { after: None },
            Slot::Status => TeamRead::Status {
                team: watched.clone()?,
            },
            Slot::Queue => TeamRead::Queue {
                team: watched?,
                after: None,
            },
        };
        self.in_flight.insert(slot);
        self.due.insert(slot, now + POLL);
        Some(read)
    }

    /// How long until a read is due, at most [`POLL`].
    pub fn wait(&self, now: Instant) -> Duration {
        if self.hidden {
            return POLL;
        }
        if !self.queued.is_empty() {
            return Duration::ZERO;
        }
        self.due
            .iter()
            .filter(|(slot, _)| !self.in_flight.contains(slot))
            .map(|(_, due)| due.saturating_duration_since(now))
            .min()
            .unwrap_or(POLL)
            .min(POLL)
    }

    fn end(&mut self, key: &str, out: &mut Vec<(String, TeamsUpdate)>) {
        if self.shown.remove(key).is_some() {
            out.push((key.to_owned(), TeamsUpdate::Ended));
        }
    }

    fn send(&mut self, key: String, update: TeamsUpdate, out: &mut Vec<(String, TeamsUpdate)>) {
        let payload = update.payload();
        let digest = digest(update.kind(), &payload);
        if self.shown.get(&key) != Some(&digest) {
            self.shown.insert(key.clone(), digest);
            out.push((key, update));
        }
    }

    fn watched(&self) -> Option<&str> {
        self.follow.team.as_deref()
    }

    /// The watched team's view from the replies it has.
    fn team_view(&self) -> Option<TeamView> {
        let (team, parts) = self.parts.as_ref()?;
        if Some(team.as_str()) != self.watched() {
            return None;
        }
        if parts.status.is_none() && parts.queue.is_none() {
            return None;
        }
        let empty = Value::Null;
        let status = parts.status.as_ref().unwrap_or(&empty);
        let queue = parts.queue.as_ref().unwrap_or(&empty);
        Some(view::team(status, queue))
    }

    /// The slot's next due time: at once when a change marked it during the
    /// read, else one [`POLL`] from now.
    fn settled(&mut self, slot: Slot, now: Instant) {
        let due = if self.dirty.remove(&slot) {
            now
        } else {
            now + POLL
        };
        self.due.insert(slot, due);
    }

    /// Applies a read's reply: its slot is due again after [`POLL`] (or at
    /// once when a change marked it), and the views it changed are returned.
    /// A reply about a view no longer followed gives nothing.
    pub fn apply(
        &mut self,
        read: &TeamRead,
        reply: Result<Value, TeamError>,
        now: Instant,
    ) -> Vec<(String, TeamsUpdate)> {
        let mut out = Vec::new();
        let slot = Slot::of(read);
        self.in_flight.remove(&slot);
        match read {
            TeamRead::List { .. } => self.apply_list(slot, reply, now, &mut out),
            TeamRead::Waiting { .. } => self.apply_waiting(slot, reply, now, &mut out),
            TeamRead::Status { team } | TeamRead::Queue { team, .. } => {
                self.apply_team(read, slot, team, reply, now, &mut out)
            }
        }
        out
    }

    fn apply_list(
        &mut self,
        slot: Slot,
        reply: Result<Value, TeamError>,
        now: Instant,
        out: &mut Vec<(String, TeamsUpdate)>,
    ) {
        if !self.follow.list {
            return;
        }
        self.settled(slot, now);
        let update = match reply {
            Ok(reply) => TeamsUpdate::List(Arc::new(view::list(&reply))),
            Err(error) => failure(error),
        };
        self.send("list".into(), update, out);
    }

    fn apply_waiting(
        &mut self,
        slot: Slot,
        reply: Result<Value, TeamError>,
        now: Instant,
        out: &mut Vec<(String, TeamsUpdate)>,
    ) {
        if !self.follow.waiting {
            return;
        }
        self.settled(slot, now);
        let update = match reply {
            Ok(reply) => TeamsUpdate::Waiting(Arc::new(view::waiting(&reply))),
            Err(error) => failure(error),
        };
        self.send("waiting".into(), update, out);
    }

    fn apply_team(
        &mut self,
        read: &TeamRead,
        slot: Slot,
        team: &str,
        reply: Result<Value, TeamError>,
        now: Instant,
        out: &mut Vec<(String, TeamsUpdate)>,
    ) {
        if Some(team) != self.watched() {
            // The slot belongs to the team followed now, which stays due; this
            // reply is about a team no tab shows.
            return;
        }
        self.settled(slot, now);
        if let Ok(reply) = &reply {
            if self
                .parts
                .as_ref()
                .is_none_or(|(watched, _)| watched != team)
            {
                self.parts = Some((team.to_owned(), Parts::default()));
            }
            if let Some((_, parts)) = self.parts.as_mut() {
                match read {
                    TeamRead::Status { .. } => parts.status = Some(reply.clone()),
                    _ => parts.queue = Some(reply.clone()),
                }
            }
        }
        let update = match reply {
            Ok(_) => match self.team_view() {
                Some(view) => TeamsUpdate::Team(Arc::new(view)),
                None => return,
            },
            Err(error) => failure(error),
        };
        self.send(team_key("team", team), update, out);
    }
}

fn failure(error: TeamError) -> TeamsUpdate {
    TeamsUpdate::Failed {
        code: error.code().to_owned(),
        message: error.message().to_owned(),
    }
}

/// A change as the batch takes it: its key, its update and its payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub key: String,
    pub update: TeamsUpdate,
    pub payload: Arc<Value>,
}

impl Sent {
    pub fn new(key: String, update: TeamsUpdate) -> Self {
        let payload = Arc::new(update.payload());
        Self {
            key,
            update,
            payload,
        }
    }
}

impl From<(String, TeamsUpdate)> for Sent {
    fn from((key, update): (String, TeamsUpdate)) -> Self {
        Self::new(key, update)
    }
}

/// The computer a change belongs to, with the model's payload computed once.
pub fn change(computer: &ComputerId, sent: Sent) -> crate::Change {
    crate::Change::Teams {
        computer: computer.clone(),
        key: sent.key,
        update: sent.update,
        payload: sent.payload,
    }
}

#[cfg(test)]
mod tests;
