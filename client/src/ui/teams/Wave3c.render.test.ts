// @vitest-environment happy-dom
// zerus-63rru.199 (Teams UI wave 3c, forms and reasons). Rows come from the
// product fixtures where a product writer pins them: the placement view
// (task-placement-view.json), the pipeline acceptance card
// (report-notes-card.json), the phase pane (phase-pane.json) and the provider
// scope (provider-scope.json). handwritten_ tests build the variants no
// fixture writes.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import boardFixture from "./fixtures/task-placement-view.json";
import reportNotesCard from "../../../core/src/teams/fixtures/report-notes-card.json";
import phaseFixture from "./fixtures/phase-pane.json";
import scope from "../../../core/src/teams/fixtures/provider-scope.json";
import payload from "../../../core/src/teams/fixtures/providers.json";
import { TaskPlacement } from "./TaskPlacement";
import { TaskDependencies } from "./TaskDependencies";
import { TeamEditButtons } from "./TeamEditors";
import { WaitingCard } from "./Waiting";
import { PhasePane } from "./PhasePane";
import { ProviderSettings } from "./ProviderSettings";
import { leadChatCause } from "./lead-chat";
import { draftKey } from "../../store/draftKeys";
import { teamTaskBlocked } from "../../store/teamTaskIntake";
import { teamView } from "./fixtures";
import type { PhaseFollow } from "../../store/team-phases";
import type { ProviderInspection, ProviderRead } from "../../store/teamProviders";
import type { WaitingItem } from "../../store/teams";
import type { TaskView, TeamView } from "./types";

const transport = vi.hoisted(() => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: transport.invoke, isTauri: () => false }));

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); transport.invoke.mockClear(); });
const mount = (child: () => import("solid-js").JSX.Element) => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(child, host); return host;
};
const tick = async () => { await new Promise((resolve) => setTimeout(resolve, 0)); };

// ---------------------------------------------------------------- B3 Route
const view = boardFixture.before as unknown as TeamView;
const pending = view.groups.flatMap((group) => group.cards).find((card) => card.state === "pending")! as TaskView;
function placement(task: TaskView) {
  const actions = vi.fn();
  const host = mount(() => createComponent(TaskPlacement, { view, task, busy: false, unresolved: false, otherFormOpen: false, onOpen: () => {}, onAction: actions }));
  host.querySelector<HTMLButtonElement>('[data-task-action="route"]')!.click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="route"]')!;
  return { host, form, actions, submit: () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })) };
}
const writesField = (form: HTMLElement) => form.querySelector<HTMLTextAreaElement>("[data-route-writes]")!;
const assigneeField = (form: HTMLElement) => form.querySelector<HTMLSelectElement>("[data-route-assignee]")!;
const type = (field: HTMLTextAreaElement, value: string) => { field.value = value; field.dispatchEvent(new Event("input", { bubbles: true })); };
const choose = (field: HTMLSelectElement, value: string) => { field.value = value; field.dispatchEvent(new Event("change", { bubbles: true })); };

it("a product task's Route form shows its assignee and write paths, and an unchanged form keeps them", () => {
  const task = { ...pending, writes: ["src/labels.rs", "docs/"] } as TaskView;
  const { form, actions, submit } = placement(task);
  expect(assigneeField(form).value).toBe("m2");
  expect([...assigneeField(form).options].map((option) => option.value)).toEqual(["", "m1", "m2"]);
  expect(writesField(form).value).toBe("src/labels.rs\ndocs/");
  submit();
  expect(actions).toHaveBeenCalledWith("route", { node: task.node, role: "developer", pipeline: "reviewed", keep_task_fields: true }, view.revision, view.bar.id);
});

it("a changed assignee or write path sends the whole placement with the task's dependencies", () => {
  const task = { ...pending, writes: ["src/labels.rs"], deps: [{ node: 1 }, { node: 3, kind: "soft" }] } as TaskView;
  const { form, actions, submit } = placement(task);
  choose(assigneeField(form), "m1");
  type(writesField(form), "src/labels.rs\n  tests/labels.rs  \n");
  submit();
  expect(actions).toHaveBeenCalledWith("route", {
    node: task.node, role: "developer", pipeline: "reviewed", keep_task_fields: false,
    deps: [{ node: 1 }, { node: 3, kind: "soft" }], writes: ["src/labels.rs", "tests/labels.rs"], assignee: "m1",
  }, view.revision, view.bar.id);
});

it("handwritten clearing the assignee sends nobody, and an absolute write path is refused before sending", () => {
  const { form, actions, submit } = placement(pending);
  choose(assigneeField(form), "");
  type(writesField(form), "/etc/passwd");
  expect(form.textContent).toContain("Write paths are relative to the repository.");
  expect(form.querySelector<HTMLButtonElement>('[type="submit"]')!.disabled).toBe(true);
  submit();
  expect(actions).not.toHaveBeenCalled();
  type(writesField(form), "");
  submit();
  expect(actions).toHaveBeenCalledWith("route", expect.objectContaining({ keep_task_fields: false, deps: [], writes: [], assignee: null }), view.revision, view.bar.id);
});

it("handwritten a task whose fields could not all be read keeps them, and says why", () => {
  const { form, actions, submit } = placement({ ...pending, fields_incomplete: true } as TaskView);
  expect(assigneeField(form).disabled).toBe(true);
  expect(writesField(form).disabled).toBe(true);
  expect(form.textContent).toContain("This task's dependencies or write paths could not all be read, so Route keeps its assignment and write paths.");
  submit();
  expect(actions).toHaveBeenCalledWith("route", expect.objectContaining({ keep_task_fields: true }), view.revision, view.bar.id);
});

// ---------------------------------------------------------------- B5 soft dependency
it("a dependency can be added as soft", () => {
  const card = view.groups.flatMap((group) => group.cards).find((candidate) => candidate.deps_editable)! as TaskView;
  const set = vi.fn();
  const host = mount(() => createComponent(TaskDependencies, { task: { ...card, deps: [] }, status: "running", busy: false, onSetDeps: set }));
  const number = host.querySelector<HTMLInputElement>('input[type="number"]')!;
  number.value = "1"; number.dispatchEvent(new Event("input", { bubbles: true }));
  const soft = host.querySelector<HTMLInputElement>("[data-dependency-soft]")!;
  expect(soft.checked).toBe(false);
  soft.click();
  host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  expect(set).toHaveBeenCalledWith([{ node: 1, kind: "soft" }]);
});

// ---------------------------------------------------------------- A5 acceptance card
function productionAcceptanceCard(): WaitingItem {
  const row = reportNotesCard;
  return {
    id: `${row.team}:${row.seq}`, team: row.team, card: row.seq, kind: row.kind,
    node: row.node, text: "Every stage of #1 passed. Accept the work?", state: row.state, by: row.actor.person,
    created_ms: row.created_ms, decisions: row.decisions, notes: row.notes,
  };
}

it("a product acceptance card puts Open task first, ahead of Approve and Reject", () => {
  const card = productionAcceptanceCard();
  expect(card.kind).toBe("pipeline_acceptance");
  const decide = vi.fn(), openTask = vi.fn();
  const host = mount(() => createComponent(WaitingCard, { item: card, teamName: "Fixes", now: card.created_ms, busy: false, onDecide: decide, onOpenTeam: vi.fn(), onOpenTask: openTask }));
  const buttons = [...host.querySelectorAll<HTMLButtonElement>(".waiting-decisions button")];
  expect(buttons[0].textContent).toBe("Open task");
  expect(buttons[0].classList.contains("primary")).toBe(true);
  expect(buttons.filter((button) => button.classList.contains("primary"))).toHaveLength(1);
  expect(host.textContent).toContain("Read the report and verdicts in the task before you approve.");
  buttons[0].click();
  expect(openTask).toHaveBeenCalledWith(card);
});

// ---------------------------------------------------------------- A6 Add task
it("Add task says why it cannot add while the task panel shows another project", () => {
  const bar = teamView.bar;
  expect(teamTaskBlocked(bar, "example-project")).toBeUndefined();
  expect(teamTaskBlocked(bar, "other-project")).toBe("The task panel shows another project. Open a session of example-project to add a task to this team.");
  expect(teamTaskBlocked(bar, undefined)).toBe("The task panel shows another project. Open a session of example-project to add a task to this team.");
  expect(teamTaskBlocked({ ...bar, status: "stopped" }, "example-project")).toBe("Only a running or paused team takes new tasks.");
});

// ---------------------------------------------------------------- C3 Edit goal
it("a team without a goal can be given one, and a greyed Edit goal says why", () => {
  const open = vi.fn();
  const host = mount(() => createComponent(TeamEditButtons, { view: { ...teamView, edit_goal: "" }, busy: false, open }));
  const button = [...host.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === "Edit goal")!;
  expect(button.disabled).toBe(false);
  dispose?.(); document.body.replaceChildren();
  const greyed = mount(() => createComponent(TeamEditButtons, { view: { ...teamView, edit_goal: null }, busy: false, open }));
  expect(greyed.querySelector("[data-edit-goal-why]")?.textContent).toBe("This window could not read the team's complete goal. Refresh the team, or update hgs if it is older.");
});

// ---------------------------------------------------------------- C4 Lead chat
it("handwritten lead chat says which cause keeps it unavailable", () => {
  const row = { computer: "", catalogKey: "run:current", name: "codex/example/lead", sessionId: "11111111-1111-4111-8111-111111111111", state: "live", runId: "current", launchId: "launch", conversationId: "conversation" };
  const key = draftKey({ computer: "", key: row.catalogKey, name: row.name, session_id: row.sessionId, archive_id: null });
  const lead = { ...teamView, members: teamView.members.map((member) => member.lead ? { ...member, session: { machine: "machine", session_id: "launch_id:launch" } } : member) };
  expect(leadChatCause(lead, row, "machine", key, false)).toBeUndefined();
  expect(leadChatCause(lead, undefined, "machine", key, false)).toBe("Open the lead's session first.");
  expect(leadChatCause(lead, row, "machine", key, true)).toBe("Close the open subagent to talk to the lead.");
  expect(leadChatCause(lead, { ...row, state: "archived" }, "machine", key, false)).toBe("The lead's session is archived.");
  expect(leadChatCause(lead, { ...row, runId: "" }, "machine", key, false)).toBe("The lead's session is not running. Start it to talk to the lead.");
  expect(leadChatCause(lead, row, "other", key, false)).toBe("The lead runs on another computer.");
  expect(leadChatCause(lead, { ...row, launchId: "other" }, "machine", key, false)).toBe("The selected session is no longer the team's lead.");
  expect(leadChatCause(lead, row, "machine", "old-key", false)).toBe("The lead's conversation is still loading.");
});

// ---------------------------------------------------------------- C7 More phases
it("after More phases, the phase list offers its first page again", async () => {
  const v = structuredClone(phaseFixture) as unknown as TeamView;
  v.plan!.list!.next = "p1.next";
  const follow = vi.fn<PhaseFollow>().mockResolvedValue(v.plan!.epoch);
  const host = mount(() => createComponent(PhasePane, { view: v, busy: false, follow }));
  await vi.waitFor(() => expect(follow).toHaveBeenCalled());
  const byText = (text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === text);
  await vi.waitFor(() => expect(byText("More phases")).toBeDefined());
  expect(byText("First page")).toBeUndefined();
  byText("More phases")!.click();
  await tick();
  expect(transport.invoke).toHaveBeenCalledWith("teams_page", { key: `phases:${v.bar.id}`, cursor: "p1.next" });
  byText("First page")!.click();
  await tick();
  expect(transport.invoke).toHaveBeenLastCalledWith("teams_page", { key: `phases:${v.bar.id}`, cursor: null });
  expect(byText("First page")).toBeUndefined();
});

// ---------------------------------------------------------------- C10 Tests and Builds
it("Tests and Builds reads the team's home from its status when the list page lacks it", async () => {
  const team = structuredClone(scope.team) as unknown as TeamView;
  const inspection = structuredClone(payload.inspection) as ProviderInspection;
  expect(team.bar.home).toBe(inspection.home);
  const port = vi.fn(async (_: ProviderRead) => inspection);
  const host = mount(() => createComponent(ProviderSettings, { view: team, home: undefined, locked: false, onSave: vi.fn(), port }));
  await tick();
  expect(host.textContent).not.toContain("The team’s home computer is missing from the loaded team list.");
  expect(port).toHaveBeenCalledOnce();
});

// ---------------------------------------------------------------- B7 New team
it("New team says the roles and pipelines are fixed when the team is created", async () => {
  const { NewTeam } = await import("./NewTeam");
  const { catalog } = await import("./fixtures");
  const host = mount(() => createComponent(NewTeam, {
    busy: false, machineId: "00000000-0000-4000-8000-0000000000aa", catalog,
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: async (input) => ({ outcome: "done", receipt: {}, request_id: input.request_id }), onSaveTemplate: async () => {}, onClose: () => {},
  }));
  expect(host.querySelector("[data-new-team-fixed]")?.textContent)
    .toBe("The roles and pipelines are fixed when the team is created. To change them, edit the template first.");
});

it("the real Team tab disables Add task with its reason until the task panel shows the team's project", async () => {
  const { TeamWorkspace } = await import("./TeamTab");
  const { tasks } = await import("../../store/tasks");
  tasks.follow("other-project");
  const host = mount(() => createComponent(TeamWorkspace, { view: teamView, busy: false, failure: null, onAction: vi.fn(), onPage: vi.fn(), onAddTask: vi.fn() }));
  const add = host.querySelector<HTMLButtonElement>("[data-team-add-task]")!;
  expect(add.disabled).toBe(true);
  expect(host.querySelector("[data-team-add-task-why]")?.textContent).toBe("The task panel shows another project. Open a session of example-project to add a task to this team.");
  tasks.follow("example-project");
  expect(add.disabled).toBe(false);
  expect(host.querySelector("[data-team-add-task-why]")).toBeNull();
  tasks.follow(undefined);
});
