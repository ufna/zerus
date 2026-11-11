//! What the Teams views follow, which read is due, and the view each reply
//! gives. Pure and clock-driven, so tests replay replies without `hgs`.
//!
//! A follow names the team list, what waits for the person and at most one
//! watched team: a team tab follows one team, and following another ends the
//! first. A followed read runs every [`POLL`] while the tab is visible; a
//! hidden tab reads nothing. The view is read again after the desktop's own
//! action ([`TeamsModel::refresh`]) and when the project's tasks topic changed
//! ([`TeamsModel::tasks_changed`]); a `teams` watch topic replaces the timer in
//! M4.
//!
//! A reply becomes an update only when the view it projects differs from the
//! last one sent for that key, so an unchanged team costs a read and nothing
//! more. Keys: `list`, `waiting` and `team:<id>`.
use super::read::{TeamError, TeamRead};
use super::view::{self, TeamList, TeamView, WaitingView};
use crate::ComputerId;
use serde::Serialize;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often a followed read runs while the Team tab is visible.
pub const POLL: Duration = Duration::from_secs(10);

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
            Self::Failed { code, message } => {
                serde_json::json!({"code": code, "message": message})
            }
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
        }
        if !self.follow.list {
            self.end("list", &mut out);
        }
        if !self.follow.waiting {
            self.end("waiting", &mut out);
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
    pub fn refresh(&mut self, now: Instant) {
        for due in self.due.values_mut() {
            *due = now;
        }
    }

    /// The project's tasks topic changed: the watched team's status and queue
    /// are due now (a report moves a node's state).
    pub fn tasks_changed(&mut self, now: Instant) {
        for slot in [Slot::Status, Slot::Queue] {
            if let Some(due) = self.due.get_mut(&slot) {
                *due = now;
            }
        }
    }

    /// The next read to run now, if any: the watched team's status first, then
    /// its queue, then the list and what waits for the person. Its slot is in
    /// flight until its reply is applied.
    pub fn next(&mut self, now: Instant) -> Option<TeamRead> {
        if self.hidden {
            return None;
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
        let empty = Value::Null;
        let status = parts.status.as_ref().unwrap_or(&empty);
        let queue = parts.queue.as_ref().unwrap_or(&empty);
        if parts.status.is_none() && parts.queue.is_none() {
            return None;
        }
        Some(view::team(status, queue))
    }

    /// Applies a read's reply: its slot is due again after [`POLL`], and the
    /// views it changed are returned. A reply about a team no longer watched
    /// gives nothing.
    pub fn apply(
        &mut self,
        read: &TeamRead,
        reply: Result<Value, TeamError>,
        now: Instant,
    ) -> Vec<(String, TeamsUpdate)> {
        let mut out = Vec::new();
        let slot = match read {
            TeamRead::List { .. } => Slot::List,
            TeamRead::Waiting { .. } => Slot::Waiting,
            TeamRead::Status { .. } => Slot::Status,
            TeamRead::Queue { .. } => Slot::Queue,
        };
        self.in_flight.remove(&slot);
        match read {
            TeamRead::List { .. } => {
                self.due.insert(slot, now + POLL);
                let update = match reply {
                    Ok(reply) => TeamsUpdate::List(Arc::new(view::list(&reply))),
                    Err(error) => failure(error),
                };
                self.send("list".into(), update, &mut out);
            }
            TeamRead::Waiting { .. } => {
                self.due.insert(slot, now + POLL);
                let update = match reply {
                    Ok(reply) => TeamsUpdate::Waiting(Arc::new(view::waiting(&reply))),
                    Err(error) => failure(error),
                };
                self.send("waiting".into(), update, &mut out);
            }
            TeamRead::Status { team } | TeamRead::Queue { team, .. } => {
                if Some(team.as_str()) != self.watched() {
                    // The slot belongs to the team followed now, which stays
                    // due; this reply is about a team no tab shows.
                    return out;
                }
                self.due.insert(slot, now + POLL);
                if let Ok(reply) = &reply {
                    if self
                        .parts
                        .as_ref()
                        .is_none_or(|(watched, _)| watched != team)
                    {
                        self.parts = Some((team.clone(), Parts::default()));
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
                        None => return out,
                    },
                    Err(error) => failure(error),
                };
                self.send(team_key("team", team), update, &mut out);
            }
        }
        out
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
mod tests {
    use super::*;
    use crate::teams::view::GROUPS;

    const TEAM_A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const TEAM_B: &str = "bbbbbbbb-0000-4000-8000-000000000002";

    fn fixture() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/team-plan/fixtures/team-queue.json"
        )))
        .expect("the shared team fixture parses")
    }

    fn status(team: &str) -> Value {
        let mut reply = fixture();
        reply["team"]["id"] = serde_json::json!(team);
        reply
    }

    fn queue() -> Value {
        fixture()
    }

    fn keys(out: &[(String, TeamsUpdate)]) -> Vec<&str> {
        out.iter().map(|(key, _)| key.as_str()).collect()
    }

    fn kinds(out: &[(String, TeamsUpdate)]) -> Vec<&'static str> {
        out.iter().map(|(_, update)| update.kind()).collect()
    }

    fn due(model: &mut TeamsModel, now: Instant) -> Option<TeamRead> {
        model.next(now)
    }

    fn follow_all(team: &str) -> TeamsFollow {
        TeamsFollow {
            list: true,
            waiting: true,
            team: Some(team.to_owned()),
        }
    }

    /// The last update per key, in the order the keys first changed.
    fn latest(out: Vec<(String, TeamsUpdate)>) -> Vec<(String, TeamsUpdate)> {
        let mut order: Vec<String> = Vec::new();
        let mut last: BTreeMap<String, TeamsUpdate> = BTreeMap::new();
        for (key, update) in out {
            if !last.contains_key(&key) {
                order.push(key.clone());
            }
            last.insert(key, update);
        }
        order
            .into_iter()
            .map(|key| {
                let update = last.remove(&key).expect("listed");
                (key, update)
            })
            .collect()
    }

    fn watched(model: &mut TeamsModel, now: Instant, team: &str) -> Vec<(String, TeamsUpdate)> {
        let mut out = Vec::new();
        let read = TeamRead::Status {
            team: team.to_owned(),
        };
        assert_eq!(due(model, now), Some(read.clone()));
        out.extend(model.apply(&read, Ok(status(team)), now));
        assert_eq!(
            due(model, now),
            Some(TeamRead::Queue {
                team: team.to_owned(),
                after: None,
            })
        );
        out.extend(model.apply(
            &TeamRead::Queue {
                team: team.to_owned(),
                after: None,
            },
            Ok(queue()),
            now,
        ));
        latest(out)
    }

    #[test]
    fn the_follow_set_never_holds_more_than_one_team() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        assert!(model.follow(follow_all(TEAM_A), now).is_empty());
        let first = watched(&mut model, now, TEAM_A);
        assert_eq!(keys(&first), ["team:aaaaaaaa-0000-4000-8000-000000000001"]);
        assert_eq!(
            model.following().team.as_deref(),
            Some(TEAM_A),
            "one team is followed"
        );

        let ended = model.follow(follow_all(TEAM_B), now);
        assert_eq!(keys(&ended), [team_key("team", TEAM_A)]);
        assert!(matches!(ended[0].1, TeamsUpdate::Ended));
        assert_eq!(model.following().team.as_deref(), Some(TEAM_B));
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_B.to_owned()
            }),
            "the new tab's team is read at once, never the old one"
        );
    }

    #[test]
    fn each_followed_view_is_read_once_per_poll() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_A.to_owned()
            })
        );
        model.apply(
            &TeamRead::Status {
                team: TEAM_A.to_owned(),
            },
            Ok(status(TEAM_A)),
            now,
        );
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Queue {
                team: TEAM_A.to_owned(),
                after: None,
            })
        );
        model.apply(
            &TeamRead::Queue {
                team: TEAM_A.to_owned(),
                after: None,
            },
            Ok(queue()),
            now,
        );
        assert_eq!(due(&mut model, now), Some(TeamRead::List { after: None }));
        model.apply(&TeamRead::List { after: None }, Ok(fixture()), now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Waiting { after: None })
        );
        model.apply(&TeamRead::Waiting { after: None }, Ok(fixture()), now);
        assert_eq!(due(&mut model, now), None, "one read per view per poll");
        assert_eq!(model.wait(now), POLL);
        assert_eq!(
            due(&mut model, now + POLL),
            Some(TeamRead::Status {
                team: TEAM_A.to_owned()
            }),
            "every poll reads it again"
        );
    }

    #[test]
    fn a_hidden_tab_reads_nothing() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        model.set_hidden(true, now);
        assert!(model.hidden());
        assert_eq!(due(&mut model, now), None);
        assert_eq!(due(&mut model, now + POLL * 10), None);
        assert_eq!(model.wait(now), POLL);
        model.set_hidden(false, now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_A.to_owned()
            }),
            "shown again, the views are read at once"
        );
    }

    #[test]
    fn the_desktop_action_reads_the_view_again() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        watched(&mut model, now, TEAM_A);
        assert_eq!(due(&mut model, now), Some(TeamRead::List { after: None }));
        model.apply(&TeamRead::List { after: None }, Ok(fixture()), now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Waiting { after: None })
        );
        model.apply(&TeamRead::Waiting { after: None }, Ok(fixture()), now);
        assert_eq!(due(&mut model, now), None, "all read, the poll is next");
        model.refresh(now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_A.to_owned()
            }),
            "an action reads the team again"
        );
    }

    #[test]
    fn a_tasks_topic_change_reads_the_team_again() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        watched(&mut model, now, TEAM_A);
        model.apply(&TeamRead::List { after: None }, Ok(fixture()), now);
        model.apply(&TeamRead::Waiting { after: None }, Ok(fixture()), now);
        assert_eq!(due(&mut model, now), None);
        model.tasks_changed(now);
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_A.to_owned()
            })
        );
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Queue {
                team: TEAM_A.to_owned(),
                after: None,
            })
        );
        assert_eq!(
            due(&mut model, now),
            None,
            "a task change does not re-read the list"
        );
    }

    #[test]
    fn an_unchanged_view_is_not_sent_twice() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        let first = watched(&mut model, now, TEAM_A);
        assert_eq!(first.len(), 1);
        assert_eq!(kinds(&first), ["team"]);
        let read = TeamRead::Status {
            team: TEAM_A.to_owned(),
        };
        let again = model.apply(&read, Ok(status(TEAM_A)), now);
        assert!(again.is_empty(), "the same reply shows nothing new");
        let read = TeamRead::Queue {
            team: TEAM_A.to_owned(),
            after: None,
        };
        let again = model.apply(&read, Ok(queue()), now);
        assert!(again.is_empty());
    }

    #[test]
    fn the_view_carries_the_teams_bar_members_and_groups() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        let out = watched(&mut model, now, TEAM_A);
        let TeamsUpdate::Team(view) = &out[0].1 else {
            panic!("a team view");
        };
        assert_eq!(view.bar.name, "Fixes");
        assert_eq!(view.members.len(), 4);
        assert_eq!(view.groups.len(), GROUPS.len());
    }

    #[test]
    fn a_failure_is_sent_and_the_view_is_not_ended() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        watched(&mut model, now, TEAM_A);
        let read = TeamRead::Status {
            team: TEAM_A.to_owned(),
        };
        let out = model.apply(
            &read,
            Err(TeamError::Refused {
                code: "teams_inactive".into(),
                message: "Teams are off.".into(),
            }),
            now,
        );
        assert_eq!(keys(&out), [team_key("team", TEAM_A)]);
        assert_eq!(out[0].1.kind(), "failed");
        assert_eq!(
            out[0].1.payload(),
            serde_json::json!({"code": "teams_inactive", "message": "Teams are off."})
        );
        assert!(!out[0].1.is_view());
        // The failure is not sent again unchanged.
        let again = model.apply(
            &read,
            Err(TeamError::Refused {
                code: "teams_inactive".into(),
                message: "Teams are off.".into(),
            }),
            now,
        );
        assert!(again.is_empty());
    }

    #[test]
    fn an_unfollowed_view_ends() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        watched(&mut model, now, TEAM_A);
        let out = model.follow(
            TeamsFollow {
                list: false,
                waiting: false,
                team: None,
            },
            now,
        );
        assert_eq!(keys(&out), [team_key("team", TEAM_A)]);
        assert!(matches!(out[0].1, TeamsUpdate::Ended));
        assert_eq!(due(&mut model, now), None);
        assert!(model.following().team.is_none());
        // Nothing ends twice.
        assert!(model
            .follow(
                TeamsFollow {
                    list: false,
                    waiting: false,
                    team: None,
                },
                now
            )
            .is_empty());
    }

    #[test]
    fn a_reply_about_a_team_no_longer_watched_gives_nothing() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        let read = TeamRead::Status {
            team: TEAM_A.to_owned(),
        };
        assert_eq!(due(&mut model, now), Some(read.clone()));
        model.follow(follow_all(TEAM_B), now);
        let out = model.apply(&read, Ok(status(TEAM_A)), now);
        assert!(out.is_empty());
        assert_eq!(
            due(&mut model, now),
            Some(TeamRead::Status {
                team: TEAM_B.to_owned()
            })
        );
    }

    #[test]
    fn a_short_wait_never_exceeds_the_poll() {
        let now = Instant::now();
        let mut model = TeamsModel::default();
        model.follow(follow_all(TEAM_A), now);
        watched(&mut model, now, TEAM_A);
        model.apply(&TeamRead::List { after: None }, Ok(fixture()), now);
        model.apply(&TeamRead::Waiting { after: None }, Ok(fixture()), now);
        assert_eq!(model.wait(now), POLL);
        assert_eq!(model.wait(now + POLL / 2), POLL / 2);
    }
}
