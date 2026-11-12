// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

// zerus-5wmh.18.1: the selection bar's Move to project opens the
// organization's Move dialog with the selected rows, and a choice moves them
// all with one call (one swarm apply).
const bridge = vi.hoisted(() => ({ moveToProject: vi.fn(async () => ({ outcome: "done", conflicts: 0 })), editSessionGroup: vi.fn(async () => ({ outcome: "done", conflicts: 0 })) }));
vi.mock("../../bridge", async (original) => ({ ...await original<object>(), gitRows: vi.fn().mockResolvedValue(undefined),
  windowVisibility: { hidden: () => false, onChange: () => () => {} }, ...bridge,
}));
vi.mock("@tanstack/solid-virtual", () => ({
  createVirtualizer: (options: { count: number; getItemKey(n: number): string }) => ({
    measure() {}, measureElement() {}, _willUpdate() {}, scrollToIndex() {}, getTotalSize: () => options.count * 94,
    getVirtualItems: () => Array.from({ length: options.count }, (_, index) => ({ key: options.getItemKey(index), start: index * 94, index })),
  }),
}));
import { SessionList } from "./SessionList";
import { OrganizationDialogs } from "./OrganizationDialogs";
import { applyBatch } from "../../store/catalog";
import { applyGrouping } from "../../store/organization";
import { emptyBatch, type GroupingOut } from "../../types";
import { setMulti, setPage, setStripMode } from "../../store/ui";

let dispose: (() => void) | undefined;
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
afterEach(async () => { dispose?.(); dispose = undefined; await settle(); document.body.replaceChildren(); setMulti({}); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const header = (id: string, extra = {}) => ({ id, name: id, color: null, accessible: true, collapsed: false, is_default: false, palette: 2, ...extra });
const grouping = (ready: boolean): GroupingOut => ({ loaded: true, default_project: "ungrouped", headers: [header("alpha"), header("ungrouped", { is_default: true })],
  ready, reason: ready ? null : "Project sync is starting.", busy: false, status: "", status_error: false, conflicts: 0 });
const rows = () => ({ ...emptyBatch(), replaces: [{ computer: "", rows: [
  { computer: "", key: "run:move-0", row: { name: "codex/move/0", agent: "codex", status: "ready", state: "running", cwd: "/move/0", run_id: "move-0", conversation_id: "c0" } },
  { computer: "", key: "run:move-1", row: { name: "claude/move/1", agent: "claude", status: "ready", state: "running", cwd: "/move/1", run_id: "move-1", conversation_id: "c1" } },
] }] });

function mount(ready: boolean) {
  vi.stubGlobal("ResizeObserver", class { constructor(private run: () => void) {} observe() { this.run(); } disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1200);
  setPage("sessions"); setStripMode(false);
  applyBatch(rows());
  applyGrouping({ ...emptyBatch(), grouping: grouping(ready) });
  setMulti({ "\nrun:move-0": true, "\nrun:move-1": true });
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => [createComponent(SessionList, {}), createComponent(OrganizationDialogs, {})], host);
  return { host, move: () => [...host.querySelectorAll<HTMLButtonElement>(".selection-bar button")].find((b) => b.dataset.text?.startsWith("Move to project")) };
}

it("moves the selected sessions with one call from the bar's Move to project", async () => {
  const t = mount(true);
  await settle();
  const move = t.move()!;
  expect(move).toBeTruthy();
  expect([move.disabled, move.getAttribute("aria-label")]).toEqual([false, "Move to project — 2 of 2 selected sessions"]);
  move.click(); await settle();
  expect(document.querySelector("#org-dlg-title")?.textContent).toBe("Move to project");
  [...document.querySelectorAll<HTMLButtonElement>(".dlg-body .menu-item")].find((b) => b.textContent === "alpha")!.click();
  await settle();
  expect(bridge.moveToProject).toHaveBeenCalledTimes(1);
  const [members, project] = bridge.moveToProject.mock.calls[0] as unknown as [string[], string];
  expect(project).toBe("alpha");
  expect(members.map((m) => m.split("\n")[1]).sort()).toEqual(["claude/move/1", "codex/move/0"]);
});

it("keeps Move to project disabled with the sync reason until project editing is ready", async () => {
  const t = mount(false);
  await settle();
  const move = t.move()!;
  expect([move.disabled, move.getAttribute("aria-label")]).toEqual([true, "Project sync is starting."]);
});

it("drags group header buttons while keeping No group fixed below them", async () => {
  const t = mount(true); setMulti({}); bridge.editSessionGroup.mockClear();
  const g = grouping(true);
  g.headers[0].session_groups = [{ id: "ui", name: "UI", collapsed: false }, { id: "inventory", name: "Inventory", collapsed: false }];
  applyGrouping({ ...emptyBatch(), grouping: g, groups: [
    { computer: "", key: "run:move-0", group: "alpha", subgroup: "ui", rank: 1 },
    { computer: "", key: "run:move-1", group: "alpha", subgroup: "inventory", rank: 2 },
  ] });
  await settle();
  const bands = [...t.host.querySelectorAll<HTMLButtonElement>(".session-group-band")];
  expect(bands.map((b) => b.querySelector(".gb-name")?.textContent)).toEqual(["UI", "Inventory", "No group"]);
  expect(bands.map((b) => b.getAttribute("draggable"))).toEqual(["true", "true", null]);
  const start = new Event("dragstart", { bubbles: true, cancelable: true });
  Object.defineProperty(start, "dataTransfer", { value: { setData: vi.fn(), effectAllowed: "" } });
  bands[1].dispatchEvent(start);
  bands[0].dispatchEvent(new MouseEvent("dragover", { bubbles: true, cancelable: true, clientY: 42 }));
  bands[0].dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await settle();
  expect(bridge.editSessionGroup).toHaveBeenLastCalledWith("alpha", { kind: "reorder", id: "inventory", before: "ui" });
});
