// @vitest-environment happy-dom
// Review of #2531 (codex/zerus-review2531/review2531-cx), adopted: the
// evidence in the real inspector (TeamWorkspace) in Russian above Accept and
// Return. Evidence of another revision than the decision shown is marked as
// updating, and Accept and Return wait for the matching evidence.
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { TeamWorkspace } from "./TeamTab";
import { workspaceLayoutView } from "./fixtures/layout";
import fixture from "../../../core/src/teams/fixtures/evidence.json";
import type { TaskEvidenceView } from "../../store/teamEvidence";

const port = vi.hoisted(() => vi.fn());
vi.mock("../../store/teamEvidence", () => ({ readTaskEvidence: port }));
const product = fixture as unknown as TaskEvidenceView;
const settle = async () => { for (let n = 0; n < 10; n++) await Promise.resolve(); };
let dispose: (() => void) | undefined;
beforeAll(async () => {
  const settings = await import("../../store/settings");
  await settings.settingsReady;
  await settings.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n");
  language.installUiLanguage();
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  port.mockReset();
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

function setup() {
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  const host = document.createElement("div");
  document.body.append(host);
  const [view, setView] = createSignal({ ...workspaceLayoutView(), revision: product.revision });
  const actions = vi.fn();
  dispose = render(() => createComponent(TeamWorkspace, {
    get view() { return view(); }, busy: false, failure: null,
    onAction: actions, onPage() {}, onAddTask() {},
  }), host);
  host.querySelector<HTMLButtonElement>('.team-task-card[data-node="1"]')!.click();
  return { host, view, setView, actions };
}

it("shows translated evidence in the real inspector above unchanged Accept and Return", async () => {
  port.mockResolvedValue(product);
  const { host, actions } = setup();
  await settle();
  const section = host.querySelector("[data-task-evidence]")!;
  expect(section.textContent).toContain("Отчёт и вердикты");
  expect(section.textContent).toContain("Отчёт: m1");
  expect(section.textContent).toContain("Ревью, раунд 1");
  expect(section.textContent).toContain("Одобрено: m3");
  expect(section.textContent).not.toMatch(/[\u00b7\u2022]|руководител|координатор/i);
  const accept = host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!;
  const returned = host.querySelector<HTMLButtonElement>('[data-task-action="return"]')!;
  expect(accept.textContent).toBe("Принять");
  expect(returned.textContent).toBe("Вернуть");
  expect(section.compareDocumentPosition(accept) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  accept.click();
  expect(actions).toHaveBeenCalledWith("accept", { node: 1 });
  returned.click();
  expect(host.querySelector(".team-reason input")).not.toBeNull();
  expect(actions).toHaveBeenCalledTimes(1);
});

function pendingRefresh() {
  let release!: (value: TaskEvidenceView) => void;
  port.mockResolvedValueOnce(product);
  port.mockImplementationOnce(() => new Promise<TaskEvidenceView>((resolve) => { release = resolve; }));
  const instance = setup();
  const refresh = () => instance.setView({ ...instance.view(), revision: instance.view().revision + 1 });
  return { ...instance, refresh, finish() { release({
    ...product,
    revision: instance.view().revision,
    report: { ...product.report!, generation: 2, text: "New report" },
    stages: [{ ...product.stages[0], round: 2, verdict: { ...product.stages[0].verdict!, text: "New findings" } }],
  }); } };
}

it("refreshes the complete report and verdict when the followed workspace revision changes", async () => {
  const { host, refresh, finish } = pendingRefresh();
  await settle();
  refresh();
  await settle();
  expect(port).toHaveBeenCalledTimes(2);
  finish();
  await settle();
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe("New report");
  expect(host.querySelector("[data-evidence-findings]")?.textContent).toBe("New findings");
  expect(host.querySelector("[data-evidence-stage]")?.textContent).toBe("Ревью, раунд 2");
});

it("identifies retained evidence as updating while a newer decision revision is already actionable", async () => {
  const { host, actions, refresh, finish } = pendingRefresh();
  await settle();
  refresh();
  await settle();
  const accept = () => host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!;
  const returned = () => host.querySelector<HTMLButtonElement>('[data-task-action="return"]')!;
  expect(port).toHaveBeenCalledTimes(2);
  // The older evidence stays readable, marked as being read again.
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe(product.report!.text);
  expect(host.querySelector("[data-task-evidence]")?.hasAttribute("data-evidence-updating")).toBe(true);
  expect(host.querySelector("[data-task-evidence]")?.textContent).toContain("Чтение отчёта и вердиктов…");
  // No decision is offered against evidence of another revision.
  expect(accept().disabled).toBe(true);
  expect(returned().disabled).toBe(true);
  expect(host.querySelector("[data-action-why]")?.textContent).toBe("Отчёт и вердикты читаются заново.");
  accept().click();
  expect(actions).not.toHaveBeenCalled();
  finish();
  await settle();
  expect(host.querySelector("[data-task-evidence]")?.hasAttribute("data-evidence-updating")).toBe(false);
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe("New report");
  expect(accept().disabled).toBe(false);
  accept().click();
  expect(actions).toHaveBeenCalledWith("accept", { node: 1 });
});

it("a read that fails leaves the decisions offered, without older evidence beside them", async () => {
  port.mockResolvedValueOnce(product);
  port.mockRejectedValueOnce(new Error("Unknown team command: evidence"));
  const { host, view, setView } = setup();
  await settle();
  setView({ ...view(), revision: view().revision + 1 });
  await settle();
  expect(host.querySelector("[data-evidence-report]")).toBeNull();
  expect(host.querySelector("[data-evidence-error]")?.textContent).toContain("Unknown team command: evidence");
  expect(host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!.disabled).toBe(false);
});

it("distinguishes testing and landing and keeps service notes beside the report", async () => {
  port.mockResolvedValue({
    ...product,
    report: { ...product.report!, notes: [{ code: "test_only", text: "One private fixture note.", parameters: [] }] },
    stages: [
      { ...product.stages[0], kind: "test", node: 14 },
      { ...product.stages[0], kind: "land", node: 15, verdict: null },
    ],
  });
  const { host } = setup();
  await settle();
  expect([...host.querySelectorAll("[data-evidence-stage]")].map((element) => element.textContent)).toEqual(["Тест, раунд 1", "Интеграция, раунд 1"]);
  expect([...host.querySelectorAll("[data-evidence-verdict]")].map((element) => element.textContent)).toEqual(["Одобрено: m3", "Вердикта пока нет"]);
  expect(host.querySelector("[data-task-evidence]")?.textContent).toContain("One private fixture note.");
});

// Delta review of #2531 (codex/zerus-review2531/review2531-cx), adopted: an
// open Return form waits with the toolbar, a failed refresh removes the older
// evidence, and evidence newer than the board blocks both decisions.

it("keeps a previously opened Return draft inert until evidence matches the board", async () => {
  const { host, actions, refresh, finish } = pendingRefresh();
  await settle();
  const toolbarReturn = () => host.querySelector<HTMLButtonElement>('[data-task-action="return"]')!;
  const toolbarAccept = () => host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!;
  const send = () => host.querySelector<HTMLButtonElement>(".team-reason button.primary")!;
  toolbarReturn().click();
  const draft = host.querySelector<HTMLInputElement>(".team-reason input")!;
  draft.value = "Rewrite the first revision.";
  draft.dispatchEvent(new Event("input", { bubbles: true }));
  expect(send().disabled).toBe(false);
  refresh();
  await settle();
  expect(host.querySelector("[data-evidence-updating]")).not.toBeNull();
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe(product.report!.text);
  expect(toolbarReturn().disabled).toBe(true);
  expect(toolbarAccept().disabled).toBe(true);
  const before = send().disabled;
  send().click();
  expect(actions).not.toHaveBeenCalled();
  expect(before).toBe(true);
  expect(draft.value).toBe("Rewrite the first revision.");
  finish();
  await settle();
  expect(send().disabled).toBe(false);
  send().click();
  expect(actions).toHaveBeenCalledExactlyOnceWith("return", {
    node: 1, reason: "Rewrite the first revision.",
  });
});

it("removes the old report and verdict before a rejected refresh restores both decision buttons", async () => {
  port.mockResolvedValueOnce(product).mockRejectedValueOnce(new Error("fixture refresh failed"));
  const { host, actions, view, setView } = setup();
  await settle();
  const toolbar = () => [
    host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!,
    host.querySelector<HTMLButtonElement>('[data-task-action="return"]')!,
  ];
  expect(host.querySelector("[data-evidence-report]")).not.toBeNull();
  expect(host.querySelector("[data-evidence-findings]")).not.toBeNull();
  setView({ ...view(), revision: view().revision + 1 });
  await settle();
  expect(host.querySelector("[data-evidence-report]")).toBeNull();
  expect(host.querySelector("[data-evidence-findings]")).toBeNull();
  expect(host.querySelector("[data-evidence-updating]")).toBeNull();
  expect(host.querySelector("[data-evidence-error]")?.textContent).toContain("fixture refresh failed");
  expect(toolbar().map((button) => button.disabled)).toEqual([false, false]);
  toolbar()[0].click();
  expect(actions).toHaveBeenCalledExactlyOnceWith("accept", { node: 1 });
});

it("marks evidence newer than the board and blocks both decisions until the board catches up", async () => {
  const newer = {
    ...product, revision: product.revision + 1,
    report: { ...product.report!, text: "Evidence from the concurrent board revision" },
  };
  port.mockResolvedValue(newer);
  const { host, actions, view, setView } = setup();
  const toolbar = () => [
    host.querySelector<HTMLButtonElement>('[data-task-action="accept"]')!,
    host.querySelector<HTMLButtonElement>('[data-task-action="return"]')!,
  ];
  await settle();
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe(newer.report.text);
  expect(host.querySelector("[data-evidence-updating]")).not.toBeNull();
  expect(toolbar().map((button) => button.disabled)).toEqual([true, true]);
  toolbar()[0].click();
  expect(actions).not.toHaveBeenCalled();
  setView({ ...view(), revision: newer.revision });
  await settle();
  expect(host.querySelector("[data-evidence-updating]")).toBeNull();
  expect(toolbar().map((button) => button.disabled)).toEqual([false, false]);
});
