// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { OrganizationDialogs } from "./OrganizationDialogs";
import { openSessionGroups, setSessionGroupPrompt } from "./session-group-dialogs";
import { applyGrouping } from "../../store/organization";
import { applyBatch, catalog } from "../../store/catalog";
import { emptyBatch, type GroupingOut } from "../../types";

const api = vi.hoisted(() => ({ editSessionGroup: vi.fn(async () => ({ outcome: "done", conflicts: 0 })) }));
vi.mock("../../bridge", async (original) => ({ ...await original<typeof import("../../bridge")>(), ...api }));
let dispose: (() => void) | undefined;
const settle = async () => { for (let i = 0; i < 15; i++) await Promise.resolve(); };
afterEach(() => { dispose?.(); setSessionGroupPrompt(undefined); document.body.replaceChildren(); vi.clearAllMocks(); });
function mount() {
  const grouping: GroupingOut = { loaded: true, default_project: "alpha", headers: [
    { id: "alpha", name: "Sample", color: null, palette: 0, accessible: true, is_default: true, collapsed: false,
      session_groups: [{ id: "ui", name: "UI", collapsed: false }] },
  ], ready: true, reason: null, busy: false, status: "", status_error: false, conflicts: 0 };
  applyBatch({ ...emptyBatch(), replaces: [{ computer: "", rows: [{ computer: "", key: "run:groups", row: {
    name: "codex/sample/group-test", machine: "workstation", status: "ready", state: "running", run_id: "groups", conversation_id: "conversation-groups",
  } }] }] });
  applyGrouping({ ...emptyBatch(), grouping, groups: [{ computer: "", key: "run:groups", group: "alpha", rank: 1 }] });
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(OrganizationDialogs, {}), host);
  return host;
}
it("creates and renames groups through the shared writer, and confirms deletion", async () => {
  const host = mount(); openSessionGroups("alpha"); await settle();
  const input = () => host.querySelector<HTMLInputElement>("#session-group-name")!;
  input().value = "Inventory"; input().dispatchEvent(new InputEvent("input", { bubbles: true }));
  input().form!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await settle();
  expect(api.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "create", id: "", name: "Inventory" });
  host.querySelector<HTMLButtonElement>('[aria-label="Rename group UI"]')!.click();
  input().value = "Interface"; input().dispatchEvent(new InputEvent("input", { bubbles: true }));
  input().form!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await settle();
  expect(api.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "rename", id: "ui", name: "Interface" });
  const calls = api.editSessionGroup.mock.calls.length;
  host.querySelector<HTMLButtonElement>('[aria-label="Delete group UI"]')!.click(); await settle();
  expect(api.editSessionGroup).toHaveBeenCalledTimes(calls);
  const confirm = host.querySelector<HTMLElement>('[role="alert"]')!;
  expect(confirm.textContent).toContain("Its sessions will stay in the project");
  confirm.querySelector<HTMLButtonElement>("button")!.click(); await settle();
  expect(api.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "delete", id: "ui" });
});
it("assigns an exact current session and can remove its group", async () => {
  const host = mount();
  openSessionGroups("alpha", [catalog.rows["\nrun:groups"]]); await settle();
  const choose = () => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "UI")!;
  choose().click(); await settle();
  expect(api.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "assign", group: "ui", members: ["workstation\ncodex/sample/group-test"] });
  openSessionGroups("alpha", [catalog.rows["\nrun:groups"]]); await settle();
  [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "No group")!.click(); await settle();
  expect(api.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "assign", group: null, members: ["workstation\ncodex/sample/group-test"] });
});
it("refuses a selection whose conversation was replaced while the dialog was open", async () => {
  const host = mount(); openSessionGroups("alpha", [catalog.rows["\nrun:groups"]]); await settle();
  applyBatch({ ...emptyBatch(), upserts: [{ computer: "", key: "run:groups", row: { name: "codex/sample/group-test", machine: "workstation", status: "ready", conversation_id: "replaced" } }] });
  [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "UI")!.click(); await settle();
  expect(api.editSessionGroup).not.toHaveBeenCalled();
});
