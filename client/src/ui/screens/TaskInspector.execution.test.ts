// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { taskDetails, type TaskDetail } from "../../store/taskDetails";
import reviewRows from "./fixtures/task-review-assignment.json";
const state = vi.hoisted(() => ({ detail: undefined as TaskDetail | undefined, error: "", offline: "", project: "orchard", locale: "en" as "en" | "ru", projected: false, classify: vi.fn(), read: vi.fn(), setLinkError: (_message: string) => {} }));
vi.mock("../../i18n", async (original) => {
  const module = await original<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return { ...module, tr: (source: string, ...args: import("../../i18n").TranslationArgument[]) =>
    module.translateWithCatalog(catalog, state.locale, source, ...args) };
});
vi.mock("../../bridge", () => ({ onTaskExecutors: async () => () => {}, attachmentPin: vi.fn(), attachmentUnpin: vi.fn(), inTauri: () => false, openTaskAttachment: vi.fn(), readTaskDetails: vi.fn(), classifyTaskReview: state.classify }));
vi.mock("../../store/ui", () => ({ selected: () => undefined, setMenu: vi.fn() }));
vi.mock("../../store/taskBoard", () => ({ writingTask: () => undefined }));
vi.mock("../../store/taskAssignment", () => ({ openTaskAssignment: vi.fn() }));
vi.mock("../../store/taskExecutors", () => ({ pendingExecutor: () => undefined, taskExecutors: { follow: vi.fn(), refresh: vi.fn() }, setTaskExecutorReview: vi.fn() }));
vi.mock("../../store/taskStart", () => ({ taskStarts: { follow: vi.fn() }, supportsTaskStart: () => false, taskStartBlocker: () => undefined, pendingStart: () => undefined, reviewTaskStart: vi.fn() }));
vi.mock("../../store/taskCancel", () => ({ PAUSE_REASON: "Individual-task pause is unavailable.", requestAttemptCancellation: vi.fn() }));
vi.mock("../../store/taskDetails", async (original) => {
  const module = await original<typeof import("../../store/taskDetails")>();
  const real = module.createTaskDetails((project, task, cursor) => state.read(project, task, cursor));
  return { ...module, taskDetails: {
    ...real,
    data: () => state.projected ? real.data() : state.detail,
    error: () => state.projected ? real.error() : state.error,
    select: (project?: string, uuid?: string) => { if (state.projected) real.select(project, uuid); },
  } };
});
vi.mock("../../store/tasks", async () => {
  const { createSignal } = await import("solid-js");
  const [linkError, setLinkError] = createSignal("");
  state.setLinkError = setLinkError;
  return { tasks: { error: () => linkError() || state.offline, project: () => state.project } };
});
vi.mock("../../store/taskFiles", () => ({ taskFileCard: vi.fn(), taskFiles: {} }));
vi.mock("../../store/taskNavigation", () => ({ taskRenameRequest: () => undefined, setTaskRenameRequest: vi.fn(), taskDescriptionRequest: () => undefined, setTaskDescriptionRequest: vi.fn() }));
vi.mock("./TaskWorkflow", () => ({ TaskWorkflow: () => null }));
vi.mock("./TaskTitle", () => ({ TaskTitle: () => null }));
vi.mock("./TaskInput", () => ({ TaskInput: () => null }));
vi.mock("../lib/icons", () => ({ Icon: () => null }));
vi.mock("../sessions/Activity", () => ({ Body: () => null }));
import { TaskInspector } from "./TaskInspector";
import { taskItem } from "./tasks-ui";

const tick = async () => { for (let n = 0; n < 15; n++) await Promise.resolve(); };
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); state.error = ""; state.offline = ""; state.project = "orchard"; state.locale = "en"; state.projected = false; state.read.mockReset(); state.classify.mockReset(); state.setLinkError(""); });
function mount(reviewRow?: unknown, projected = false) {
  const task = taskItem(reviewRow ?? { uuid: "task", id: "T-234567", stage: "active", writable: false });
  const execution = reviewRow ? undefined : { requirement: 3, attempts: 1, current: null, conflict: false, has_unknown: true, has_unexplained: false };
  state.detail = { ...task, execution,
    files: [], log: [], total: 0, started: false, dispatch: "", unknownAttempts: [] };
  state.projected = projected;
  const host = document.createElement("div"); document.body.append(host);
  const project = projected ? state.project : "orchard";
  dispose = render(() => createComponent(TaskInspector, { project, task, executorAvailable: false, openExecutor: vi.fn() }), host);
  return host;
}
it("the actual inspector renders the selected detail's execution even when the board row has none", () => {
  const host = mount(), view = host.querySelector('[aria-label="Saved execution"]');
  expect(view).not.toBeNull();expect(view?.textContent).toContain("No current attempt recorded.");
  expect(view?.textContent).toContain("Some attempt delivery is unknown.");expect(view?.querySelectorAll("button,a")).toHaveLength(0);
});
it("a disconnected task subscription marks the retained saved evidence as stale", () => {
  state.offline = "connection ended";
  const host = mount();expect(host.querySelector(".tv-execution")?.textContent).toContain("could not be refreshed");
});

it("another project's subscription failure does not mark the selected detail as stale", () => {
  state.project = "another-project";state.offline = "connection ended";
  const host = mount();expect(host.querySelector(".tv-execution")?.textContent).not.toContain("could not be refreshed");
});

it("shows the saved acceptance criterion beside the review owner", () => {
  const host = mount(reviewRows[0]), assignment = host.querySelector('[aria-label="Review assignment"]');
  expect(assignment?.textContent).toContain("Your decision");
  expect(assignment?.textContent).toContain("Acceptance criterion: Approve the sample output");
});

it.each([
  ["en", "Review assignment", "Acceptance criterion: Check the fixture", "Your decision", "Coordinator or reviewer next", "Coordinator to triage"],
  ["ru", "Кто проверяет", "Критерий приёмки: Check the fixture", "Вам на решение", "Дальше проверяет координатор или ревьюер", "Координатору нужно уточнить, что дальше"],
] as const)("uses the native detail projection and keeps only the criterion on stale reads in %s", async (locale, heading, criterion, person, technical, triage) => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  state.locale = locale;
  state.project = "example-project";
  const expected = [person, technical, triage, undefined, triage, triage];
  for (let n = 0; n < details.length; n++) {
    state.read.mockResolvedValueOnce(details[n]);
    const host = mount(details[n], true);
    await tick();
    expect(state.read).toHaveBeenLastCalledWith(state.project, details[n].uuid, null);
    const assignment = () => host.querySelector(`[aria-label="${heading}"]`);
    if (expected[n] === undefined) expect(assignment()).toBeNull();
    else expect(assignment()?.textContent).toContain(expected[n]);
    if (n === 0 || n === 1) {
      state.read.mockRejectedValueOnce(new Error("fixture refresh failed"));
      taskDetails.refresh(); await tick();
      expect(host.querySelector("[role=alert]")?.textContent).toContain("fixture refresh failed");
      expect(assignment()?.textContent).toContain(criterion);
      expect(assignment()?.textContent).not.toContain(expected[n]);
      state.read.mockResolvedValueOnce(details[n]);
      taskDetails.refresh(); await tick();
      expect(assignment()?.textContent).toContain(expected[n]);
    }
    dispose?.(); host.remove();
  }
});


it("keeps the prior review owner hidden across superseded detail reads", async () => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const { createSignal } = await import("solid-js");
  const old = details[1], done = { ...old, stage: "done", review_assignment: undefined };
  state.project = old.project;
  state.projected = true;
  state.read.mockResolvedValueOnce(old);
  const pending: ((value: unknown) => void)[] = [];
  state.read.mockImplementation(() => new Promise(resolve => { pending.push(resolve); }));
  const [task, updateTask] = createSignal(taskItem({ ...old, history_revision: "one" }));
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TaskInspector, {
    project: old.project, get task() { return task(); }, executorAvailable: false, openExecutor: vi.fn(),
  }), host);
  await tick();
  const label = () => host.querySelector('[aria-label="Review assignment"]')?.textContent;
  expect(label()).toContain("Coordinator or reviewer next");
  taskDetails.refresh();
  state.setLinkError("project stream ended");
  updateTask(taskItem({ ...done, history_revision: "two" }));
  state.setLinkError("");
  await tick();
  expect(pending).toHaveLength(1);
  expect(label()).not.toContain("Coordinator or reviewer next");
  pending[0](old); await tick();
  expect(pending).toHaveLength(2);
  expect(label()).not.toContain("Coordinator or reviewer next");
  pending[1](done); await tick();
  expect(label()).toBeUndefined();
});


it.each([
  ["en", "Choose next reviewer", "Acceptance criterion", "Save reviewer"],
  ["ru", "Выбрать следующего проверяющего", "Критерий приёмки", "Сохранить проверяющего"],
] as const)("classifies a settled ordinary report with its exact revision in %s", async (locale, chooseLabel, criterionLabel, saveLabel) => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const before = { ...details[2], review_lane: "triage", review_editable: true, history_revision: "a".repeat(64) };
  const after = { ...before, review_lane: "person", history_revision: "b".repeat(64),
    review_assignment: { owner: "person", criterion: "Read the release output" } };
  state.project = before.project;
  state.locale = locale;
  state.read.mockResolvedValueOnce(before).mockResolvedValue(after);
  state.classify.mockResolvedValue(undefined);
  const host = mount(before, true);
  await tick();
  const button = (text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent === text)!;
  button(chooseLabel).click();
  const owner = host.querySelector<HTMLSelectElement>(".tv-review-editor select")!;
  owner.value = "person";
  owner.dispatchEvent(new Event("change", { bubbles: true }));
  const field = host.querySelector<HTMLInputElement>(".tv-review-editor input")!;
  expect(field.closest("label")?.textContent).toContain(criterionLabel);
  expect(button(saveLabel).disabled).toBe(true);
  field.value = "Read the release output";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  const form = host.querySelector<HTMLFormElement>(".tv-review-editor")!;
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(state.classify).toHaveBeenCalledExactlyOnceWith(before.project, before.uuid, before.history_revision, "person", "Read the release output");
  expect(host.querySelector(".tv-review-editor")).toBeNull();
  expect(host.querySelector(".tv-stage")?.classList.contains("st-review")).toBe(true);
  expect(host.textContent).toContain("Read the release output");
});
