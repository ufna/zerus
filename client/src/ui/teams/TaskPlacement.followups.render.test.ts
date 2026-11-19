// @vitest-environment happy-dom
// zerus-3u5yu, follow-ups of the #2527 review: the window's Reassign and
// Route choices. The team is the command line's own placement view
// (fixtures/task-placement-view.json); stage, approved-phase and pending
// cards, and a fourth developer, are hand-written variants of its rows.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import boardFixture from "./fixtures/task-placement-view.json";
import { TaskPlacement } from "./TaskPlacement";
import { setSetting, settingsReady } from "../../store/settings";
import { installUiLanguage, uiLanguage } from "../../i18n";
import type { TaskView, TeamView } from "./types";

const view = boardFixture.before as unknown as TeamView;
const cards = view.groups.flatMap((group) => group.cards);
const pending = cards.find((card) => card.state === "pending")!;
const reported = cards.find((card) => card.state === "reported")!;
const m4 = { ...view.members.find((member) => member.id === "m2")!, id: "m4" };
let dispose: (() => void) | undefined;
afterEach(async () => { dispose?.(); dispose = undefined; document.body.replaceChildren(); await setSetting("workspace/language", "en", { persist: false }); });

function mount(task: TaskView, current: TeamView = view) {
  const host = document.createElement("div"); document.body.append(host);
  const actions = vi.fn();
  dispose = render(() => createComponent(TaskPlacement, { view: current, task, busy: false, unresolved: false, otherFormOpen: false, onOpen: () => {}, onAction: actions }), host);
  return { host, actions };
}
const button = (host: HTMLElement, kind: string) => host.querySelector<HTMLButtonElement>(`[data-task-action="${kind}"]`)!;
const offered = (host: HTMLElement) => [...host.querySelectorAll<HTMLOptionElement>('[data-placement-form="reassign"] option')].map((option) => option.value).filter(Boolean);

it("handwritten_a_stage_cannot_be_routed_and_says_why", () => {
  const { host } = mount({ ...reported, kind: "stage", state: "pending" });
  expect(button(host, "route").disabled).toBe(true);
  expect(button(host, "route").title).toBe("A stage's role comes from its pipeline. Reassign the stage to change its member.");
});

it("handwritten_an_approved_phase_task_cannot_be_routed_and_says_why", () => {
  const { host } = mount({ ...pending, phase: 3 });
  expect(button(host, "route").disabled).toBe(true);
  expect(button(host, "route").title).toBe("Changing an approved task's route requires a plan edit.");
});

it("handwritten_a_replaceable_pending_decision_still_blocks_route_but_not_reassign", () => {
  const { host } = mount({ ...pending, pending_decision: "return", pending_replaceable: true });
  expect(button(host, "route").disabled).toBe(true);
  expect(button(host, "route").title).toBe("The task's previous decision has not settled yet.");
  expect(button(host, "reassign").disabled).toBe(false);
  expect(host.textContent).toContain("Reassign replaces the pending decision.");
});

it("handwritten_a_stage_reassign_leaves_out_the_implementer_and_the_targets_earlier_implementers", () => {
  const team = { ...view, members: [...view.members, m4] };
  const stage = { ...reported, kind: "stage", role: "developer", state: "pending", assignee: "", holder: null, implementer: "m1", earlier_implementers: ["m2"] } as TaskView;
  const { host } = mount(stage, team);
  button(host, "reassign").click();
  expect(offered(host)).toEqual(["m4"]);
});

it("names roles, pipelines and members in the catalog's Russian terms", async () => {
  await settingsReady;
  installUiLanguage();
  await setSetting("workspace/language", "ru", { persist: false });
  await vi.waitFor(() => expect(uiLanguage()).toBe("ru"));
  const { host } = mount(pending);
  button(host, "route").click();
  const form = host.querySelector('[data-placement-form="route"]')!;
  const roles = [...form.querySelectorAll<HTMLSelectElement>("select")[0].options].map((option) => option.textContent);
  expect(roles).toEqual(["Выберите роль", "Разработчик", "Ревьюер"]);
  expect(form.querySelectorAll<HTMLSelectElement>("select")[1].options[0].textContent).toBe("Выберите конвейер");
  expect(form.textContent).not.toMatch(/процесс/i);
  dispose?.(); document.body.replaceChildren();
  const reassign = mount(pending, { ...view, members: [...view.members, m4] });
  button(reassign.host, "reassign").click();
  expect(reassign.host.querySelector('[data-placement-form="reassign"]')?.textContent).toContain("Новый участник");
});
