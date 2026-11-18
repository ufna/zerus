// @vitest-environment happy-dom
// A member opens its own session from the real Team workspace (zerus-rzvct G2),
// through the real catalog, machine, UI and session stores. Ported from the
// independent review of #2525 (review2525 member probe): the exact machine and
// launch or run binding, and the review's two findings, a selected native child
// that must not survive the navigation and a lead whose ID equals its role.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { applyBatch, catalog, rowId } from "../../store/catalog";
import { activity, subagentActivity } from "../../store/session";
import * as ui from "../../store/ui";
import { MenuLayer } from "../components/Overlays";
import { activitySourceFor } from "../sessions/SessionDetail";
import type { Batch, RowOut, ViewRow } from "../../types";
import type { MemberView, TeamView } from "./types";
import { TeamWorkspace } from "./TeamTab";
import fixture from "./fixtures/lanes-view.json";

const scan = vi.hoisted(() => ({ lengths: [] as number[] }));
vi.mock("../../store/catalog", async (original) => {
  const real = await original<typeof import("../../store/catalog")>();
  return { ...real, orderedRows: () => {
    const rows = real.orderedRows();
    if (new Error().stack?.includes("memberSessions")) scan.lengths.push(rows.length);
    return rows;
  } };
});
const view = (fixture.pages as unknown as TeamView[])[0];
const first = view.members.find((member) => member.id === "m1")!;
const lead = view.members.find((member) => member.id === "lead")!;
const otherMachine = "00000000-0000-4000-8000-0000000000bb";
const newLaunch = "00000000-0000-4000-8000-000000000099";
const absentLaunch = "00000000-0000-4000-8000-000000000098";
const launch = first.session!.session_id.split(":")[1];
const leadLaunch = lead.session!.session_id.split(":")[1];
function listed(computer: string, run: string, launchId: string, member = "m1", state = "running"): RowOut {
  return { computer, key: "run:" + run,
    row: { name: "claude/example/" + member, agent: "claude", status: "ready", state, run_id: run,
      launch_id: launchId, conversation_id: "conversation-" + run,
      archive_id: state === "archived" ? "example-archive" : undefined } as ViewRow };
}
function batch(changes: Partial<Batch>) {
  applyBatch({ upserts: [], removes: [], replaces: [], computers: [], activity: [], ...changes });
}
function replace(rows: RowOut[]) {
  batch({
    replaces: ["", "peer", "unknown-peer"].map((computer) => ({ computer, rows: rows.filter((row) => row.computer === computer) })),
    computers: ["", "peer"].map((computer) => ({ computer, status: "live", attempt: 0 })),
    machines: [
      { computer: "", machine: { machine_id: first.session!.machine } },
      { computer: "peer", machine: { machine_id: otherMachine } },
    ] as Batch["machines"],
  });
}
let dispose: (() => void) | undefined;
function mount(initial = view) {
  const [current, setView] = createSignal(initial);
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => [
    createComponent(TeamWorkspace, { get view() { return current(); }, busy: false, failure: null,
      onAction: vi.fn(), onPage: vi.fn(), onAddTask: vi.fn() }),
    createComponent(MenuLayer, {}),
  ], host);
  return { host, setView };
}
const card = (host: HTMLElement, member = "m1") => host.querySelector<HTMLElement>('[data-member="' + member + '"]')!;
const nameButton = (host: HTMLElement, member?: string) => card(host, member).querySelector<HTMLButtonElement>("button.team-member-name");
function menu(host: HTMLElement, member?: string) {
  card(host, member).querySelector<HTMLButtonElement>(".team-member-more")!.click();
  return [...host.querySelectorAll<HTMLButtonElement>(".menu-item")].find((el) => el.textContent === "Open session")!;
}
const changed = (member: MemberView, session: NonNullable<MemberView["session"]>) => ({ ...member, session });
const source = () => { const actual = activitySourceFor(ui.selected()!.key); return actual === subagentActivity ? "child activity" : actual === activity ? "main activity" : "other source"; };
beforeEach(() => {
  replace([]);
  ui.setPage("sessions"); ui.setDetailTab("team"); ui.setSelectedKey(undefined); ui.setSubagentOpen(undefined);
  scan.lengths.length = 0;
});
afterEach(() => { dispose?.(); dispose = undefined; ui.setMenu(undefined); document.body.replaceChildren(); replace([]); ui.setSubagentOpen(undefined); });

it("a lead whose native child was selected opens the member's main Activity, not that child's", () => {
  const parent = listed("", "lead-main", leadLaunch, "lead");
  const target = listed("", "old-member", launch);
  replace([parent, target]);
  ui.setSelectedKey(rowId("", parent.key));
  ui.setSubagentOpen("lead-child");
  ui.setDetailTab("team");
  for (const open of [(host: HTMLElement) => menu(host).click(), (host: HTMLElement) => nameButton(host)!.click()]) {
    ui.setSelectedKey(rowId("", parent.key)); ui.setSubagentOpen("lead-child"); ui.setDetailTab("team");
    const { host } = mount();
    open(host);
    expect(ui.selected()?.key).toBe(rowId("", target.key));
    expect(ui.detailTab()).toBe("activity");
    expect(ui.subagentOpen()).toBeUndefined();
    expect(source()).toBe("main activity");
    dispose?.(); dispose = undefined; ui.setMenu(undefined); document.body.replaceChildren();
  }
});

it("a member whose ID equals its role opens its session from its displayed label", () => {
  const parent = listed("", "lead-main", leadLaunch, "lead");
  replace([parent]);
  const { host } = mount();
  expect(menu(host, "lead").getAttribute("aria-disabled")).not.toBe("true");
  ui.setMenu(undefined);
  const label = nameButton(host, "lead");
  expect(label?.textContent).toBe("Lead");
  // The chip inside the link says what the link does, not its role ID.
  expect(label!.querySelector(".role-badge")?.getAttribute("title")).toBe("Open this member's session");
  label!.click();
  expect(ui.selected()?.key).toBe(rowId("", parent.key));
  expect(ui.detailTab()).toBe("activity");
});

it("selects the exact launch among same-ID and same-launch wrong-machine decoys", () => {
  const right = listed("", "own", launch);
  replace([listed("peer", "foreign", launch), listed("", "other-team", absentLaunch), right]);
  const { host } = mount();
  nameButton(host)!.click();
  expect(ui.selected()?.key).toBe(rowId("", right.key));
});

it("selects a matching remote member on its own machine and matches a run binding exactly", () => {
  const remote = listed("peer", "remote", launch);
  replace([listed("", "local", launch), remote]);
  let mounted = mount({ ...view, members: [changed(first, { ...first.session!, machine: otherMachine })] });
  menu(mounted.host).click();
  expect(ui.selected()?.key).toBe(rowId("peer", remote.key));
  dispose?.(); ui.setMenu(undefined); document.body.replaceChildren();
  const right = listed("", "expected-run", newLaunch);
  replace([listed("", "previous", launch), right]);
  mounted = mount({ ...view, members: [changed(first, { ...first.session!, session_id: "run_id:expected-run" })] });
  nameButton(mounted.host)!.click();
  expect(ui.selected()?.key).toBe(rowId("", right.key));
});

it("refuses a removed run and its same-name replacement, even from an already open menu", () => {
  const old = listed("", "original", launch);
  replace([old]);
  const { host } = mount();
  const open = menu(host);
  expect(open.getAttribute("aria-disabled")).not.toBe("true");
  batch({ removes: [{ computer: "", key: old.key, reason: "archived" }], upserts: [listed("", "replacement", newLaunch), listed("peer", "remote-old", launch)] });
  open.click();
  expect(ui.selected()).toBeUndefined();
  expect(nameButton(host)).toBeNull();
  expect(menu(host).title).toBe("This member's session is not on the session list.");
});

it("scans the live catalog once for the rendered team and ignores a status-only delta", () => {
  const rows = Array.from({ length: 5_000 }, (_, i) => listed("", "example-" + i, "launch-" + i, "example-" + i));
  rows.push(listed("", "exact", launch));
  replace(rows);
  scan.lengths.length = 0;
  const { host } = mount();
  expect(host.querySelectorAll("[data-member]")).toHaveLength(view.members.length);
  expect(scan.lengths).toEqual([5_001]);
  batch({ upserts: [{ ...rows[0], row: { ...rows[0].row, status: "working" } }] });
  expect(scan.lengths).toEqual([5_001]);
  expect(catalog.rows[rowId("", "run:example-0")].status).toBe("working");
}, 25_000);
