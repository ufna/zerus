// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { taskDetails, type TaskDetail } from "../../store/taskDetails";
import reviewRows from "./fixtures/task-review-assignment.json";
const state = vi.hoisted(() => ({ detail: undefined as TaskDetail | undefined, error: "", offline: "", project: "orchard", locale: "en" as "en" | "ru", projected: false, classify: vi.fn(), quickReview: vi.fn(), submit: vi.fn(), teamRead: vi.fn(), teamLive: vi.fn(), teamListed: vi.fn(), setTeamFailure: (_failed: boolean) => {}, read: vi.fn(), setLinkError: (_message: string) => {} }));
vi.mock("../../i18n", async (original) => {
  const module = await original<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return { ...module, tr: (source: string, ...args: import("../../i18n").TranslationArgument[]) =>
    module.translateWithCatalog(catalog, state.locale, source, ...args) };
});
vi.mock("../../bridge", async (original) => ({ ...await original<typeof import("../../bridge")>(), onTaskExecutors: async () => () => {}, attachmentPin: vi.fn(), attachmentUnpin: vi.fn(), inTauri: () => false, openTaskAttachment: vi.fn(), readTaskDetails: vi.fn(), classifyTaskReview: state.classify, changeTaskQuickReview: state.quickReview }));
vi.mock("../../store/ui", () => ({ selected: () => undefined, setMenu: vi.fn() }));
vi.mock("../../store/taskBoard", () => ({ writingTask: () => undefined, setWritingTask: vi.fn() }));
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
vi.mock("../../store/taskEditor", () => ({ taskEditor: {
  view: () => ({ project: state.project, task: state.detail?.uuid, mode: "note", recovery: false }),
  locked: () => false, submit: state.submit,
} }));
vi.mock("../../store/machines", async (original) => ({ ...await original<typeof import("../../store/machines")>(), machineCards: {
  "": { machine_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
  peer: { machine_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb" },
} }));
vi.mock("../../store/taskContext", () => ({ projectSessions: () => [
  { computer: "", name: "codex/local-checker", runId: "local-run" },
  { computer: "peer", name: "codex/peer-checker", runId: "peer-run" },
] }));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [failed, setFailed] = createSignal(false);
  state.setTeamFailure = setFailed;
  return { teams: { currentTeam: state.teamLive, listed: () => state.teamListed() ?? [],
    available: () => true, readFailureFor: () => failed() ? { code: "storage_error" } : null,
    readTeamSnapshot: state.teamRead } };
});
vi.mock("../teams/team-navigation", () => ({ openTeamPage: vi.fn() }));
vi.mock("./TaskWorkflow", () => ({ TaskWorkflow: () => null }));
vi.mock("./TaskTitle", () => ({ TaskTitle: () => null }));
vi.mock("./TaskInput", () => ({ TaskInput: () => null }));
vi.mock("../lib/icons", () => ({ Icon: () => null }));
vi.mock("../sessions/Activity", () => ({ Body: () => null }));
import { TaskInspector } from "./TaskInspector";
import { taskItem } from "./tasks-ui";
import { reviewOutcome } from "./TaskReviewOutcome";

const tick = async () => { for (let n = 0; n < 15; n++) await Promise.resolve(); };
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); state.error = ""; state.offline = ""; state.project = "orchard"; state.locale = "en"; state.projected = false; state.read.mockReset(); state.classify.mockReset(); state.quickReview.mockReset(); state.submit.mockReset(); state.teamRead.mockReset(); state.teamLive.mockReset(); state.teamListed.mockReset(); state.setTeamFailure(false); state.setLinkError(""); });
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

it.each([
  ["en", "Record review outcome…", "Save review outcome", "Accept result", "Report under review", "View report in History"],
  ["ru", "Записать итог проверки…", "Сохранить итог проверки", "Принять результат", "Отчёт на проверке", "Показать отчёт в истории"],
] as const)("uses the exact report in ordinary review forms and History in %s", async (locale, open, save, accept, reportChip, history) => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const original = { ...details[2], review_report_id: "scoped-report:one", quick_review_status: "unclassified",
    history_revision: "a".repeat(64), log: [{ id: "scoped-report:one", kind: "report", text: "Synthetic result" }] };
  const newer = { ...original, history_revision: "b".repeat(64), review_report_id: "scoped-report:two",
    log: [...original.log, { id: "scoped-report:two", kind: "report", text: "New synthetic result" }] };
  const ready = { ...newer, history_revision: "c".repeat(64), quick_review_status: "ready",
    quick_review: { kind: "ready", evidence: "The retained trace proves the criterion." } };
  state.project = original.project; state.locale = locale;
  state.read.mockResolvedValue(original);
  state.quickReview.mockResolvedValue(undefined);
  const host = mount(original, true);
  await tick();
  const button = (label: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent === label)!;
  const submit = () => host.querySelector("form.tv-review-editor")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  const enter = (value: string) => {
    const field = host.querySelector<HTMLTextAreaElement>(".tv-review-editor textarea")!;
    field.value = value; field.dispatchEvent(new Event("input", { bubbles: true }));
  };
  button(open).click();
  const kind = host.querySelector<HTMLSelectElement>(".tv-review-editor select")!;
  kind.value = "check"; kind.dispatchEvent(new Event("change", { bubbles: true }));
  const assignee = [...host.querySelectorAll<HTMLSelectElement>(".tv-review-editor select")].at(-1)!;
  expect([...assignee.options].map(v => v.value)).toEqual(["", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\nrun_id:local-run"]);
  enter("Check the published result.");
  expect(button(save).disabled).toBe(true);
  assignee.value = assignee.options[1].value;
  assignee.dispatchEvent(new Event("change", { bubbles: true }));
  expect(button(save).disabled).toBe(false);
  state.quickReview.mockRejectedValueOnce("hgs: The selected review assignee is unavailable.");
  submit(); await tick();
  expect(host.querySelector(".tv-doc [role=alert]")?.textContent).toContain(locale === "en"
    ? "The selected session could not be verified."
    : "Не удалось подтвердить выбранную сессию.");
  expect(host.querySelector<HTMLTextAreaElement>(".tv-review-editor textarea")?.value).toBe("Check the published result.");
  state.quickReview.mockClear();
  state.read.mockResolvedValue(newer);
  taskDetails.refresh(); await tick();
  expect(button(save).disabled).toBe(true);
  submit();
  expect(state.quickReview).not.toHaveBeenCalled();

  host.querySelector<HTMLButtonElement>(".tv-review-buttons button:last-child")!.click();
  button(open).click();
  enter(ready.quick_review.evidence);
  state.read.mockResolvedValue(ready);
  submit(); await tick();
  expect(state.quickReview).toHaveBeenCalledExactlyOnceWith(ready.project, ready.uuid,
    { op: "set", history_revision: newer.history_revision, report_id: newer.review_report_id, disposition: ready.quick_review });
  button(history).click(); await tick();
  expect([...host.querySelectorAll("[data-reviewed-report]")].map(row => row.getAttribute("data-reviewed-report"))).toEqual([newer.review_report_id]);
  expect(host.querySelector("[data-reviewed-report]")?.textContent).toContain(reportChip);
  host.querySelector<HTMLButtonElement>(".tv-back")!.click();
  button(accept).click(); await tick();
  expect(state.quickReview).toHaveBeenLastCalledWith(ready.project, ready.uuid,
    { op: "accept", history_revision: ready.history_revision, report_id: ready.review_report_id });
});

it("never gives reconciliation or a Team task ordinary verdict or acceptance controls", async () => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const base = { ...details[2], review_report_id: "other-route", history_revision: "d".repeat(64),
    quick_review: { kind: "ready", evidence: "A stale ordinary decision" } };
  state.project = base.project;
  for (const variant of [
    { ...base, quick_review_status: "unresolved", route: "Review outcome" },
    { ...base, quick_review_status: "ready", team: {
      team: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", home: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    }, route: "Team review" },
  ]) {
    state.read.mockResolvedValue(variant);
    const host = mount(variant, true);
    await tick();
    expect(host.querySelector(`[aria-label="${variant.route}"]`)).not.toBeNull();
    expect(host.textContent).not.toContain(base.quick_review.evidence);
    expect(host.textContent).not.toContain("Accept result");
    expect(host.textContent).not.toContain("Record review outcome");
    expect(host.textContent?.includes("Open team page")).toBe(variant.route === "Team review");
    dispose?.(); host.remove();
  }
  expect(state.quickReview).not.toHaveBeenCalled();
  expect(state.submit).not.toHaveBeenCalled();
});

it.each(["en", "ru"] as const)("shows native Team facts only from the selected Team's own scope in %s", async (locale) => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", home = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  const selected = { ...details[2], quick_review_status: "team", team: { team: id, home } };
  const native = {
    bar: { id, home, project: selected.project }, revision: 9, settings: { phase_acceptance: "person" },
    plan: { list: { rows: [{ n: 2, acceptance: "lead" }] } },
    groups: [{ cards: [{
      node: 6, kind: "task", task: { uuid: selected.uuid, project: selected.project },
      phase: 2, state: "in_progress", holder: "past-holder",
      stages: [{ state: "reported", holder: "fixture-current-holder" }],
    }] }],
  };
  state.project = selected.project; state.locale = locale;
  state.read.mockResolvedValue(selected);
  const pending: ((reply: unknown) => void)[] = [];
  state.teamRead.mockImplementation(() => new Promise(resolve => pending.push(resolve)));
  const host = mount(selected, true);
  await tick();
  expect(state.teamRead).toHaveBeenCalledExactlyOnceWith(id, 0);
  expect(host.textContent).not.toContain("fixture-current-holder");
  pending.shift()!({ ...native, bar: { ...native.bar, home: id } }); await tick();
  expect(host.textContent).not.toContain("fixture-current-holder");
  dispose?.(); host.remove();

  const active = mount(selected, true); await tick();
  pending.shift()!(native); await tick();
  expect(active.textContent).toContain("fixture-current-holder");
  expect(active.textContent).not.toContain("past-holder");
  expect(active.textContent).toContain(locale === "en" ? "Team stage: Submitted" : "Этап команды:");
  expect(active.textContent).toContain(locale === "en" ? "Phase acceptance: Team lead" : "Кто принимает фазу: Лид команды");
  expect(active.textContent).not.toContain(locale === "en" ? "Accept result" : "Принять результат");
  state.setTeamFailure(true); await tick();
  expect(active.textContent).not.toContain("fixture-current-holder");
  expect(active.textContent).not.toContain(locale === "en" ? "Team stage: Submitted" : "Этап команды:");
  expect(state.teamRead).toHaveBeenCalledTimes(3);
  state.setTeamFailure(false);
  pending.shift()!({ ...native, revision: 10, groups: [{ cards: [{
    ...native.groups[0].cards[0], stages: [{ state: "reported", holder: "fresh-holder" }],
  }] }] });
  await tick();
  expect(active.textContent).toContain("fresh-holder");
  expect(state.quickReview).not.toHaveBeenCalled();
});

it("a compact Team row waits for the known revision and hides a failed current read", async () => {
  const { details } = (await import("./fixtures/task-review-projected.json")).default;
  const id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", home = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  const selected = { ...details[2], quick_review_status: "team", team: { team: id, home } };
  const binding = { id, home, project: selected.project };
  const native = { bar: binding, revision: 9, groups: [{ cards: [{
    node: 6, kind: "task", task: { uuid: selected.uuid, project: selected.project },
    phase: 2, state: "reported", holder: "previous-holder", stages: [],
  }] }] };
  state.teamListed.mockReturnValue([{ ...binding, revision: 10 }]);
  state.teamLive.mockReturnValue(native);
  const outcome = () => reviewOutcome(selected.project, taskItem(selected));
  expect(outcome()).toEqual({ label: "Team review", actor: undefined });
  state.teamLive.mockReturnValue({ ...native, revision: 10 });
  expect(outcome()?.actor).toBe("previous-holder");
  state.setTeamFailure(true);
  expect(outcome()).toEqual({ label: "Team review", actor: undefined });
});
