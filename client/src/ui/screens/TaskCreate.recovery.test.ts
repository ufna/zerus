// @vitest-environment happy-dom
// The recovery of a card created but not queued (third confirmation, findings
// 2 and 3): Send waits for the created task, read exactly by its identity or
// held by the board, and never calls an unread task changed; a reply settles
// only the notice it was started for; one Send at a time. The real Send to
// team runs here; the native board action, the exact read and the board's
// cache are fakes.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({
  board: vi.fn(async (..._args: unknown[]) => {}), notify: vi.fn(), reads: [] as { key: string; resolve: (row: unknown) => void; reject: (error: unknown) => void }[],
  setCached: (_: unknown) => {}, create: vi.fn(async (..._args: unknown[]) => false),
  view: (() => undefined) as () => unknown, setView: (_value: unknown) => {}, list: (() => undefined) as () => unknown, setList: (_value: unknown) => {},
}));
const TEAM = "33333333-3333-4333-8333-333333333333";
const FIRST = "11111111-1111-4111-8111-111111111111";
const SECOND = "22222222-2222-4222-8222-222222222222";
vi.mock("../../store/taskForms", async () => {
  const { createSignal } = await import("solid-js");
  const [view, setView] = createSignal<unknown>();
  f.view = view; f.setView = setView;
  return { createCard: f.create, newCard: { follow: vi.fn(), locked: () => false, opening: () => false, busy: () => false, view, edit: vi.fn(), flush: vi.fn(async () => {}), error: () => "" } };
});
vi.mock("../../store/ui", () => ({ selected: () => undefined }));
vi.mock("../../bridge", async (original) => ({ ...(await original() as object), taskBoardAction: f.board }));
vi.mock("./task-view-read", () => ({ readSelectedTask: (key: string) => new Promise((resolve, reject) => f.reads.push({ key, resolve, reject })) }));
vi.mock("../../store/tasks", async () => {
  const { createSignal } = await import("solid-js");
  const [cached, setCached] = createSignal<unknown>();
  f.setCached = setCached;
  return { tasks: { project: () => "example-project", find: (uuid: string) => (cached() as { uuid?: string } | undefined)?.uuid === uuid ? cached() : undefined } };
});
vi.mock("../../store/actions", async (original) => ({ ...(await original() as object), notify: f.notify }));
vi.mock("../../store/taskNavigation", () => ({ requestTaskDescription: vi.fn() }));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [list, setList] = createSignal<unknown>();
  f.list = list; f.setList = setList;
  return { teams: { available: () => true, list } };
});
import TaskCreate from "./TaskCreate";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); vi.clearAllMocks(); f.reads = []; f.setCached(undefined); });

const fixes = { id: TEAM, project: "example-project", name: "Fixes" };
const card = (over: Record<string, unknown> = {}) => ({ project: "example-project", text: "Task title", handle: 1, blocked: false, read_only: false, recovery: false, error: "", ...over });
const row = (uuid: string, over: Record<string, unknown> = {}) => ({ uuid, stage: "todo", writable: true, ...over });
const settle = async () => { for (let n = 0; n < 10; n++) await Promise.resolve(); };
function setup() {
  f.setList({ teams: [fixes], truncated: false, next: null });
  f.setView(card());
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TaskCreate, { project: "example-project", scope: "1 task", backlog: false, accessible: true, children: null }), host);
  (host.querySelector('[aria-label="New task"]') as HTMLButtonElement).click();
  return host;
}
const choose = (host: HTMLElement) => host.querySelector<HTMLButtonElement>("button.tk-team")!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
const enter = (host: HTMLElement) => (host.querySelector("input.tk-newtitle") as HTMLInputElement).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
const button = (host: HTMLElement, label: string) => [...host.querySelectorAll<HTMLButtonElement>("[data-not-queued] button")].find((candidate) => candidate.textContent === label);
async function createNotQueued(host: HTMLElement, id: string, reason: string) {
  f.create.mockImplementationOnce(async () => { f.setView(card({ text: "", created: id, not_queued: { task: id, team: TEAM, reason } })); return true; });
  f.setView(card({ text: `Card ${id.slice(0, 1)}` }));
  (host.querySelector('[aria-label="New task"]') as HTMLButtonElement).click();
  choose(host); enter(host);
  await vi.waitFor(() => expect(host.querySelector("[data-not-queued]")?.textContent).toContain(reason));
}

it("waits for the created task, never calls it changed, then sends with the row it read (finding 2)", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "The team is stopped.");
  expect(f.reads.map((read) => read.key)).toEqual([`example-project\n${FIRST}`]);
  const send = button(host, "Send to team")!;
  expect(send.disabled).toBe(true);
  expect(send.getAttribute("aria-busy")).toBe("true");
  expect(host.querySelector("[data-recovery-reading]")?.textContent).toBe("Reading the task…");
  send.click(); await settle();
  expect(f.board).not.toHaveBeenCalled();
  expect(f.notify.mock.calls.flat().join(" ")).not.toContain("changed");
  f.reads[0].resolve(row(FIRST)); await settle();
  expect(send.disabled).toBe(false);
  send.click(); await settle();
  expect(f.board).toHaveBeenCalledWith("example-project", FIRST, "todo", { kind: "send_to_team", team: TEAM });
  expect(host.querySelector("[data-not-queued]")).toBeNull();
});

it("the board's own row, when it arrives first, is enough (the reviewer's cache half)", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "The team is stopped.");
  const send = button(host, "Send to team")!;
  expect(send.disabled).toBe(true);
  f.setCached(row(FIRST)); await settle();
  expect(send.disabled).toBe(false);
  send.click(); await settle();
  expect(f.board).toHaveBeenCalledTimes(1);
});

it("a failed read says so and can be read again; the existing checks still refuse a task that left To do", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "The team is stopped.");
  f.reads[0].reject(new Error("offline")); await settle();
  expect(host.querySelector("[data-recovery-unread]")?.textContent).toContain("offline");
  expect(button(host, "Send to team")!.disabled).toBe(true);
  button(host, "Read again")!.click();
  expect(f.reads).toHaveLength(2);
  f.reads[1].resolve(row(FIRST, { stage: "active" })); await settle();
  button(host, "Send to team")!.click(); await settle();
  expect(f.board).not.toHaveBeenCalled();
  expect(host.querySelector("[data-recovery-refusal]")?.textContent).toContain("The task changed");
  expect(host.querySelector("[data-not-queued]"), "the notice stays with the reason").not.toBeNull();
});

it("a late read for a dismissed notice changes nothing", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "First not queued");
  button(host, "Dismiss")!.click();
  await createNotQueued(host, SECOND, "Second not queued");
  f.reads[0].resolve(row(FIRST)); await settle();
  expect(button(host, "Send to team")!.disabled, "the first task's read does not ready the second notice").toBe(true);
  f.reads[1].resolve(row(SECOND)); await settle();
  expect(button(host, "Send to team")!.disabled).toBe(false);
});

it("an earlier Send's reply cannot settle a later task's notice (finding 3)", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "First not queued");
  f.reads[0].resolve(row(FIRST)); await settle();
  let finish!: () => void;
  f.board.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
  button(host, "Send to team")!.click(); await settle();
  expect(button(host, "Send to team")!.disabled, "one Send at a time").toBe(true);
  button(host, "Send to team")!.click(); await settle();
  expect(f.board).toHaveBeenCalledTimes(1);
  button(host, "Dismiss")!.click();
  f.setView(card({ text: "Second task" }));
  await createNotQueued(host, SECOND, "Second not queued");
  finish(); await settle();
  expect(host.querySelector("[data-not-queued]")?.textContent).toContain("Second not queued");
  expect(host.querySelector("[data-recovery-refusal]")).toBeNull();
});

it("an earlier Send's refusal is not shown on a later notice", async () => {
  const host = setup();
  await createNotQueued(host, FIRST, "First not queued");
  f.reads[0].resolve(row(FIRST)); await settle();
  let refuse!: (error: unknown) => void;
  f.board.mockImplementationOnce(() => new Promise<void>((_resolve, reject) => { refuse = reject; }));
  button(host, "Send to team")!.click(); await settle();
  await createNotQueued(host, SECOND, "Second not queued");
  refuse("invalid_state: The first team is stopped."); await settle();
  expect(host.querySelector("[data-recovery-refusal]")).toBeNull();
  expect(host.querySelector("[data-not-queued]")?.textContent).toContain("Second not queued");
});
