import { trNoop } from "../../i18n";
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
    name: trNoop("Bugfix"),
    description: trNoop("A queue of independent fixes: a developer and a reviewer."),
    lead: { agent: "claude", account: { mode: "freest" }, model: "", effort: "", rules: "" },
    roster: [{ role: "developer", count: 2, optional: false }, { role: "reviewer", count: 1, optional: false }],
    default_pipeline: "reviewed",
    settings: {
      queue: true, routing: "direct", plan_approval: "review", phase_acceptance: "lead", integration: "branch", land_allowed: false,
      delegations: { accept_results: true, answer_members: true, confirm_trust: true, resume_exited: true, merge_accepted: true },
    },
  }],
  planning_rules: [],
};

// Optional projection fields, for the board-reference preview and render tests.
// Every fact below is synthetic. Role words intentionally differ from stage
// kinds: a Test stage is assigned to the member whose custom role is "quality".
const boardMember = (id: string, role: string, agent: string, model: string, lead = false) => ({
  id, role, agent, model, lead, account: "account-example", state: "active",
  session: { machine: "00000000-0000-4000-8000-0000000000aa", session_id: `launch_id:00000000-0000-4000-8000-${id.padStart(12, "0")}` },
});
const boardMembers = [
  boardMember("lead", "lead", "claude", "example-lead", true),
  boardMember("backend", "backend", "deepseek", "example-code"),
  boardMember("frontend", "frontend", "codex", "example-code"),
  boardMember("reviewer", "reviewer", "claude", "example-review"),
  boardMember("quality", "quality", "kimi", "example-test"),
];
const boardCard = (node: number, title: string, state: string, role: string, assignee: string, round = 1): import("./types").TaskView => ({
  node, title, state, role, assignee, round, kind: "task", rounds: 3,
  task: { project: "example-project", uuid: `00000000-0000-4000-8000-${String(node).padStart(12, "0")}` },
  pipeline: "reviewed-and-tested", implementer: assignee, rank: `a${String(node).padStart(2, "0")}`,
  held: false, admitted: true, integrated: false, decided_by: "", stages: [],
});
const boardStage = (target: number, kind: "review" | "test" | "land", state: string, index: number, round = 1): import("./types").StageView => ({
  node: target * 10 + index + 100, index, target, kind, state, round,
  role: kind === "test" ? "quality" : kind === "land" ? "lead" : "reviewer",
  assignee: kind === "test" ? "quality" : kind === "land" ? "lead" : "reviewer", implementer: "frontend",
});
const boardCards = [
  { ...boardCard(9, "Card form keyboard order", "pending", "frontend", "frontend"), held: true },
  boardCard(6, "Rate limits and retries", "pending", "backend", "backend"),
  boardCard(2, "Saved cards API", "in_progress", "backend", "backend", 2),
  { ...boardCard(5, "Contract tests", "reported", "quality", "quality", 3), stages: [boardStage(5, "review", "accepted", 0, 3), boardStage(5, "test", "accepted", 1, 3)] },
  { ...boardCard(10, "Saved cards API docs", "reported", "backend", "backend"), stages: [boardStage(10, "review", "pending", 0)] },
  { ...boardCard(3, "Card form UI", "reported", "frontend", "frontend"), stages: [boardStage(3, "review", "in_progress", 0)] },
  { ...boardCard(4, "Address validation", "reported", "frontend", "frontend", 2), stages: [boardStage(4, "review", "accepted", 0, 2), boardStage(4, "test", "in_progress", 1, 1)] },
  { ...boardCard(12, "Provider token vault", "accepted", "backend", "backend"), decided_by: "lead", stages: [boardStage(12, "review", "accepted", 0), boardStage(12, "test", "accepted", 1)] },
  { ...boardCard(13, "Checkout copy review", "accepted", "frontend", "frontend"), decided_by: "lead", stages: [boardStage(13, "review", "accepted", 0)] },
];
export const fidelityView: TeamView = {
  ...teamView,
  bar: { ...teamView.bar, name: "Orbit platform / checkout", counts: { members: { total: 5, active: 5, paused: 0, not_created: 0, removed: 0 }, nodes: { waiting: 0, pending: 2, in_progress: 1, reported: 4, accepted: 2, cancelled: 0 }, inbox: 1, cards: 1 } },
  members: boardMembers,
  groups: ["waiting", "pending", "in_progress", "reported", "accepted", "cancelled"].map((state) => ({ state, cards: boardCards.filter((card) => card.state === state) })),
  truncated: false, next: null,
};
