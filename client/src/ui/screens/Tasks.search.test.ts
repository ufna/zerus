// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { STAGES } from "./tasks-ui";
import type { Session } from "../sessions/types";
import { toSession } from "../../store/catalog";

const f = vi.hoisted(() => ({
  read: vi.fn(), write: vi.fn(), project: { id: "example-project", name: "Example", accessible: true },
  setSelected: (_session: Session | undefined) => {}, setOnly: (_only: boolean) => {},
  locale: "en" as "en" | "ru",
}));
vi.mock("../../i18n", async (original) => {
  const module = await original<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return { ...module, tr: (source: string, ...args: import("../../i18n").TranslationArgument[]) =>
    module.translateWithCatalog(catalog, f.locale, source, ...args) };
});
vi.mock("../../bridge", async (original) => {
  const module = await original<typeof import("../../bridge")>();
  return { ...module, taskBoardAction: f.write };
});
vi.mock("../../store/ui", async (original) => {
  const module = await original<typeof import("../../store/ui")>();
  const { createSignal } = await import("solid-js");
  const [selected, setSelected] = createSignal<Session>();
  f.setSelected = setSelected;
  return { ...module, selected };
});
vi.mock("../../store/settings", async (original) => {
  const module = await original<typeof import("../../store/settings")>();
  const { createSignal } = await import("solid-js");
  const [only, setOnly] = createSignal(false);
  f.setOnly = setOnly;
  return { ...module,
    flag: (key: string) => key === "tasks/onlySession" ? only() : module.flag(key),
    setSetting: async (key: string, value: Parameters<typeof module.setSetting>[1]) => key === "tasks/onlySession" ? void setOnly(!!value) : module.setSetting(key, value),
  };
});
vi.mock("../../store/tasks", async (original) => {
  const module = await original<typeof import("../../store/tasks")>();
  return { ...module, tasks: module.createTasks(async () => {}, f.read) };
});
vi.mock("../../store/taskContext", async () => {
  const { tasks } = await import("../../store/tasks");
  const { STAGES: stages } = await import("./tasks-ui");
  return {
    taskProject: () => f.project,
    projectSessions: () => [], findTaskSession: () => () => undefined, projectTasks: tasks.tasks,
    sessionTaskRows: () => tasks.session.active() ? Object.fromEntries(stages.map((s) => [s, tasks.session.rows(s)])) : undefined,
  };
});
vi.mock("./TaskCreate", () => ({ default: (props: { children: unknown }) => props.children }));
import { tasks } from "../../store/tasks";
import { TasksPanel } from "./Tasks";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; tasks.follow(undefined); f.setSelected(undefined); f.setOnly(false); f.locale = "en"; document.body.replaceChildren(); vi.useRealTimers(); vi.clearAllMocks(); });
const tick = async () => { for (let i = 0; i < 15; i++) await Promise.resolve(); };

it("binds the search field to bounded reads, keeps stage tabs and clears an empty result", async () => {
  vi.useFakeTimers();
  f.read.mockResolvedValue({ tasks: [], page: { next: null }, counts: Object.fromEntries(STAGES.map((s) => [s, 0])) });
  tasks.follow(f.project.id);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  const input = host.querySelector<HTMLInputElement>('input[aria-label="Search tasks"]')!;
  expect(input).not.toBeNull();
  input.value = "T-aaaa01"; input.dispatchEvent(new Event("input", { bubbles: true }));
  expect(f.read).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(200); await tick();
  expect(f.read).toHaveBeenCalledWith(f.project.id, "review", 50, null, undefined, "T-aaaa01");
  expect(f.read).toHaveBeenCalledWith(f.project.id, "active", 50, null, undefined, "T-aaaa01");
  expect(host.querySelector(".tk-empty")?.textContent).toContain("No tasks match your search");
  expect(host.querySelectorAll('[role="tab"]')).toHaveLength(6);
  host.querySelector<HTMLButtonElement>('[data-filter="todo"]')!.click(); await tick();
  expect(f.read).toHaveBeenLastCalledWith(f.project.id, "todo", 50, null, undefined, "T-aaaa01");
  host.querySelector<HTMLButtonElement>('[aria-label="Clear task search"]')!.click(); await tick();
  expect(host.querySelector<HTMLInputElement>('input[aria-label="Search tasks"]')!.value).toBe("");
  expect(tasks.session.active()).toBe(false);
  expect(host.querySelector('[data-filter="todo"]')?.getAttribute("aria-selected")).toBe("true");
});

it("never opens a global search when Only this session has no stable identity", async () => {
  vi.useFakeTimers();
  f.read.mockResolvedValue({ tasks: [], page: { next: null }, counts: Object.fromEntries(STAGES.map((s) => [s, 0])) });
  const legacy = toSession("", "name:codex/example/legacy", { name: "codex/example/legacy", state: "stopped" });
  expect([legacy.launchId, legacy.runId]).toEqual([undefined, undefined]);
  f.setSelected(legacy); f.setOnly(true);
  tasks.follow(f.project.id);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  const input = host.querySelector<HTMLInputElement>('input[aria-label="Search tasks"]')!;
  input.value = "Needle"; input.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await tick();
  expect(f.read, "an unknown session identity must never become a project-wide query").not.toHaveBeenCalled();
  expect(host.querySelectorAll(".tk-card")).toHaveLength(0);
  expect(host.querySelectorAll(".tk-more")).toHaveLength(0);
  expect(host.querySelector('[aria-label="Only this session"]')?.getAttribute("aria-pressed")).toBe("true");
  expect(host.querySelector(".tk-empty")?.textContent).toContain("This session has no stable identity");
  host.querySelector<HTMLButtonElement>('[data-filter="todo"]')!.click(); await tick();
  expect(f.read).not.toHaveBeenCalled();
  expect([...host.querySelectorAll(".tk-tile-count")].map((el) => el.textContent)).toEqual(["0", "0", "0", "0", "0", "0"]);
  // The explicit Only this session toggle can still open a project-wide search.
  host.querySelector<HTMLButtonElement>('[aria-label="Only this session"]')!.click(); await tick();
  expect(f.read).toHaveBeenLastCalledWith(f.project.id, "todo", 50, null, undefined, "Needle");
});

it("drops in-flight global results when an identity-less session filter is selected, then restores identified scope", async () => {
  vi.useFakeTimers();
  const pending: ((page: unknown) => void)[] = [];
  f.read.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
  tasks.follow(f.project.id);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  const input = host.querySelector<HTMLInputElement>('input[aria-label="Search tasks"]')!;
  input.value = "Needle"; input.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await tick();
  expect(f.read).toHaveBeenCalledTimes(2);
  f.setSelected(toSession("", "name:codex/example/legacy", { name: "codex/example/legacy", state: "stopped" }));
  f.setOnly(true); await tick();
  for (const resolve of pending) resolve({ tasks: [], page: { next: null }, counts: Object.fromEntries(STAGES.map((s) => [s, 123])) });
  await tick();
  expect(f.read).toHaveBeenCalledTimes(2);
  expect(tasks.session.active()).toBe(false);
  expect([...host.querySelectorAll(".tk-tile-count")].map((el) => el.textContent)).toEqual(["0", "0", "0", "0", "0", "0"]);
  expect(host.querySelector(".tk-empty")?.textContent).toContain("This session has no stable identity");
  f.setSelected(toSession("", "launch:identified", { name: "codex/example/identified", launch_id: "identified" }));
  await tick();
  expect(f.read).toHaveBeenCalledWith(f.project.id, "review", 50, null, { host: "", sessionId: "launch_id:identified" }, "Needle");
  expect(f.read).toHaveBeenLastCalledWith(f.project.id, "active", 50, null, { host: "", sessionId: "launch_id:identified" }, "Needle");
});

it.each([
  ["en", "In review", "Accept", ["Your decision", "Coordinator or reviewer next", "Coordinator to triage", "Coordinator to triage"]],
  ["ru", "На проверке", "Принять", ["Вам на решение", "Дальше проверяет координатор или ревьюер", "Координатору нужно уточнить, что дальше", "Координатору нужно уточнить, что дальше"]],
] as const)("shows who is expected to review each task in the shared review page in %s", async (locale, stageLabel, acceptLabel, owners) => {
  const reviewRows = (await import("./fixtures/task-review-assignment.json")).default;
  f.locale = locale;
  vi.useFakeTimers();
  f.read.mockImplementation(async (_project, stage) => ({
    tasks: stage === "review" ? reviewRows : [],
    page: { next: null },
    counts: { review: reviewRows.length, active: 0, todo: 0, backlog: 0, done: 0 },
  }));
  tasks.follow(f.project.id);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  host.querySelector<HTMLButtonElement>('[data-filter="review"]')!.click();
  const input = host.querySelector<HTMLInputElement>('.tk-search input')!;
  input.value = "sample";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await tick();

  expect(host.querySelector('[data-filter="review"]')?.textContent).toContain(stageLabel);
  const cards = [...host.querySelectorAll(".tk-card")];
  expect(cards).toHaveLength(4);
  expect(cards.map((card) => card.querySelector(".tk-review-owner")?.textContent)).toEqual(owners);
  expect(cards.every((card) => !!card.querySelector(`button[aria-label="${acceptLabel}"]`))).toBe(true);
  expect(f.write).not.toHaveBeenCalled();
  for (const n of [0, 1, 2]) {
    cards[n].querySelector<HTMLButtonElement>(`button[aria-label="${acceptLabel}"]`)!.click();
    await tick();
    expect(f.write).toHaveBeenLastCalledWith(f.project.id, reviewRows[n].uuid, "review", { kind: "accept" });
  }
  expect(f.write).toHaveBeenCalledTimes(3);
});

it.each([
  ["en", "Your decision", "Coordinator or reviewer next", "Coordinator to triage"],
  ["ru", "Вам на решение", "Дальше проверяет координатор или ревьюер", "Координатору нужно уточнить, что дальше"],
] as const)("uses projected execution across the normal board and bounded search in %s", async (locale, person, technical, triage) => {
  const { stream, search } = (await import("./fixtures/task-review-projected.json")).default;
  expect(search.tasks[4].execution?.conflict).toBe(true);
  expect(search.tasks[4].execution?.has_unknown).toBe(false);
  expect(search.tasks[5].execution?.has_unknown).toBe(true);
  f.locale = locale;
  vi.useFakeTimers();
  let failReview = false;
  f.read.mockImplementation(async (_project, stage) => {
    if (failReview && stage === "review") throw new Error("fixture review refresh failed");
    return stage === "review" ? search : { ...search, tasks: [] };
  });
  tasks.follow(f.project.id);
  tasks.apply([stream]);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  host.querySelector<HTMLButtonElement>('[data-filter="review"]')!.click();
  const labels = () => [...host.querySelectorAll(".tk-card")].map((card) => card.querySelector(".tk-review-owner")?.textContent);
  const expected = [person, technical, triage, undefined, triage, triage];
  expect(labels()).toEqual(expected);
  expect(f.read).not.toHaveBeenCalled();

  // Normal streamed failure withdraws the labels, even though the model
  // keeps the last rows. A fresh stream publication restores them.
  tasks.apply([{ ...stream, kind: "failed", payload: { message: "fixture stream ended" } }]);
  expect(host.querySelector(".tk-review-owner")).toBeNull();
  tasks.apply([stream]);
  expect(labels()).toEqual(expected);

  // Use Current so Review and In progress refresh independently.
  host.querySelector<HTMLButtonElement>('[data-filter="current"]')!.click();
  const input = host.querySelector<HTMLInputElement>(".tk-search input")!;
  input.value = "sample";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await tick();
  expect(f.read).toHaveBeenCalledWith(f.project.id, "review", 50, null, undefined, "sample");
  expect(labels()).toEqual(expected);
  expect(host.querySelector('[data-filter="review"] .tk-tile-count')?.textContent).toBe("6");

  failReview = true;
  tasks.session.changed(); await tick();
  expect(host.querySelector(".tk-line[role=status]")?.textContent).toContain("fixture review refresh failed");
  expect(labels()).toEqual(Array(6).fill(undefined));
  expect(host.querySelector('[data-filter="review"] .tk-tile-count')?.textContent).toBe("6");

  failReview = false;
  tasks.session.changed(); await tick();
  expect(labels()).toEqual(expected);
  expect(host.querySelector(".tk-line[role=status]")).toBeNull();
  expect(f.write).not.toHaveBeenCalled();
});

it("stream reconnect keeps old search routing labels hidden until the scoped Review read completes", async () => {
  const { stream, search } = (await import("./fixtures/task-review-projected.json")).default;
  vi.useFakeTimers();
  const pending: ((page: unknown) => void)[] = [];
  let reconnect = false;
  f.read.mockImplementation(async (_project, stage) => {
    if (stage !== "review") return { ...search, tasks: [] };
    if (reconnect) return new Promise(resolve => { pending.push(resolve); });
    return search;
  });
  tasks.follow(f.project.id);
  tasks.apply([stream]);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  const input = host.querySelector<HTMLInputElement>('.tk-search input')!;
  input.value = "sample";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await tick();
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(5);
  reconnect = true;
  tasks.session.changed(); await tick();
  expect(pending).toHaveLength(1);
  tasks.apply([{ ...stream, kind: "failed", payload: { message: "project stream ended" } }]);
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(0);
  tasks.apply([{ ...stream, payload: { ...stream.payload, tasks: [], digest: "reconnected-window" } }]);
  await tick();
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(0);
  pending[0](search); await tick();
  expect(pending).toHaveLength(2);
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(0);
  pending[1](search); await tick();
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(5);
});

it("stream reconnect waits for retained window pages before showing their next-reviewer labels", async () => {
  const { stream, search } = (await import("./fixtures/task-review-projected.json")).default;
  let resolveReview!: (page: unknown) => void;
  let reconnect = false;
  f.read.mockImplementation(async (_project, stage, _limit, after) => {
    expect(stage).toBe("review");
    if (reconnect) return new Promise(resolve => { resolveReview = resolve; });
    expect(after).toBe("review-tail");
    return { ...search, tasks: search.tasks.slice(1) };
  });
  const windowed = { ...stream, payload: { ...stream.payload, tasks: stream.payload.tasks.slice(0, 1),
    window: { window: { stages: { review: { total: 6, next: "review-tail" } } } } } };
  tasks.follow(f.project.id);
  tasks.apply([windowed]);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TasksPanel, {}), host);
  await tick();
  host.querySelector<HTMLButtonElement>('[data-filter="review"]')!.click();
  const showMore = host.querySelector<HTMLButtonElement>(".tk-more")!;
  expect(showMore).not.toBeNull();
  showMore.click(); await tick();
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(5);
  tasks.apply([{ ...windowed, kind: "failed", payload: { message: "project stream ended" } }]);
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(0);
  reconnect = true;
  tasks.apply([{ ...windowed, payload: { ...windowed.payload, digest: "reconnected-window" } }]);
  await tick();
  expect(typeof resolveReview).toBe("function");
  const tail = () => host.querySelector(`[data-task="${search.tasks[1].uuid}"] .tk-review-owner`);
  expect(tail()).toBeNull();
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(1);
  resolveReview(search); await tick();
  expect(tail()?.textContent).toBe("Coordinator or reviewer next");
  expect(host.querySelectorAll(".tk-review-owner")).toHaveLength(5);
});
