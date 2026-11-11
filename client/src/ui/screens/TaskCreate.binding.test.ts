// @vitest-environment happy-dom
// The New task form's team choice is bound to the form's project and to a new
// card (finding 9 of the second review): another project clears it, a choice
// no longer offered is refused, and a reserved card keeps the destination it
// was reserved with.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({
  create: vi.fn(async (..._args: unknown[]) => false),
  view: (() => undefined) as () => unknown,
  setView: (_value: unknown) => {},
  list: (() => undefined) as () => unknown,
  setList: (_value: unknown) => {},
}));
const TEAM = "33333333-3333-4333-8333-333333333333";
const OTHER = "44444444-4444-4444-8444-444444444444";
vi.mock("../../store/taskForms", async () => {
  const { createSignal: signal } = await import("solid-js");
  const [view, setView] = signal<unknown>();
  f.view = view; f.setView = setView;
  return {
    createCard: f.create,
    newCard: { follow: vi.fn(), locked: () => false, opening: () => false, busy: () => false, view, edit: vi.fn(), flush: vi.fn(async () => {}), error: () => "" },
  };
});
vi.mock("../../store/ui", () => ({ selected: () => undefined }));
vi.mock("../../store/taskNavigation", () => ({ requestTaskDescription: vi.fn() }));
vi.mock("../../store/teams", async () => {
  const { createSignal: signal } = await import("solid-js");
  const [list, setList] = signal<unknown>();
  f.list = list; f.setList = setList;
  return { teams: { available: () => true, list } };
});
import TaskCreate from "./TaskCreate";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); vi.clearAllMocks(); });

const fixes = { id: TEAM, project: "example-project", name: "Fixes" };
const card = (project: string, over: Record<string, unknown> = {}) => ({ project, text: "Task title", handle: 1, blocked: false, read_only: false, recovery: false, error: "", ...over });
function setup(project: () => string) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TaskCreate, { get project() { return project(); }, scope: "1 task", backlog: false, accessible: true, children: null }), host);
  (host.querySelector('[aria-label="New task"]') as HTMLButtonElement).click();
  return host;
}
const chooser = (host: HTMLElement) => host.querySelector<HTMLButtonElement>("button.tk-team");
const enter = (host: HTMLElement) => (host.querySelector("input.tk-newtitle") as HTMLInputElement).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
const choose = (host: HTMLElement) => chooser(host)!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));

it("does not submit a previous project's hidden team choice", async () => {
  const [project, setProject] = createSignal("example-project");
  f.setList({ teams: [fixes], truncated: false, next: null });
  f.setView(card("example-project"));
  const host = setup(project);
  choose(host);
  expect(chooser(host)!.textContent).toContain("Fixes");
  f.setView(card("other-project"));
  setProject("other-project");
  (host.querySelector('[aria-label="New task"]') as HTMLButtonElement).click();
  expect(chooser(host)).toBeNull();
  enter(host);
  await vi.waitFor(() => expect(f.create).toHaveBeenCalled());
  expect(f.create).toHaveBeenCalledWith("other-project", false, undefined, undefined);
});

it("refuses a choice the form no longer offers and sends nothing", async () => {
  f.setList({ teams: [fixes, { id: OTHER, project: "example-project", name: "Other" }], truncated: false, next: null });
  f.setView(card("example-project"));
  const host = setup(() => "example-project");
  choose(host);
  expect(chooser(host)!.textContent).toContain("Fixes");
  // The team leaves the list while the form is open.
  f.setList({ teams: [{ id: OTHER, project: "example-project", name: "Other" }], truncated: false, next: null });
  enter(host);
  await Promise.resolve();
  expect(f.create).not.toHaveBeenCalled();
  expect(host.querySelector(".tk-create-error")?.textContent).toContain("no longer offered");
});

it("a reserved card keeps its destination and the chooser cannot change it", async () => {
  f.setList({ teams: [fixes, { id: OTHER, project: "example-project", name: "Other" }], truncated: false, next: null });
  f.setView(card("example-project", { recovery: true, team: TEAM }));
  const host = setup(() => "example-project");
  expect(chooser(host)!.disabled).toBe(true);
  expect(chooser(host)!.textContent).toContain("Fixes");
  enter(host);
  await vi.waitFor(() => expect(f.create).toHaveBeenCalled());
  expect(f.create).toHaveBeenCalledWith("example-project", false, undefined, undefined);
});

it("an older reserved card without a team shows no chooser and sends no team", async () => {
  f.setList({ teams: [fixes], truncated: false, next: null });
  f.setView(card("example-project", { recovery: true }));
  const host = setup(() => "example-project");
  expect(chooser(host)).toBeNull();
  enter(host);
  await vi.waitFor(() => expect(f.create).toHaveBeenCalled());
  expect(f.create).toHaveBeenCalledWith("example-project", false, undefined, undefined);
});
