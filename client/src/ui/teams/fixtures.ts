import type { TeamCatalog, TeamView } from "../../store/teams";

// The pinned Teams replies of package 5 (`crates/team-plan/fixtures/cli-status.json`
// and `cli-queue.json`, copied into client/core), projected the way the core
// projects them. The render tests use these values and no live `hgs`.

export const TEAM_ID = "00000000-0000-4000-8000-00000000beef";

export const teamView: TeamView = {
  bar: {
    id: TEAM_ID,
    project: "example-project",
    name: "Fixes",
    status: "running",
    goal: "",
    lead: "lead",
    integration_branch: "zerus/team/fixes",
    base_commit: "0123456789abcdef0123456789abcdef01234567",
    revision: 16,
    counts: {
      members: { total: 4, not_created: 2, active: 2, paused: 0, removed: 0 },
      nodes: { waiting: 0, pending: 1, in_progress: 0, reported: 1, accepted: 0, cancelled: 0 },
      inbox: 1,
      cards: 1,
    },
  },
  members: [
    {
      id: "lead", role: "lead", agent: "claude", account: "account-example", state: "active", lead: true,
      session: { machine: "00000000-0000-4000-8000-0000000000aa", session_id: "launch_id:00000000-0000-4000-8000-000000000064" },
    },
    {
      id: "m1", role: "developer", agent: "claude", account: "account-example", state: "active", lead: false,
      session: { machine: "00000000-0000-4000-8000-0000000000aa", session_id: "launch_id:00000000-0000-4000-8000-000000000031" },
    },
    { id: "m2", role: "developer", agent: "claude", account: "", state: "not_created", lead: false, session: null },
    { id: "m3", role: "reviewer", agent: "claude", account: "", state: "not_created", lead: false, session: null },
  ],
  groups: [
    { state: "draft", cards: [] },
    { state: "waiting", cards: [] },
    {
      state: "pending", cards: [{
        node: 1, title: "Task 1", state: "pending", task: { project: "example-project", uuid: "00000000-0000-4000-8000-000000000001" },
        role: "developer", pipeline: "reviewed", assignee: "", implementer: "", rank: "a01", round: 1,
        held: false, admitted: true, integrated: false, decided_by: "", stages: [],
      }],
    },
    { state: "in_progress", cards: [] },
    {
      state: "reported", cards: [{
        node: 2, title: "Task 2", state: "reported", task: { project: "example-project", uuid: "00000000-0000-4000-8000-000000000002" },
        role: "developer", pipeline: "reviewed", assignee: "", implementer: "m1", rank: "a02", round: 1,
        held: false, admitted: true, integrated: false, decided_by: "", stages: [
          { node: 5, index: 0, role: "reviewer", state: "pending", assignee: "m3", implementer: "m1" },
        ],
      }],
    },
    { state: "accepted", cards: [] },
    { state: "cancelled", cards: [] },
  ],
  truncated: false,
  next: null,
  revision: 16,
};

/** The built-in `bugfix` template as `hgs team catalog show` sends it. */
export const catalog: TeamCatalog = {
  roles: [
    { id: "developer", name: "Developer", agent: "claude", account: { mode: "freest" }, model: "", effort: "", pipeline: "", context: "keep" },
    { id: "reviewer", name: "Reviewer", agent: "claude", account: { mode: "freest" }, model: "", effort: "", pipeline: "", context: "keep" },
  ],
  pipelines: [
    { id: "reviewed", name: "Reviewed: review, lead", implementer: "developer", stages: [{ kind: "review", role: "reviewer" }], max_rounds: 0, accept: "lead" },
  ],
  templates: [{
    id: "bugfix",
    name: "Bugfix",
    description: "A queue of independent fixes: a developer and a reviewer.",
    lead: { agent: "claude", account: { mode: "freest" }, model: "", effort: "", rules: "" },
    roster: [{ role: "developer", count: 2, optional: false }, { role: "reviewer", count: 1, optional: false }],
    default_pipeline: "reviewed",
    settings: { queue: true, routing: "direct", plan_approval: "review", phase_acceptance: "lead", integration: "branch", land_allowed: false },
  }],
  planning_rules: [],
};
