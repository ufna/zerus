// @vitest-environment happy-dom
// Task rows come from TaskDoc's real lifecycle writer, pinned by
// crates/replica/src/task_ui_fixture_tests.rs. Only the native I/O is faked.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import fromWriter from "./fixtures/task-intake.json";
import { teamView } from "./fixtures";
import type { TeamView } from "../../store/teams";

const f = vi.hoisted(() => ({
  board: vi.fn(), create: vi.fn(async (..._args: unknown[]) => false), page: vi.fn(), inspector: vi.fn(),
  setTeam: (_view: TeamView | undefined) => {}, setAvailable: (_on: boolean) => {},
  setForm: (_view: unknown) => {}, form: (() => undefined) as () => unknown,
  setList: (_rows: unknown[]) => {},
}));
vi.mock("@tauri-apps/api/core", async (original) => ({ ...await original<typeof import("@tauri-apps/api/core")>(), invoke: vi.fn(async () => 1) }));
vi.mock("../../bridge", async (original) => ({ ...await original<typeof import("../../bridge")>(), taskBoardAction: f.board }));
vi.mock("../../store/ui", async (original) => {
  const module = await original<typeof import("../../store/ui")>();
  const { toSession } = await import("../../store/catalog");
  const row = toSession("", "launch:example", { name: "codex/example/lead", launch_id: "example" });
  return { ...module, selected: () => row, setPage: f.page };
});
vi.mock("../../store/machines", async (original) => ({ ...await original<typeof import("../../store/machines")>(), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
vi.mock("../../store/inspector", async (original) => ({ ...await original<typeof import("../../store/inspector")>(), openInspector: f.inspector }));
vi.mock("../../store/taskNavigation", () => ({ selectedTask: () => undefined, openTask: vi.fn(), requestTaskDescription: vi.fn(), requestTaskRename: vi.fn() }));
vi.mock("../../store/tasks", async (original) => {
  const module = await original<typeof import("../../store/tasks")>();
  return { ...module, tasks: module.createTasks(async () => {}) };
});
vi.mock("../../store/taskContext", async () => {
  const { tasks } = await import("../../store/tasks");
  return { taskProject: () => ({ id: "example-project", name: "Example", accessible: true }), projectTasks: tasks.tasks,
    projectSessions: () => [], findTaskSession: () => () => undefined, sessionTaskRows: () => undefined };
});
vi.mock("../../store/taskForms", async () => {
  const { createSignal } = await import("solid-js");
  const [view, setView] = createSignal<unknown>(); f.form = view; f.setForm = setView;
  return { createCard: f.create, newCard: { view, follow: vi.fn(), locked: () => false, opening: () => false, busy: () => false,
    edit: vi.fn(), flush: vi.fn(async () => {}), error: () => "" } };
});
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [team, setTeam] = createSignal<TeamView>(); const [available, setAvailable] = createSignal(true);
  const [rows, setList] = createSignal<unknown[]>([]);
  Object.assign(f, { setTeam, setAvailable, setList });
  return { newRequestId: () => "request-example", teams: {
    team, available, list: () => ({ teams: rows(), truncated: false, next: null }),
    teamOfSession: () => ({ state: "lead", team: "00000000-0000-4000-8000-00000000beef" }),
    closeTeam: async () => {}, openTeam: async () => {}, clearFailure: vi.fn(), confirmation: () => null,
    readFailureFor: () => null, failureFor: () => null, unresolved: () => [], busy: () => false,
    run: vi.fn(), page: vi.fn(),
  } };
});
vi.mock("../lib/pointerDrag", async (original) => ({ ...await original<typeof import("../lib/pointerDrag")>(), POINTER_DRAG: true }));
import { TeamTab } from "./TeamTab";
import { TasksPanel } from "../screens/Tasks";
import TaskCreate from "../screens/TaskCreate";
import { tasks } from "../../store/tasks";
import { taskItem } from "../screens/tasks-ui";
import { taskDragStart, setDraggedTask, TASK_MIME } from "../../store/taskAssignment";
import { setTeamTaskCreation } from "../../store/taskCreation";
import { setWritingTask } from "../../store/taskBoard";
import { notices, bus } from "../../store/notices";

let dispose: (() => void) | undefined;
const tick = async () => { for (let n = 0; n < 15; n++) await Promise.resolve(); };
const form = (over = {}) => ({ project: "example-project", text: "", handle: 1, recovery: false, ...over });
function publish(stage: string) {
  const raw = fromWriter.find((row) => row.stage === stage)!;
  tasks.apply([{ computer: "", key: "example-project", kind: "list", payload: { project: { id: "example-project" }, tasks: [raw] } }]);
  return taskItem(raw);
}
beforeEach(() => {
  f.board.mockReset(); f.create.mockClear(); f.page.mockClear(); f.inspector.mockClear();
  tasks.follow("example-project"); f.setTeam(teamView); f.setAvailable(true);
  f.setList([{ ...teamView.bar, home: "example-machine" }]); f.setForm(form());
});
afterEach(() => {
  dispose?.(); dispose = undefined; tasks.follow(undefined); setDraggedTask(undefined); setWritingTask(undefined);
  setTeamTaskCreation(undefined);
  for (const notice of notices()) bus.dismiss(notice.id);
  document.body.replaceChildren(); vi.restoreAllMocks();
});
async function setup(panel = false) {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => [createComponent(TeamTab, {}), panel ? createComponent(TasksPanel, {}) : createComponent(TaskCreate, {
    project: "example-project", scope: "Tasks", backlog: false, accessible: true, children: null,
  })], host);
  await tick();
  expect(host.querySelector("[data-team-tab]")).not.toBeNull();
  return host;
}
function add(host: HTMLElement) {
  [...host.querySelectorAll<HTMLButtonElement>(".team-board-head button")].find((button) => button.textContent === "Add task")!.click();
}
function start(stage: string) {
  const task = publish(stage), data = new DataTransfer();
  taskDragStart(dragEvent("dragstart", data), task);
  return { task, data };
}
function dragEvent(type: string, data: DataTransfer) {
  // happy-dom does not implement DragEvent's dataTransfer constructor field.
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: data });
  return event as DragEvent;
}
function drop(host: HTMLElement, data: DataTransfer) {
  const board = host.querySelector(".team-canvas")!;
  const over = dragEvent("dragover", data);
  board.dispatchEvent(over);
  board.dispatchEvent(dragEvent("drop", data));
  return over.defaultPrevented;
}

it("Add task opens the New task form with this team preselected and submits to its queue", async () => {
  const host = await setup(); add(host); await tick();
  expect(f.page).not.toHaveBeenCalled(); expect(f.inspector).toHaveBeenCalledWith("tasks");
  expect(host.querySelector("input.tk-newtitle")).not.toBeNull();
  expect(host.querySelector("button.tk-team")?.textContent).toContain("Fixes");
  f.setForm(form({ text: "New task" }));
  host.querySelector("input.tk-newtitle")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await tick();
  expect(f.create).toHaveBeenCalledWith("example-project", false, expect.anything(), teamView.bar.id);
});

it("keeps the requested team while the form opens asynchronously, even beyond the list page", async () => {
  f.setForm(undefined); f.setList([]);
  const host = await setup(); add(host); await tick();
  f.setForm(form({ text: "New task" })); await tick();
  expect(host.querySelector("button.tk-team")?.textContent).toContain("Fixes");
});

it("Add task keeps a reserved card's original team", async () => {
  f.setForm(form({ recovery: true, team: "22222222-2222-4222-8222-222222222222" }));
  const host = await setup(); add(host); await tick();
  const chooser = host.querySelector<HTMLButtonElement>("button.tk-team")!;
  expect(chooser.disabled).toBe(true); expect(chooser.textContent).toContain("22222222");
});

it("Add task refuses while Teams are off and a stopped team offers no Add task", async () => {
  const host = await setup(); f.setAvailable(false); add(host); await tick();
  expect(f.inspector).not.toHaveBeenCalled(); expect(host.querySelector("input.tk-newtitle")).toBeNull();
  expect(notices().map((n) => n.text)).toContain("The chosen team is no longer offered for this project. Nothing was sent.");
  f.setAvailable(true); f.setTeam({ ...teamView, bar: { ...teamView.bar, status: "stopped" } });
  expect([...host.querySelectorAll<HTMLButtonElement>(".team-board-head button")].find((button) => button.textContent === "Add task")!.disabled).toBe(true);
});

it.each(["backlog", "todo"])("drops a %s card through Send to team", async (stage) => {
  const host = await setup(), { task, data } = start(stage);
  expect(drop(host, data)).toBe(true); await tick();
  expect(f.board).toHaveBeenCalledExactlyOnceWith("example-project", task.uuid, stage, { kind: "send_to_team", team: teamView.bar.id });
  expect(notices().map((n) => n.text)).toContain("Sent to Fixes");
});

it.each(["active", "review", "done"])("refuses a %s drop with Send to team's wording", async (stage) => {
  const host = await setup(), { data } = start(stage);
  expect(drop(host, data)).toBe(false); await tick();
  expect(f.board).not.toHaveBeenCalled();
  expect(notices().map((n) => n.text)).toContain("The task changed. Nothing was changed. Refresh the task.");
});

it("rechecks the task stage at drop and preserves the team's refusal verbatim", async () => {
  const host = await setup(), { data } = start("todo"); publish("active");
  drop(host, data); await tick(); expect(f.board).not.toHaveBeenCalled();
  expect(notices().map((n) => n.text)).toContain("The task changed. Nothing was changed. Refresh the task.");
  const next = start("backlog"); f.board.mockRejectedValueOnce("The queue is full.");
  drop(host, next.data); await tick();
  expect(notices().map((n) => n.text)).toContain("The queue is full."); expect(f.board).toHaveBeenCalledTimes(1);
});

it("ignores foreign payloads and other projects, and never repeats an in-flight send", async () => {
  const host = await setup(), { data } = start("todo");
  data.setData(TASK_MIME, JSON.stringify({ project: "other-project", uuid: fromWriter[0].uuid }));
  drop(host, data); await tick(); expect(f.board).not.toHaveBeenCalled();
  let finish!: () => void; f.board.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
  drop(host, start("todo").data); drop(host, start("todo").data);
  expect(f.board).toHaveBeenCalledTimes(1);
  expect(notices().map((n) => n.text)).toContain("Another task change is being written. Nothing was sent.");
  finish(); await tick();
});

it("refuses a now read-only card, and ignores drags after leaving the project or disabling Teams", async () => {
  const host = await setup(), { data } = start("todo");
  tasks.apply([{ computer: "", key: "example-project", kind: "list", payload: { project: { id: "example-project" }, tasks: fromWriter.filter((row) => !row.writable) } }]);
  expect(drop(host, data)).toBe(false); await tick();
  expect(f.board).not.toHaveBeenCalled();
  expect(notices().map((n) => n.text)).toContain("The task changed. Nothing was changed. Refresh the task.");
  const next = start("todo"); f.setAvailable(false); drop(host, next.data); await tick();
  expect(f.board).not.toHaveBeenCalled(); f.setAvailable(true);
  const other = start("todo"); tasks.follow("other-project"); drop(host, other.data); await tick();
  expect(f.board).not.toHaveBeenCalled();
});

it.each(["backlog", "todo", "active", "review", "done"])("handles a %s pointer drag from the Tasks side panel", async (stage) => {
  publish(stage); const host = await setup(true);
  host.querySelector<HTMLButtonElement>(`[data-filter="${stage}"]`)!.click();
  if (stage === "done") host.querySelector<HTMLButtonElement>(".tk-older")?.click();
  const card = host.querySelector<HTMLElement>(".tk-card")!, board = host.querySelector<HTMLElement>(".team-canvas")!;
  expect(card).not.toBeNull(); expect(card.dataset.stage).toBe(stage);
  vi.spyOn(document, "elementFromPoint").mockReturnValue(board);
  card.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 0, pointerId: 1, button: 0, buttons: 1, isPrimary: true }));
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 100, clientY: 100, pointerId: 1, buttons: 1 }));
  window.dispatchEvent(new PointerEvent("pointerup", { clientX: 100, clientY: 100, pointerId: 1 }));
  // Allow the drag's one-click suppression timer to retire before the next
  // component test clicks a stage tab.
  await new Promise<void>((done) => setTimeout(done, 0));
  await tick();
  if (["backlog", "todo"].includes(stage)) {
    expect(f.board).toHaveBeenCalledExactlyOnceWith("example-project", fromWriter[0].uuid, stage, { kind: "send_to_team", team: teamView.bar.id });
  } else {
    expect(f.board).not.toHaveBeenCalled();
    expect(notices().map((n) => n.text)).toContain("The task changed. Nothing was changed. Refresh the task.");
  }
});

const modes = ["board", "graph", "lanes", "layers"] as const;
function selectMode(host: HTMLElement, mode: typeof modes[number]) {
  if (mode !== "board") host.querySelector<HTMLButtonElement>(`[data-team-${mode}-mode]`)!.click();
}
function expectDropTarget(host: HTMLElement, board: boolean) {
  const canvas = host.querySelector<HTMLElement>(".team-canvas")!;
  expect.soft(canvas.dataset.teamTaskDrop).toBe(board ? teamView.bar.id : undefined);
  expect.soft(canvas.classList.contains("team-task-drop")).toBe(board);
  expect.soft(host.textContent?.includes("Drop a Backlog or To do task here to send it to this team.")).toBe(board);
}

it.each(modes)("limits HTML task drops to Board (view: %s)", async (mode) => {
  const host = await setup();
  // Change mode with a drag already active: the shared canvas must retire its
  // target and handlers as soon as the person leaves Board.
  const { task, data } = start("todo");
  selectMode(host, mode); await tick();
  expectDropTarget(host, mode === "board");
  expect.soft(drop(host, data)).toBe(mode === "board"); await tick();
  if (mode === "board") {
    expect(f.board).toHaveBeenCalledExactlyOnceWith("example-project", task.uuid, "todo", { kind: "send_to_team", team: teamView.bar.id });
  } else expect(f.board).not.toHaveBeenCalled();
});

it.each(modes)("limits pointer task drops to Board (view: %s)", async (mode) => {
  publish("todo"); const host = await setup(true);
  selectMode(host, mode); await tick();
  host.querySelector<HTMLButtonElement>('[data-filter="todo"]')!.click();
  const card = host.querySelector<HTMLElement>(".tk-card")!, canvas = host.querySelector<HTMLElement>(".team-canvas")!;
  expect(card).not.toBeNull();
  vi.spyOn(document, "elementFromPoint").mockReturnValue(canvas);
  card.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 0, pointerId: 1, button: 0, buttons: 1, isPrimary: true }));
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 100, clientY: 100, pointerId: 1, buttons: 1 }));
  expectDropTarget(host, mode === "board");
  window.dispatchEvent(new PointerEvent("pointerup", { clientX: 100, clientY: 100, pointerId: 1 }));
  await new Promise<void>((done) => setTimeout(done, 0)); await tick();
  if (mode === "board") {
    expect(f.board).toHaveBeenCalledExactlyOnceWith("example-project", fromWriter[0].uuid, "todo", { kind: "send_to_team", team: teamView.bar.id });
  } else expect(f.board).not.toHaveBeenCalled();
});
