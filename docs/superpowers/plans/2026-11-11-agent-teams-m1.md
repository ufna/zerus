# Agent Teams M1 (queue team) implementation plan

> **For agentic workers:** this plan is executed package by package. One
> implementer session owns one package in its own worktree; an independent
> reviewer checks it; the coordinating session merges it into
> `integrate/teams`. Work test-first. Read the specification before your
> package.

**Goal:** a person creates a standing team from a template and sends it tasks;
each task runs through the team's pipeline and accepted work lands on one
integration branch, on one computer, behind a setting that is off by default.

**Architecture:** a pure crate (`crates/team-plan`) holds the types and every
decision about what may run. `src/teams` holds the store, the operations, the
catalog, integration and the CLI. A teams actor in the desktop service
(`src/desktop/teams`) causes every effect in sessions through existing service
actors. The desktop reads typed projections from `client/core/src/teams`.

**Tech stack:** Rust 1.99 (`cargo +1.99.1`), SQLite through `src/sqlite_file`,
serde and serde_json, Git plumbing, SolidJS and vitest in `client/`.

**Specification:** `docs/superpowers/specs/2026-11-11-agent-teams-design.md`.
Epic: `zerus-63rru`.

## Global constraints

- Teams are inactive unless the preview setting is on. With it off there is no
  store file, `hgs team` answers `teams_inactive`, the desktop shows no entry
  point, and no code path of an ordinary session changes.
- A team task is a Zerus task. The replicated task schema is not raised and no
  new `Target` variant is added. The only replica change is the optional
  `team` key in a task's field map (package 6).
- Only `crates/team-plan` decides what may run. Only the teams actor causes
  effects in sessions, and only through existing service actors. An operation
  in `src/teams/ops` never touches a session.
- An intent is recorded before an effect and its outcome after. An unknown
  outcome is never retried.
- Session identity is the computer UUID with `session_id` or `launch_id`.
  A session name is never used to identify a session.
- Every decision, card and journal entry records an actor with a person
  identifier (`"local"` today). Authority is checked in one function.
- Reads are bounded: rows and pages, never a whole team in one value.
- Zerus executes nothing that a repository or a template declares.
- Integration never runs `git reset`, never writes a checkout and never
  pushes.
- The repository language is English. No address, home path or absolute
  workspace path in source, tests or tracker text. Fixtures use synthetic paths
  and reserved example domains.
- The desktop UI never separates values with dots
  (`scripts/check-ui-separators.py`), and the client entry bundle stays under
  500,000 bytes (`client/scripts/check-bundle-size.py`).
- Credit: a file imported unchanged from `WhiteWh/zerus` at `ca4cc4481` is
  committed with its fork author as the Git author (take the identity from
  `git log -1 --format='%an <%ae>' whitewh/whitewh/main -- <path>`), with a
  message naming the source commit and path. Its adaptation is a second,
  ordinary commit. New code carries no co-author trailer for the fork.

## Rules for every package

1. Work only in your own worktree and branch, created from `integrate/teams`.
   Change only the files your package owns; if you need a change elsewhere,
   say so in your report instead of making it.
2. Tests first: write the failing test, see it fail, implement, see it pass.
3. Commit locally in small commits. The repository owner explicitly authorized
   local commits for this work on 2026-11-11; pushing and merging stay with
   the coordinating session. Never push.
4. Run, for each crate you touched, with `--manifest-path` and never at the
   repository root: `cargo +1.99.1 fmt --check`, `cargo +1.99.1 clippy
   --locked --all-targets -- -D warnings`, `cargo +1.99.1 test --locked`. For
   the `hgs` binary run only the tests of your module (`cargo +1.99.1 test
   --locked --bin hgs teams::`), not the whole suite. After changing a
   `crates/*` API also run `cargo +1.99.1 check --locked --bin hgs`.
5. The computer is shared. Keep build output inside your worktree, use at most
   eight build jobs (`-j 8`), start no stress loops, and do not use the Mac or
   the Windows computer.
6. Do not start, stop, kill or message sessions other than the coordinating
   session. Do not run `hgs` commands that change sessions. Tests that need
   `hgs` state use a temporary `HGS_CONFIG_DIR` and `HGS_STATE_DIR`.
7. Report to the coordinating session with: the branch and head commit, what
   you built, the exact commands you ran with pass and fail counts, the path
   and SHA-256 of the raw test log inside your worktree, and what is not done
   or uncertain. A result without a log counts as unverified.

## Review focus

Conditions the specification implies and that are most likely to hurt a
person using Teams. Each is pinned by a test in the package named.

1. The desktop service restarts between recording a hand-out intent and its
   outcome: the task is not sent again and the lead gets an inbox item
   (package 8).
2. Two tasks are ready and one member is free: exactly one is handed out, and
   a member with a task in progress outside the team is not free (packages 2
   and 8).
3. A task is sent to the team twice, or with the same request ID: one plan
   node exists (packages 3 and 6).
4. A reviewer is the only member of its role and it implemented the task: the
   stage is not handed to it, and the task goes to the lead with the reason
   (packages 2 and 9).
5. The integration ref moved between computing a merge and updating the ref:
   nothing is lost and the merge is computed again; a conflicting merge
   changes no ref and no reachable object (package 11).

## File structure

| Path | Responsibility | Package |
|---|---|---|
| `crates/team-plan/src/lib.rs` | Re-exports | 1 |
| `crates/team-plan/src/model.rs` | Team, member, phase, node and settings types | 1 |
| `crates/team-plan/src/catalog.rs` | Role, seat, pipeline, template, catalog, bundle types | 1 |
| `crates/team-plan/src/problem.rs` | `Problem` and the error codes | 1 |
| `crates/team-plan/src/graph.rs` | Cycles, layers, dependants (imported kernel) | 2 |
| `crates/team-plan/src/ready.rs` | Readiness, hand-out order, member choice | 2 |
| `crates/team-plan/src/overlap.rs` | Write-set overlap | 2 |
| `crates/team-plan/src/pipeline.rs` | Stage progression | 2 |
| `crates/team-plan/src/accounts.rs` | Choosing an account | 2 |
| `crates/team-plan/src/validate.rs` | Catalog and bundle validation | 4 |
| `crates/team-plan/src/builtin.rs` | The built-in catalog | 4 |
| `crates/team-plan/fixtures/*.json` | Shared fixtures | 1, then their owners |
| `src/teams/mod.rs` | Module list and the preview setting | 1 |
| `src/teams/store/` | SQLite store | 3 |
| `src/teams/ops/` | Operations and the authority function | 3, 9, 10 |
| `src/teams/catalog.rs` | A person's catalog on disk, save, import, export | 4 |
| `src/teams/integrate.rs` | Git plumbing for the integration branch | 11 |
| `src/teams/cli.rs` | `hgs team` | 5 |
| `src/agent_guide/team.md`, `team-lead.md` | Guide pages | 5 |
| `src/desktop/teams/` | Member launch, teams actor, standing confirmations | 7, 8, 17 |
| `client/core/src/teams/` | Projections and actions | 12 |
| `client/src/ui/teams/`, `client/src/store/teams*.ts` | Frontend | 13, 14 |

## Contracts (package 1 materializes these exactly)

All types derive `Clone, Debug, PartialEq, Serialize, Deserialize`. Structs
use `#[serde(deny_unknown_fields)]`; enums use `#[serde(rename_all =
"snake_case")]`. Optional and empty fields are skipped when serialized and
default when absent.

```rust
// crates/team-plan/src/model.rs
pub type TeamId = String;    // lowercase UUID
pub type MemberId = String;  // "lead", "m1", "m2", unique in a team
pub type NodeId = u32;       // counter per team, starts at 1
pub type PersonId = String;  // "local" today

pub struct TaskRef { pub project: String, pub uuid: String }
pub struct SessionKey {
    pub machine: String,          // computer UUID
    pub session_id: String,       // "launch_id:<uuid>" or "run_id:<id>"
}

pub enum TeamStatus { Draft, Planning, Running, Paused, Finished, Stopped, Archived }
pub enum MemberState { NotCreated, Active, Paused, Removed }
pub enum PhaseKind { Plan, Queue }
pub enum PhaseMode { Standard, Free }
pub enum PhaseState { Draft, Approved, AwaitingAcceptance, Done, Rejected }
pub enum Approval { Review, Auto }
pub enum Acceptance { Lead, Person, Pipeline }
pub enum DepKind { Hard, Soft }
pub enum NodeKind { Task, Check, Stage }
pub enum Severity { None, Crit, Minor }
pub enum NodeSource { Planned, Sent }
pub enum NodeState { Draft, Waiting, Pending, InProgress, Reported, Accepted, Cancelled }
pub enum Decider { Lead, Person, Pipeline }
pub enum Routing { Direct, Lead }
pub enum Integration { Branch, None }
pub enum ContextMode { Keep, Fresh }

pub struct Delegations {
    pub accept_results: bool, pub answer_members: bool, pub confirm_trust: bool,
    pub resume_exited: bool, pub merge_accepted: bool,
}                                  // Default: all true
pub struct Caps {
    pub max_members: u32,          // 6
    pub max_phase_tasks: u32,      // 50
    pub max_open_tasks: u32,       // 500
    pub max_open_questions: u32,   // 3
}
pub struct Timers {
    pub report_timeout_seconds: u32,   // 120
    pub start_timeout_seconds: u32,    // 180
    pub inbox_interval_seconds: u32,   // 120
    pub remind_lead_minutes: u32,      // 15
    pub remind_person_minutes: u32,    // 60
    pub lead_stall_minutes: u32,       // 20
    pub answer_escalate_minutes: u32,  // 10
    pub resume_limit_per_hour: u32,    // 3
}
pub struct Settings {
    pub delegations: Delegations, pub caps: Caps, pub timers: Timers,
    pub queue: bool,                 // true
    pub routing: Routing,            // Direct
    pub plan_approval: Approval,     // Review
    pub phase_acceptance: Acceptance,// Lead; Pipeline is refused here
    pub integration: Integration,    // Branch
    pub land_allowed: bool,          // false
}

pub struct Team {
    pub id: TeamId, pub project: String, pub name: String, pub goal: String,
    pub home: String,                // computer UUID
    pub owner: PersonId, pub status: TeamStatus, pub lead: MemberId,
    pub settings: Settings, pub bundle: TemplateBundle,
    pub integration_branch: String, pub base_commit: String, pub revision: u64,
}
pub struct Member {
    pub id: MemberId, pub role: String, pub agent: String, pub account: String,
    pub state: MemberState, pub session: Option<SessionKey>,
    pub worktree: String, pub ownership_nonce: String,
}
pub struct Phase {
    pub n: u32, pub order: u32, pub title: String, pub kind: PhaseKind,
    pub mode: PhaseMode, pub approval: Approval, pub state: PhaseState,
    pub acceptance: Acceptance, pub barrier: bool, pub paused: bool,
}
pub struct Dep { pub node: NodeId, pub kind: DepKind }
pub struct Node {
    pub id: NodeId, pub phase: u32, pub task: Option<TaskRef>,
    pub title: String, pub text: String,      // text only while a draft
    pub source: NodeSource, pub deps: Vec<Dep>,
    pub assignee: Option<MemberId>, pub role: String, pub pipeline: String,
    pub writes: Vec<String>, pub estimate_minutes: u32,
    pub kind: NodeKind, pub severity: Severity, pub origin: Option<NodeId>,
    pub stage_of: Option<NodeId>,   // the implementing node, for a stage
    pub stage_index: u32,           // position in the pipeline, for a stage
    pub round: u32,                 // 1 on the first pass
    pub implementer: Option<MemberId>,
    pub hold: bool, pub admitted: bool,
    pub state: NodeState,           // derived by the store from the task
    pub decided_by: Option<Decider>, pub integrated: bool,
    pub rank: String,               // the task's board rank; queue order
}
```

`NodeState` is derived, never stored as truth: `Draft` has no task;
`Waiting` is a queue node that is not admitted or not routed yet; `Pending`
is To do; `InProgress` is In progress; `Reported` is In review; `Accepted` is
Done; `Cancelled` is a cancelled task.

```rust
// crates/team-plan/src/catalog.rs
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AccountChoice { Freest, Named { account: String } }   // Default: Freest
pub enum StageKind { Review, Test, Land }
pub struct Role {
    pub id: String, pub name: String, pub prompt: String,
    pub agent: String,              // "claude", "codex", "kimi", "dsh", "opencode"
    pub account: AccountChoice, pub model: String, pub effort: String,
    pub pipeline: String,           // empty: the template's default
    pub context: ContextMode,       // Keep
}
pub struct Seat { pub role: String, pub count: u32, pub optional: bool }
pub struct Stage { pub kind: StageKind, pub role: String }
pub struct Pipeline {
    pub id: String, pub name: String, pub implementer: String, // a role id
    pub stages: Vec<Stage>, pub max_rounds: u32,               // 0 reads as 3
    pub accept: Acceptance,
}
pub struct LeadSpec {
    pub agent: String, pub account: AccountChoice,
    pub model: String, pub effort: String, pub rules: String,
}
pub struct Template {
    pub id: String, pub name: String, pub description: String,
    pub lead: LeadSpec, pub roster: Vec<Seat>, pub default_pipeline: String,
    pub settings: Settings,
}
pub struct Catalog {
    pub roles: Vec<Role>, pub pipelines: Vec<Pipeline>,
    pub templates: Vec<Template>, pub planning_rules: Vec<String>,
}
pub struct TemplateBundle {         // an exported template; a team's copy
    pub template: Template, pub roles: Vec<Role>, pub pipelines: Vec<Pipeline>,
}
```

```rust
// crates/team-plan/src/problem.rs
pub struct Problem { pub code: String, pub message: String, pub refs: Vec<String> }
```

Error codes, stable strings: `teams_inactive`, `not_found`,
`stale_revision`, `not_allowed`, `invalid_state`, `unknown_role`,
`unknown_pipeline`, `unknown_member`, `unknown_account`,
`no_account_available`, `no_seat_for_stage`, `dependency_cycle`,
`over_cap`, `duplicate`, `unknown_field`, `integration_conflict`,
`integration_unavailable`, `delivery_uncertain`.

Functions other packages rely on (package 2 unless marked):

```rust
// graph.rs
pub fn cycles(nodes: &[Node]) -> Vec<Vec<NodeId>>;
pub fn layers(nodes: &[Node]) -> BTreeMap<NodeId, u32>;
pub fn dependants(nodes: &[Node]) -> BTreeMap<NodeId, u32>;   // transitive count
// overlap.rs
pub fn overlaps(a: &[String], b: &[String]) -> bool;
// ready.rs
pub struct MemberView { pub id: MemberId, pub role: String, pub state: MemberState, pub free: bool }
pub struct PlanView<'a> {
    pub phases: &'a [Phase], pub nodes: &'a [Node],
    pub members: &'a [MemberView], pub lead: &'a str,
    pub integration: Integration,
}
pub struct Handout { pub node: NodeId, pub member: MemberId }
pub fn ready(view: &PlanView) -> Vec<NodeId>;
pub fn handouts(view: &PlanView) -> Vec<Handout>;   // at most one per member
// pipeline.rs
pub enum Step {
    Stage { index: u32, kind: StageKind, role: String },
    ToLead { reason: String }, ToPerson, Accept,
    ReturnToImplementer { round: u32 },
}
pub fn after_report(node: &Node, pipeline: &Pipeline) -> Step;
pub fn after_verdict(node: &Node, stage: &Node, pass: bool, pipeline: &Pipeline) -> Step;
// accounts.rs
pub struct UsageWindow { pub used_percent: f64, pub resets_at: Option<i64> }
pub struct AccountUsage {
    pub id: String, pub agent: String, pub signed_in: bool,
    pub windows: Vec<UsageWindow>,
}
pub fn pick_account(agent: &str, choice: &AccountChoice, accounts: &[AccountUsage],
    taken: &BTreeMap<String, u32>) -> Result<String, Problem>;
// validate.rs, builtin.rs (package 4)
pub fn validate_catalog(catalog: &Catalog) -> Vec<Problem>;
pub fn validate_bundle(bundle: &TemplateBundle, caps: &Caps) -> Vec<Problem>;
pub fn bundle(catalog: &Catalog, template: &str) -> Result<TemplateBundle, Problem>;
pub fn builtin() -> Catalog;
```

Readiness (`ready`): the node is `Pending`; not held; its phase is `Approved`
and not paused, or it is a queue node that is admitted; no earlier phase is
`AwaitingAcceptance`; a barrier phase has no undecided node in an earlier
phase; every hard dependency is `Accepted` and `integrated`; every soft
dependency is at least `InProgress`; a cancelled dependency releases; no node
that is `InProgress` or `Reported` has an overlapping write set. In a team
with no integration branch a hard dependency needs only `Accepted`:
`PlanView` carries the team's `integration` mode. A stage node is never a
dependency. A node whose phase or dependency is missing from the view is not
ready, so the store supplies the rows of archived dependencies.

Hand-out (`handouts`): order is stage nodes first, then plan nodes by most
transitive dependants and then by ID, then queue nodes by `rank`. A member
qualifies when it is `Active` and `free`, is not the lead, has the node's
role, and is the assignee when one is named. A stage node is never given to
its `implementer`. Each member gets at most one node per call; a node with no
qualifying member is skipped.

Choosing an account (`pick_account`): for `Named`, the account must exist,
match the agent and be signed in, else `unknown_account`. For `Freest`: take
signed-in accounts of the agent; drop any with a window at 100 percent or
more; headroom is 100 minus the highest `used_percent`; highest headroom
wins, then the lower `taken` count, then the lower ID. Accounts with no
windows are used only when no account with windows qualifies. Nothing left
gives `no_account_available`; its `refs` hold the earliest time an account
becomes usable again: for each account the latest reset among its full
windows, then the earliest of those across accounts.

Operation envelope, used by the CLI and by client core: a request is a JSON
object `{"request_id": "<uuid>", "team": "<id>", "expected_revision": <n>,
...}` with the operation's own fields; a reply is `{"ok": true, "revision":
<n>, ...}` or `{"ok": false, "error": {"code", "message", "refs"}}`. The
same `request_id` with the same content returns the first reply.

## Packages

Each package lists what it owns, what it must deliver and the tests that
prove it. Packages 5, 6, 7 and 12 are in the addendum below; the rest follow
when their dependencies are merged.

### Package 1: contracts

**Owns:** `crates/team-plan` (new crate, package `zerus-team-plan`),
`src/teams/mod.rs`, the `team` entry in the command dispatch of `src/cli.rs`,
the lines in `Cargo.toml`,
`client/core/Cargo.toml`, the lock files and
`.github/workflows/checks.yml` that add the crate.

**Deliver:**

- The crate with `model.rs`, `catalog.rs`, `problem.rs` exactly as in
  Contracts, with `Default` implementations giving the values in the
  comments. Wire it the way `crates/watch-protocol` is wired, with its own
  `Cargo.lock`, serde and serde_json only, and add its fmt, clippy and test
  lines to `checks.yml` next to that crate's.
- `crates/team-plan/fixtures/catalog-builtin.json` (three templates, four pipelines and
  their roles as the specification's tables say), `team-queue.json` (a
  running team of a lead, two developers and a reviewer with six queue nodes
  in different states) and `bundle-bugfix.json`.
- `src/teams/mod.rs`: `pub fn enabled(state_dir: &Path) -> bool`, true only
  when the file `teams/preview` exists in the `hgs` state directory, and
  `pub const INACTIVE: &str = "teams_inactive"`. Register an `hgs team`
  command that answers `{"ok":false,"error":{"code":"teams_inactive",...}}`
  and a non-zero exit when it is off, and `hgs team preview on|off|status`
  that creates or removes the file. `preview on|off` is refused when the
  caller has `HGS_SESSION` and `HGS_RUN_ID` set.

**Tests:** every type round-trips through JSON; an unknown field is refused;
absent optional fields default; each fixture parses into its type; the
defaults equal the documented values; with the setting off no file is created
under the state directory by any `hgs team` call; `preview on` from a session
environment is refused.

### Package 2: plan kernel

**Owns:** `graph.rs`, `ready.rs`, `overlap.rs`, `pipeline.rs`, `accounts.rs`
in `crates/team-plan/src`.

**Deliver:** the functions in Contracts. `graph.rs` starts as an import: copy
`src/state/teams/plan.rs` from `whitewh/whitewh/main` unchanged into
`crates/team-plan/src/imported_plan.rs` in a commit authored as the Global
constraints say, then in a second commit keep only the graph kernel (cycle
detection, longest-path layers, transitive dependants, dispatch order),
adapted to `Node`, renamed to `graph.rs`, with its tests. Do the same for the
write-set overlap helpers of `src/state/teams/model.rs` into `overlap.rs`,
without the hardcoded container folders.

**Tests:** cycles of one, two and three nodes; layers of a diamond;
dependants of a chain; each readiness condition, one test per condition, each
failing when the condition is removed; Review focus 2 and 4; hand-out order
(stage before plan before queue, rank order inside the queue); `pick_account`
for each rule in Contracts, including a tie and nothing available;
`after_report` and `after_verdict` for each built-in pipeline; with three
rounds, a return in round 1 or 2 goes to the implementer and the return in
round 3 goes to the lead.

### Package 3: team store and operations

**Owns:** `src/teams/store/`, `src/teams/ops/`.

**Deliver:**

- A private SQLite file `teams/teams-v1.sqlite3` under the `hgs` state
  directory, created on the first team, opened through `src/sqlite_file`,
  with `PRAGMA user_version` and a refusal to open a newer version. Tables
  for team, member, phase, node, dependency, lead inbox, owner card, journal
  and effect intent; indexes for every read below; no JSON blob holding a
  whole team.
- `ops::apply(store, actor, request, now_ms) -> Result<Reply, Problem>`: one
  transaction with the revision check and one journal entry per operation;
  idempotent by `request_id`.
- `ops::authority(actor, operation, team) -> Result<(), Problem>`: the only
  place that decides who may do what, from the specification's authority
  table.
- Operations of M1: create team from a bundle; set member session and state;
  pause, resume, stop, archive (team and member); add a task to the queue;
  admit; route; hold, release, cancel; record hand-out intent and outcome;
  record report; accept, return, reassign; ask and reply; open and decide an
  owner card; change settings.
- Reads: team by ID; members; the queue page by rank; nodes by state; inbox
  page; cards page; journal page by sequence. Every list takes a limit of at
  most 200 and a cursor.
- Decided queue nodes move to an archive table and do not count toward
  `max_open_tasks`.

**Tests:** each operation's success, its authority refusal and its
invalid-state refusal; idempotency by `request_id`; Review focus 3; stale
revision; paging stays stable while rows are added; a newer `user_version`
is refused; 1,000 decided queue nodes leave the live queue page unchanged.

### Package 4: catalog and templates

**Owns:** `validate.rs` and `builtin.rs` in `crates/team-plan/src`,
`src/teams/catalog.rs`.

**Deliver:** `builtin()` equal to `crates/team-plan/fixtures/catalog-builtin.json`;
validation (unknown role or pipeline, a pipeline stage or implementer role
with no seat, duplicate IDs, a roster over `max_members`, `Pipeline`
acceptance used as a phase acceptance); a person's catalog as one JSON file
in the `hgs` configuration directory, merged over the built-in one by ID;
save, import and export of a `TemplateBundle`. The catalog model starts as an
import of `src/state/teams/catalog.rs` from the fork, in the two-commit form,
reduced to roles, seats, pipelines and templates.

**Tests:** each validation rule; built-in equals the fixture; an imported
bundle is complete on an empty catalog; an unknown field in a file is refused
with `unknown_field` and the file is left untouched; a person's entry
overrides a built-in one with the same ID.

### Packages 5, 6 and 7

Their briefs are in the addendum for wave 2 below.

### Package 8: teams actor

**Owns:** `src/desktop/teams/actor.rs` and its effect ports.

**Deliver:** event intake (store revision, watch stream), `handouts`, intent
then the task attempt writer then outcome, doorbells through deferred send,
one safety check a minute. Effects sit behind traits so tests use fakes.
Review focus 1 and 2.

### Package 9: pipelines

**Owns:** stage operations in `src/teams/ops/`, the actor's stage step.

**Deliver:** stage nodes and tasks created at report, `verdict`, rounds,
acceptance by the lead or by the pipeline. Review focus 4.

### Package 10: lead inbox, timers, escalation

**Owns:** inbox and card operations, the actor's timers.

**Deliver:** inbox items for report, question, no report, not started,
stopped member, conflict; coalesced doorbells; reminders and escalation to
owner cards by the `Timers` values; pause and resume.

### Package 11: integration branch

**Owns:** `src/teams/integrate.rs`.

**Deliver:**

```rust
pub struct Repo { pub git_dir: PathBuf }   // the repository's common directory
pub enum Merge {
    Merged { commit: String },
    Conflict { files: Vec<String> },
    Stale,                                  // the ref moved; call again
}
pub fn create_branch(repo: &Repo, branch: &str, base: &str) -> Result<(), Problem>;
pub fn reported_commit(worktree: &Path) -> Result<Reported, Problem>;
pub struct Reported { pub commit: String, pub branch: String, pub dirty: bool }
pub fn descends(repo: &Repo, commit: &str, base: &str) -> Result<bool, Problem>;
pub fn merge(repo: &Repo, branch: &str, commit: &str, message: &str) -> Result<Merge, Problem>;
pub fn sync_base(repo: &Repo, branch: &str, base_branch: &str) -> Result<Merge, Problem>;
```

`merge` reads the branch tip, runs `git merge-tree --write-tree`, creates the
merge commit with `git commit-tree` (signed when `commit.gpgsign` is set) and
advances the ref with `git update-ref <ref> <new> <old>`. It refuses with
`integration_unavailable` when the branch is checked out in any worktree or
Git is older than 2.38. It runs no other Git command that writes, and no
`reset`, `checkout`, `push`, `fetch` or hook.

**Tests,** each on a temporary repository: a clean merge advances the ref and
both parents are right; a conflict returns the files and leaves every ref and
the object count of reachable commits unchanged; Review focus 5 (the ref is
moved by the test between the two steps through an injected hook point); a
branch checked out in a worktree is refused; a commit that does not descend
from the base is reported; a dirty worktree is reported; `sync_base` merges
the base in and reports a conflict the same way; no file in any worktree
changes during any call (compare a tree hash of the working directories).

## Addendum: wave 2 (packages 5, 6, 7 and 12)

Written 2026-11-11 against the store package as reviewed. The interface the
four packages share:

```rust
// src/teams/ops (package 3)
pub fn apply(store: &mut Store, actor: &Actor, request: &Request, now_ms: i64) -> Result<Reply, Problem>;
pub enum Actor { Person { .. }, Session { .. }, Other { .. }, Service { .. } }   // never deserialized
pub struct Request { pub request_id: String, pub team: String, pub expected_revision: u64, pub operation: Operation }
pub struct Reply { pub ok: bool, pub revision: u64, pub node: Option<NodeId>, pub item: Option<u64> }
```

`Operation` has one variant per operation of the store. Facts (`ObserveTask`,
`HandoutIntent`, `HandoutOutcome`, `ReopenIntent`, `ReopenOutcome`,
`SetMemberSession`, `RecordReport`, `RecordIntegration`,
`RecordIntegrationConflict`) are accepted only from `Actor::Service`, and no
package builds that actor from input.

**How a team comes to life.** `Create` only records the team: its copy of the
template, its members in state `NotCreated`, its settings. The desktop service
(package 7) sees members that are not created and launches them. So the CLI
and the desktop create a team the same way, and neither launches anything.

### Package 5: CLI and guides

**Owns:** `src/teams/cli.rs` (and `src/teams/cli/` if it needs more than one
file), the dispatch in `src/teams/mod.rs` that today answers `not_found`,
`src/agent_guide/team.md`, `src/agent_guide/team-lead.md` and their
registration.

**Deliver:**

- `hgs team` subcommands of M1, each reading one JSON request on stdin where
  it has input and printing one JSON reply: `catalog show|import|export`,
  `template list|show|save|import|export|remove`, `create`, `settings`,
  `pause`, `resume`, `stop`, `archive`, `decide` for the person; `status`,
  `inbox`, `queue`, `cards`, `journal`, `admit`, `route`, `accept`, `return`,
  `reassign`, `hold`, `release`, `cancel`, `reply`, `pause`, `resume` for the
  lead; `current` and `ask` for a member. `verdict` belongs to package 9.
- Actor resolution. A caller with `HGS_SESSION` and `HGS_RUN_ID` is verified
  exactly as `hgs task` verifies it (live run and process ancestry, the code
  in `src/tasks.rs`), then mapped by session identity to the team's lead or a
  member, else `Actor::Other`. A caller without them is the person only when
  no live session agent is among its process ancestors; the reply says the
  interim rule was used. A request field never names the actor.
- Reads are pages of the store: a limit of at most 200 and a cursor. `status`
  returns the team without its bundle, its members and counts; `status
  --bundle` adds the bundle.
- The reply shapes of the five reads are written as fixtures in
  `crates/team-plan/fixtures/` (`cli-status.json`, `cli-queue.json`,
  `cli-inbox.json`, `cli-cards.json`, `cli-journal.json`) and pinned by tests,
  because package 12 reads them.
- The two guide pages, each within 2,400 bytes: what a member does (the task
  protocol stays `hgs task`; `hgs team current` and `ask`), and what a lead
  does (inbox first, decide every report, never build or merge a member's
  work).

**Tests:** every command against a temporary state directory with the preview
on; each actor mapping, including a member trying a lead's command and a
session of another team; a request that tries to name its actor is refused
with `unknown_field`; with the preview off every command answers
`teams_inactive`; page stability; the fixtures round-trip.

### Package 6: tasks and teams

**Owns:** the optional `team` key in `crates/replica` task fields; in
`src/tasks.rs` and `src/tasks/`: `hgs task add --team TEAM`, `hgs task target
REF team TEAM`, and the three refusals; `src/teams/tasks.rs`.

**Deliver:**

- `team` is `{ "team": <team id>, "home": <computer UUID> }` in a task's field
  map, read and written without raising the task schema. A reader at the
  current schema that does not know the key is unaffected: pin that with a
  test that decodes a document carrying the key with the key's code removed.
- `src/teams/tasks.rs`: `observe(project, uuid) -> TaskObservation` built from
  the task document this code read itself (stage, cancelled, rank, title and
  the document's revision token), and `send(actor, task, team, role,
  pipeline)`, which applies `AddTask` as the caller and then `ObserveTask` as
  the service from that observation.
- `hgs task add --team` and `hgs task target REF team` call `send`. The
  replicated `target` stays `any`.
- On the home computer, for a task whose `team` key names a live team:
  `send`, `handoff` and `stage done` are refused for every agent except that
  team's lead. On another computer they are refused for every agent. A person
  is never refused; the override is observed by the service.

**Tests:** Review focus 3 through the CLI; the key survives a sync round trip
and an edit by a build that does not know it; each refusal and its two
exceptions; a task sent twice has one node; with the preview off `--team`
answers `teams_inactive` and writes nothing.

### Package 7: member launch and identity

**Owns:** `src/desktop/teams/` (the launch part) and its registration in the
desktop service.

**Deliver:** a service actor in the service's own pattern (admission, a fixed
plan, the original child and its custody; a restart never continues an
operation). For a team that is Running:

- the integration branch is created with `integrate::create_branch` when the
  team's integration is a branch and the branch does not exist;
- the lead, then each `NotCreated` member, is launched through the service's
  existing worktree and launch actors, each member in its own generated
  worktree and as a child of the lead;
- the account is `pick_account` over the usage windows of `hgs account
  inspect` for the member's agent, with the accounts already given to this
  team's members as `taken`;
- the launch ID binds the session: when the session appears with that launch
  ID, `SetMemberSession` records its key, its worktree and the ownership
  nonce;
- a launch whose outcome is unknown becomes an owner card and is never
  retried; `no_account_available` becomes an owner card that names the reset
  time.

Effects sit behind traits so tests use fakes.

**Tests:** the order (branch, lead, members); one launch per member across a
restart at every step; an unknown launch produces one card and no second
launch; account choice spreads two seats over two accounts; a member is bound
only by its launch ID, never by its name.

### Package 12: client core

**Owns:** `client/core/src/teams/`, `client/src-tauri/src/teams.rs`, the
`teams` key of the batch, and the lines that add `zerus-team-plan` to
`client/core/Cargo.toml` and the client lock files.

**Deliver:**

- Typed projections for the desktop: `list` (teams of the computer), `team`
  (bar, members, the queue as board groups by state, each card with its
  stages), `waiting` (owner cards and person-only requests of team sessions).
  The view structures start as an import of the fork's
  `client/core/src/teams/view.rs` in the two-commit form, reduced to what M1
  shows.
- Pure functions from the read replies of package 5 (the `cli-*.json`
  fixtures) to those views. Until package 5 is merged, build them on
  `team-queue.json`.
- Actions as `TeamCall { team, action, request_id }` with the outcomes done,
  failed, uncertain and confirm; an uncertain action is never repeated
  automatically.
- Tauri commands that run the reads and actions through `hgs team`, follow
  one team at a time, and stop when the tab is hidden. The view is read again
  after the desktop's own action, when the project's tasks topic changes, and
  every 10 seconds while the Team tab is visible. A `teams` watch topic
  replaces the timer in M4.

**Tests:** each projection from the fixtures, including a card with a stage
in each state and an empty queue; the 200-row page bound; an action outcome
of each kind; the follow set never holds more than one team.

### Packages 8 to 10 and 13 to 17

8 the teams actor; 9 pipelines; 10 lead inbox, timers and escalation; 13 New
team and the Team tab with Board and task card; 14 Waiting for you and Send to
team; 15 isolated real-agent coverage; 16 guide, reference and credit; 17
standing confirmations (folder trust for a team worktree, resume of an exited
member). Their briefs are written when packages 5 and 7 are merged.

## Decisions after the first reviews

Recorded 2026-11-11 from the independent reviews of packages 2, 3, 4 and 11.
Packages 5 to 17 follow them.

**Who records what.** Facts are recorded only by the Service actor: task
observations, hand-out and reopen intents and outcomes, a member's session
binding, a report, an integration receipt. A person, a lead or a member cannot
record them, and a request never carries a task observation. The Service actor
is built only by trusted code that read the task document or performed the
effect itself; the CLI never accepts it from input.

**A lead decides, the service acts.** Accept, Return, Reassign and Cancel
record a decision. While a decision of a node is not settled, another decision
of that node is refused.

**Accept and integration.** Accept carries no receipt. In a team with an
integration branch, a non-stage node is Done only after the service merged it
and recorded the commit. The delegation `merge_accepted` says who triggers the
merge: the lead's Accept when it is on, the person's approval of an owner card
when it is off. A conflict is recorded by the service as a return with the
conflict list. With no integration branch, and for a stage node, there is no
merge and no receipt. A receipt or a conflict that arrives after the team
was stopped is still recorded. When a node's decision settles, its open
integration cards close.

**Return.** The reopen message is an effect: an intent for the new generation
before it is sent, an outcome after, never retried when uncertain. A report of
the new generation is accepted once that effect is recorded.

**Hand-out.** Only the service records a hand-out, and only for a node the
kernel says is ready. The readiness view holds the live nodes with their
direct dependencies and stage targets; an archived node is a leaf whose own
dependencies are not followed. Reassigning a node that was never handed out
only changes its assignee, and a reopen needs an earlier attempt of the node,
so neither goes around the gate. The effect row holds the current holder. A node's
`assignee` is the plan's and changes only by Route and Reassign. For a task
node `implementer` is set when its hand-out is submitted; a refusal leaves
neither. For a stage node `implementer` is the member who implemented the task
it checks, copied when the stage is created and never changed by the stage's
own hand-out. The teams actor passes `free: false` for every member with an
open hand-out, because a stage's holder is only on its effect row. A
dependency that names a stage node never releases.

**Queue.** Only the person and the lead route a task by naming a role or a
pipeline while sending; from anyone else the values are a suggestion. Only
admitted nodes count toward `max_open_tasks`; unadmitted ones are bounded at
100 per team. A task has at most one live node: a cancelled node, and every
node of an archived or stopped team, releases it.

**Pauses.** A team the person paused is not resumed by the lead, as with a
member the person stopped.

**Integration (package 11).** No hook runs during an integration call: one
defined by the repository makes integration unavailable, the person's own are
switched off for these calls. A merge driver defined by the repository makes
integration unavailable on the merge paths; the person's own driver and signer
are honoured. A commit the branch already contains is a no-op. A conflict
changes no ref and no reachable object.

**Catalog (package 4).** A seat counts only with a positive count. Duplicate
IDs are checked inside each catalog before catalogs are merged. A template's
default pipeline is required. Every role's pipeline must exist, seated or not.

**Kernel (package 2).** A stage verdict counts only when the stage node's role
equals the pipeline's stage at its index, and a stage is ready only in its
target's round. A node ID that appears twice is ambiguous everywhere: nothing
that depends on it or checks it is ready. Write sets are relative to the
repository. ASCII components are compared without regard to case; a
component with a non-ASCII character matches any component in its position
unless the two are byte-equal, because case and normalization rules differ
between file systems. An absolute path, and a parent component after a
pattern, cover the whole repository. Operations
refuse an absolute path in a write set at input.

**Shape.** One function per operation of about 60 lines or fewer; no file that
is one long match.

## Decisions during wave 2

**Member launch (package 7).** The desktop's `creation_worktree` and
`creation_launch` actors admit only the proof of a person's gesture, and they
get no second admission. The teams actor launches through the path every CLI
caller has, in the form `creation_launch::child` uses: this computer's own
`hgs` as the one original child, run by the shared `desktop::original_child`
runner with a child kind and custody of its own. It runs `worktrees create`
and then the documented launch with the reserved launch ID. The arguments
come only from the team store.

A launch that never started leaves the member `NotCreated` for a later pass.
A refused launch makes one owner card and no second launch. An unknown
result, or an exit of 0 with no session carrying the launch ID within the
bound, makes one card marked uncertain and is never retried.

**Parent link.** A launch by the service cannot prove ancestry, so the link
is written after binding with the existing `hgs parent` action `set`, and
reads `manual`. It is written only when the member has none and never
replaces one. An agent the command refuses (DeepSeek, OpenCode) has no link,
which is a known skip. The link is presentation and attention routing: a
failed write blocks nothing and makes no card, and the lead's authority over
members comes from the team store. A source of its own for team links needs
the aligned client change and waits for M3.

**Integration facts (package 3).** `RecordIntegration` and
`RecordIntegrationConflict` name the accept decision they belong to. The
permit is that exact decision: an accept of that node, Ready or settled with
preserved authority, with no fact yet; there is no fallback to the pending or
the latest decision. The fact changes the node only when the decision's
generation is the node's current one. The service reads the authorized
accepts that have no fact as a bounded page and names the decision it acted
on.

**One caller proof (packages 5 and 6).** `crate::teams::caller`
(`src/teams/caller.rs`, owned by package 5) answers who runs a command:
`Caller::Person`, `Caller::Session(SessionKey)` or `Caller::Refused(Problem)`.
The person is a call with no session markers and no live session agent among
its process ancestors, found by a bounded lookup keyed by those ancestors and
checked against a real process in a test; a lookup that cannot be read is a
refusal. The CLI and the task intake both use it, and package 5 merges before
package 6.

**How the service reaches a session (packages 8, 10 and 17).** The desktop's
deferred-send, deferred-answer and creation actors admit only the proof of a
person's gesture. The teams actor does not enter them. Each of its effects is
this computer's own `hgs` run as the actor's one original child:

| Effect | Command |
|---|---|
| Hand-out and reopen | `hgs task team-deliver` (below) |
| Doorbell, a return's reason, a brief on resume | `hgs send SESSION --json`, bound to the run and conversation it read |
| An answer to a member's prompt | `hgs answer SESSION --json`, after the approval journal reserved it |
| Integration | the functions of `src/teams/integrate.rs` in a child of its own |

**Team delivery (package 6, second part).** The attempt writer is reachable
only through the task CLI, and `start-deliver` is the person's. A new
`hgs task team-deliver REF --json` reads `{schema, id, team, node, member}`
and delivers only what the team store already holds: an open hand-out or
reopen effect of that team with that request ID for that node and member, on
the team's home computer. The effect is the authority; the caller is not
asked who it is. It writes the attempt as the team's lead, with the prompt
`freeze` builds followed by the team context from the store (role and goal,
criteria, dependency reports within 2,000 characters each and 6,000 in total,
binding decisions, base commit, branch, write set). It answers
`{state: submitted|refused|uncertain|cutting_over}` in the form of
`start-deliver`, and the same ID never sends twice. `--recover-only` reads
the receipt and sends nothing.

**The CLI as the desktop's interface (packages 3, 5 and 12).** The client
core reads through `hgs team` and follows the shapes package 5 pins in
`crates/team-plan/fixtures/cli-*.json`: a page is
`{ok, actor, team, rows, next}`, `status` is
`{ok, actor, team, members, counts}`, and a read takes `--team`, `--limit`
and `--after`. Three things the desktop needs are added:

- `hgs team list`: the teams of this computer, a page of
  `{id, project, name, status, lead, home, revision}` in ID order
  (`cli-list.json`);
- `hgs team waiting`: the open owner cards of all teams, oldest first, each
  a card row with its team ID (`cli-waiting.json`);
- the person's `pause-member`, `resume-member`, `stop-member` and
  `archive-member`.

The store gains the reads behind them: `teams`, `live_teams` for the
service's pass, and `waiting`, each a bounded page that does not scan
archived teams or closed cards. A request of a team session that only the
person can answer is not a card; the desktop takes it from its session
attention data.

**A fact after integration was switched off (package 3).** With a valid
permit the fact is recorded as provenance; the node changes only when it
still needs a merge and the decision's generation is current.

**Task key (package 6).** Everything under `crates/replica` is one commit of
its own with its compatibility tests, merged after the repository owner
confirms the replica change. Sending a task to a team and the three refusals
work on the home computer from the team store alone; the key adds only what
another computer needs.

## Order

| Wave | Packages | Starts when |
|---|---|---|
| 0 | 1, 11 | now |
| 1 | 2, 3, 4 | package 1 is merged |
| 2 | 5, 6, 7, 12 | package 3 is merged |
| 3 | 8, 9, 10, 13, 14, 17 | packages 2, 5 and 7 are merged |
| 4 | 15, 16 | package 8 is merged |

The seam with the task queue work (`zerus-cg5h.4.8`) is recorded on that
tracker item before package 8 starts: the teams actor calls the task attempt
writer today and the per-executor queue when it exists, and adds no other way
to deliver a task into a session.
