// @vitest-environment happy-dom
// Regression from the independent tester: use the production form/editor
// lifecycle, with only native form I/O and surrounding readiness stores faked.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { FormKind, FormSubmit } from "../../store/taskForms";
import type { TaskEditorView } from "../../store/taskEditor";
import type { TeamBar } from "../../store/teams";

const f = vi.hoisted(() => ({ open: vi.fn(), edit: vi.fn(), submit: vi.fn(), notify: vi.fn(), inspector: vi.fn() }));
const TEAM = "33333333-3333-4333-8333-333333333333";
const OTHER = "22222222-2222-4222-8222-222222222222";
vi.mock("../../bridge", async (original) => ({
  ...await original<typeof import("../../bridge")>(), taskFormOpen: f.open, taskFormEdit: f.edit, taskFormSubmit: f.submit,
}));
vi.mock("../../store/actions", () => ({ notify: f.notify }));
vi.mock("../../store/ui", () => ({ selected: () => undefined }));
vi.mock("../../store/organization", () => ({ groupOf: () => "example-project" }));
vi.mock("../../store/taskNavigation", () => ({ requestTaskDescription: vi.fn() }));
vi.mock("../../store/taskBoard", () => ({ sendToTeam: vi.fn(), writingTask: () => undefined }));
vi.mock("./task-view-read", () => ({ readSelectedTask: vi.fn() }));
vi.mock("../../store/inspector", () => ({ openInspector: f.inspector }));
vi.mock("../../store/tasks", () => ({ tasks: { project: () => "example-project", find: () => undefined } }));
vi.mock("../../store/taskAssignment", () => ({ draggedTask: () => undefined, readTaskDrag: vi.fn(), setDraggedTask: vi.fn(), TASK_MIME: "application/x-zerus-task" }));
vi.mock("../../store/teams", () => ({ teams: {
  available: () => true,
  list: () => ({ teams: [
    { id: "33333333-3333-4333-8333-333333333333", project: "example-project", name: "Fixes", status: "running", revision: 1 },
    { id: "22222222-2222-4222-8222-222222222222", project: "example-project", name: "Reserved team", status: "paused", revision: 1 },
  ] }), team: () => undefined,
} }));
import { requestTeamTask } from "../../store/teamTaskIntake";
import TaskCreate from "./TaskCreate";
import { newCard } from "../../store/taskForms";
import { teamTaskCreation, setTeamTaskCreation } from "../../store/taskCreation";
import { teamView } from "../teams/fixtures";

const requestedTeam: TeamBar = { ...teamView.bar, id: TEAM };
const view = (handle: number, over: Partial<TaskEditorView<FormKind>> = {}): TaskEditorView<FormKind> => ({
  project: "example-project", task: "", mode: "new_card", handle, rev: 1, text: "Retained draft", position: 14, anchor: 14,
  files: [], error: "", recovery: false, blocked: false, saved: true, ...over,
});
const tick = async () => { for (let n = 0; n < 60; n++) await Promise.resolve(); };
let dispose: (() => void) | undefined;
let release: (() => void) | undefined;
function mount() {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TaskCreate, { project: "example-project", scope: "Tasks", backlog: false, accessible: true, children: null }), host);
  return host;
}
beforeEach(async () => {
  newCard.follow(); await tick();
  f.open.mockReset(); f.edit.mockReset(); f.submit.mockReset(); f.notify.mockReset(); f.inspector.mockClear(); setTeamTaskCreation(undefined);
});
afterEach(async () => {
  release?.(); release = undefined; dispose?.(); dispose = undefined;
  newCard.follow(); await tick(); setTeamTaskCreation(undefined); document.body.replaceChildren();
});
const enter = (host: HTMLElement) => host.querySelector("input.tk-newtitle")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

it("selects the requested team through the production editor on a cold open", async () => {
  f.open.mockResolvedValueOnce(view(1)); f.submit.mockResolvedValueOnce(view(1, { text: "", created: "44444444-4444-4444-8444-444444444444" }));
  requestTeamTask(requestedTeam); expect(f.inspector).toHaveBeenCalledWith("tasks");
  const host = mount(); await tick();
  expect(host.querySelector("button.tk-team")?.textContent).toContain("Fixes");
  enter(host); await tick();
  expect(f.submit).toHaveBeenCalledWith(expect.objectContaining({ team: TEAM, handle: 1, recover: false }));
});

it.each([false, true])("waits for a pending-save remount before selecting the team (reserved card: %s)", async (reserved) => {
  f.open.mockResolvedValueOnce(view(10)); mount(); await tick();
  expect(newCard.view()?.handle).toBe(10);
  f.edit.mockImplementationOnce((_kind: FormKind, input: Partial<TaskEditorView<FormKind>>) => new Promise((resolve) => {
    release = () => resolve(view(10, { ...input, saved: true }));
  }));
  newCard.edit("Unsaved title", 13, 13); void newCard.flush(); await tick();
  expect(f.edit).toHaveBeenCalledTimes(1);
  dispose!(); dispose = undefined;
  f.open.mockResolvedValueOnce(view(11, { text: "Unsaved title", recovery: reserved, team: reserved ? OTHER : null }));
  f.submit.mockResolvedValueOnce(view(11, { text: "", created: "55555555-5555-4555-8555-555555555555" }));
  requestTeamTask(requestedTeam);
  const host = mount(); await tick();
  expect(newCard.opening()).toBe(true); expect(newCard.view()?.handle).toBe(10);
  expect(teamTaskCreation()).toEqual({ project: "example-project", team: TEAM });
  release!(); release = undefined; await tick();
  expect(newCard.opening()).toBe(false); expect(newCard.view()?.handle).toBe(11);
  expect(teamTaskCreation()).toBeUndefined();
  const chooser = host.querySelector<HTMLButtonElement>("button.tk-team")!;
  expect(chooser.textContent).toContain(reserved ? "Reserved team" : "Fixes");
  expect(chooser.disabled).toBe(reserved);
  enter(host); await tick();
  expect(f.submit).toHaveBeenCalledTimes(1);
  const submitted = f.submit.mock.calls[0][0] as FormSubmit;
  expect(submitted).toMatchObject({ handle: 11, recover: reserved });
  if (reserved) expect(submitted).not.toHaveProperty("team");
  else expect(submitted).toMatchObject({ team: TEAM, target: null, catalog_key: null, session_id: null });
});
