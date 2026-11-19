// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { applyBatch, rowId } from "../../store/catalog";
import { setSetting, settingsReady } from "../../store/settings";
import { teams, type TeamCall, type TeamView } from "../../store/teams";
import { setDetailTab, setPage, setSelectedKey } from "../../store/ui";
import { emptyBatch, type ViewRow } from "../../types";
import listFixture from "../screens/fixtures/teams-screen.json";
import presentation from "../sessions/fixtures/team-session-presentation.json";
import { installUiLanguage, uiLanguage } from "../../i18n";
import boardFixture from "./fixtures/task-placement-view.json";
import { TeamTab } from "./TeamTab";

const transport = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: transport.invoke, isTauri: () => false }));

const view = boardFixture.before as unknown as TeamView;
const lead = view.members.find((member) => member.lead)!.session!;
const leadKey = "run:team-lead";
let dispose: (() => void) | undefined;

const publish = (current: TeamView) => teams.apply([{ computer: "", key: `team:${current.bar.id}`, kind: "team", payload: current }]);
const calls = () => transport.invoke.mock.calls.filter(([command]) => command === "team_action");
const button = (host: HTMLElement, kind: string) => host.querySelector<HTMLButtonElement>(`[data-task-action="${kind}"]`)!;
const choose = (select: HTMLSelectElement, value: string) => {
  select.value = value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
};

beforeEach(async () => {
  await teams.recheck();
  applyBatch({
    ...emptyBatch(),
    machines: [{ computer: "", machine: { machine_id: lead.machine } as never }],
    computers: [{ computer: "", status: "live", attempt: 0 }],
    upserts: [{ computer: "", key: leadKey, row: {
      ...(presentation.ready as unknown as ViewRow),
      launch_id: lead.session_id.slice("launch_id:".length),
    } }],
  });
  teams.apply([{ computer: "", key: "list", kind: "list", payload: listFixture.running }]);
  setPage("sessions");
  setDetailTab("team");
  setSelectedKey(rowId("", leadKey));
});

afterEach(async () => {
  dispose?.();
  dispose = undefined;
  await teams.closeTeam();
  for (const entry of teams.unresolved()) await teams.dismiss(entry);
  await setSetting("workspace/language", "en", { persist: false });
  setSelectedKey(undefined);
  transport.invoke.mockReset();
  document.body.replaceChildren();
});

async function open(node: number) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TeamTab, {}), host);
  await vi.waitFor(() => expect(teams.followed()).toBe(view.bar.id));
  publish(view);
  const task = `.team-task-card[data-node="${node}"]`;
  await vi.waitFor(() => expect(host.querySelector(task)).not.toBeNull());
  host.querySelector<HTMLButtonElement>(task)!.click();
  return host;
}

it("sends one revision-bound Reassign through the same Tauri command as Accept and Return", async () => {
  const host = await open(boardFixture.reported);
  expect(button(host, "reassign")).not.toBeNull();
  button(host, "reassign").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="reassign"]')!;
  const member = form.querySelector("select")!;
  expect([...member.options].map((option) => option.value)).toEqual(["", "m2"]);
  expect(calls()).toHaveLength(0);
  const submit = form.querySelector<HTMLButtonElement>('[type="submit"]')!;
  choose(member, "m2");
  expect(submit.disabled).toBe(true);
  const reason = form.querySelector<HTMLInputElement>("input")!;
  reason.value = "Continue the work on the same role";
  reason.dispatchEvent(new Event("input", { bubbles: true }));
  expect(submit.disabled).toBe(false);
  transport.invoke.mockImplementation(async (command: string, payload?: { call: TeamCall }) =>
    command === "team_action" ? { outcome: "failed", code: "stale_revision", detail: "The team changed before confirmation.", request_id: payload!.call.request_id } : undefined);
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  expect(calls()).toHaveLength(1);
  expect(calls()[0][1].call).toMatchObject({
    team: view.bar.id, expected_revision: view.revision,
    action: { kind: "reassign", node: boardFixture.reported, member: "m2", reason: reason.value },
  });
  expect(calls()[0][1].call.confirmed).toBeUndefined();
  await vi.waitFor(() => expect(host.querySelector("[data-team-action-failure-message]")?.textContent).toBe("The team changed before confirmation."));
  // zerus-uiy9p G10: the banner names the refused action.
  expect(host.querySelector("[data-team-action-failure-action]")?.textContent).toBe("Reassign");
});

it("shows the reason that a reported task cannot be routed", async () => {
  const host = await open(boardFixture.reported);
  expect(button(host, "route")).not.toBeNull();
  expect(button(host, "route").disabled).toBe(true);
  expect(host.querySelector("[data-team-detail]")?.textContent).toContain("Only waiting or pending tasks can be routed.");
  expect(calls()).toHaveLength(0);
});

it("uses only pipelines for the chosen pinned role and checks the original after an uncertain Route", async () => {
  const host = await open(boardFixture.pending);
  button(host, "route").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="route"]')!;
  const [role, pipeline] = form.querySelectorAll("select");
  expect([...role.options].map((option) => option.value)).toEqual(["", "developer", "reviewer"]);
  expect([...pipeline.options].map((option) => option.value)).toEqual(["", "reviewed"]);
  choose(role, "reviewer");
  expect([...pipeline.options].map((option) => option.value)).toEqual(["", "analysis"]);
  expect(form.textContent).toContain("Changing the role clears the current assignment.");
  expect(form.querySelector<HTMLButtonElement>('[type="submit"]')!.disabled).toBe(true);
  choose(pipeline, "analysis");
  transport.invoke.mockImplementation(async (command: string, payload?: { call: TeamCall }) => {
    if (command === "team_action") return { outcome: "uncertain", detail: "Check the original route.", request_id: payload!.call.request_id };
    if (command === "team_action_again") return { outcome: "done", receipt: {}, request_id: payload!.call.request_id };
  });
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  expect(calls()).toHaveLength(1);
  const call: TeamCall = calls()[0][1].call;
  expect(call).toMatchObject({
    team: view.bar.id, expected_revision: view.revision,
    action: { kind: "route", node: boardFixture.pending, role: "reviewer", pipeline: "analysis", keep_task_fields: true },
  });
  await vi.waitFor(() => expect(host.textContent).toContain("Check or dismiss the unresolved task action first."));
  expect(button(host, "route").disabled).toBe(true);
  host.querySelector<HTMLButtonElement>("[data-team-request] button")!.click();
  expect(transport.invoke).toHaveBeenCalledWith("team_action_again", { call });
  expect(calls()).toHaveLength(1);
});

// zerus-3u5yu (7), review of #2548 P2-1: an open form stays open across another
// team write (here the product's Hold of the same task) and keeps its choices,
// but it is bound to the revision the person reviewed. A newer revision marks
// it stale; nothing is sent until Review again binds the current revision.
// The first three tests are the reviewer's probes, adopted unchanged.
it("review2548 a Route draft from another revision is refused before transport", async () => {
  const host = await open(boardFixture.pending);
  button(host, "route").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="route"]')!;
  const after = boardFixture.after as unknown as TeamView;
  expect(after.revision).not.toBe(view.revision);
  publish(after);
  expect(host.querySelector('[data-placement-form="route"]')).toBe(form);
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  if (calls().length) {
    console.log(JSON.stringify({ opening: view.revision, displayed: after.revision, submitted: calls()[0][1].call.expected_revision }));
  }
  expect(calls()).toHaveLength(0);
});

it("shows a selected old-service Route refusal to the person", async () => {
  const host = await open(boardFixture.pending);
  button(host, "route").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="route"]')!;
  const detail = "Update the selected desktop service to route or reassign tasks from this window.";
  transport.invoke.mockImplementation(async (command: string, payload?: { call: TeamCall }) =>
    command === "team_action" ? { outcome: "failed", code: "person_gesture_required", detail, request_id: payload!.call.request_id } : undefined);
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(host.querySelector("[data-team-action-failure-message]")?.textContent).toBe(detail));
  expect(host.querySelector("[data-team-action-failure-action]")?.textContent).toBe("Route");
  expect(host.querySelector("[data-team-action-failure]")?.getAttribute("role")).toBe("alert");
  expect(calls()).toHaveLength(1);
  console.log(JSON.stringify({ alert: detail, sends: calls().length }));
});

it("refuses submitting a Reassign draft after the team revision changes", async () => {
  const host = await open(boardFixture.reported);
  button(host, "reassign").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="reassign"]')!;
  choose(form.querySelector("select")!, "m2");
  const reason = form.querySelector<HTMLInputElement>("input")!;
  reason.value = "Continue with the replacement";
  reason.dispatchEvent(new Event("input", { bubbles: true }));
  const after = boardFixture.after as unknown as TeamView;
  expect(after.revision).not.toBe(view.revision);
  publish(after);
  expect(host.querySelector('[data-placement-form="reassign"]')).toBe(form);
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  console.log(JSON.stringify({ opening: view.revision, displayed: after.revision, submitted: calls()[0]?.[1].call.expected_revision }));
  expect(calls()).toHaveLength(0);
});

it("sends the reviewed revision, and Review again binds a stale Route to the current one with its choices kept", async () => {
  const host = await open(boardFixture.pending);
  button(host, "route").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="route"]')!;
  const submit = form.querySelector<HTMLButtonElement>('[type="submit"]')!;
  const [role, pipeline] = [...form.querySelectorAll<HTMLSelectElement>("select:not([data-route-assignee])")].map((select) => select.value);
  expect(submit.disabled).toBe(false);
  expect(form.querySelector("[data-placement-review-again]")).toBeNull();
  const after = boardFixture.after as unknown as TeamView;
  publish(after);
  expect(host.querySelector('[data-placement-form="route"]')).toBe(form);
  expect(submit.disabled).toBe(true);
  expect(form.textContent).toContain("The team changed after you opened this form. Review the choices, then use Review again.");
  form.querySelector<HTMLButtonElement>("[data-placement-review-again]")!.click();
  expect(form.querySelector("[data-placement-review-again]")).toBeNull();
  expect(submit.disabled).toBe(false);
  expect([...form.querySelectorAll<HTMLSelectElement>("select:not([data-route-assignee])")].map((select) => select.value)).toEqual([role, pipeline]);
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(calls()).toHaveLength(1));
  expect(calls()[0][1].call).toMatchObject({ expected_revision: after.revision,
    action: { kind: "route", node: boardFixture.pending, role, pipeline, keep_task_fields: true } });
});

it("keeps showing the chosen member after another write, and Review again sends it with the current revision", async () => {
  const host = await open(boardFixture.reported);
  button(host, "reassign").click();
  const form = host.querySelector<HTMLFormElement>('[data-placement-form="reassign"]')!;
  choose(form.querySelector("select")!, "m2");
  const reason = form.querySelector<HTMLInputElement>("input")!;
  reason.value = "Continue with the replacement";
  reason.dispatchEvent(new Event("input", { bubbles: true }));
  const after = boardFixture.after as unknown as TeamView;
  publish(after);
  expect(form.querySelector("select")!.value).toBe("m2");
  expect(form.querySelector<HTMLButtonElement>('[type="submit"]')!.disabled).toBe(true);
  form.querySelector<HTMLButtonElement>("[data-placement-review-again]")!.click();
  expect(form.querySelector("select")!.value).toBe("m2");
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(calls()).toHaveLength(1));
  expect(calls()[0][1].call).toMatchObject({ expected_revision: after.revision,
    action: { kind: "reassign", node: boardFixture.reported, member: "m2", reason: "Continue with the replacement" } });
});

it("renders the disabled action and its reason in Russian", async () => {
  await settingsReady;
  installUiLanguage();
  await setSetting("workspace/language", "ru", { persist: false });
  await vi.waitFor(() => expect(uiLanguage()).toBe("ru"));
  const host = await open(boardFixture.reported);
  expect(button(host, "reassign").textContent).toBe("Переназначить…");
  expect(button(host, "route").textContent).toBe("Распределить…");
  expect(button(host, "route").disabled).toBe(true);
  expect(host.textContent).toContain("Маршрут можно выбрать только для задачи, которая ещё ожидает начала работы.");
});
