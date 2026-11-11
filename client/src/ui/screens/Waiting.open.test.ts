// @vitest-environment happy-dom
// Waiting for you against the real store (findings 10, 11 and 13 of the second
// review): Open team and Open task find the lead's row by the machine and
// launch binding the Team tab uses, read the team's list page when the team is
// not on the pages read so far, and never by the row's native session id; a
// quiet supervised question is not a person's request; a later page has the
// way back and never says that nothing waits. Only the Tauri invoke is a fake.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { TeamsOut } from "../../types";
import { teamView } from "../teams/fixtures";

const MACHINE = "00000000-0000-4000-8000-0000000000aa";
const TEAM = "00000000-0000-4000-8000-00000000beef";
const f = vi.hoisted(() => ({
  rows: [] as Record<string, unknown>[], pages: [] as [string, string | null][], follows: [] as unknown[], acts: [] as unknown[],
  act: async (call: { request_id: string }): Promise<unknown> => ({ outcome: "done", receipt: {}, request_id: call.request_id }),
  select: vi.fn(), page: vi.fn(), detail: vi.fn(), open: vi.fn(),
}));
vi.mock("../../bridge", async (importOriginal) => ({ ...(await importOriginal<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: async (command: string, args: { key: string; cursor: string | null; call: { request_id: string } }) => {
    if (command === "team_action") { f.acts.push(args.call); return f.act(args.call); }
    if (command === "teams_page") f.pages.push([args.key, args.cursor]);
    if (command === "teams_follow") f.follows.push(args);
    return undefined;
  },
}));
vi.mock("../../store/machines", async (importOriginal) => ({ ...(await importOriginal<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
vi.mock("../../store/catalog", async (importOriginal) => ({ ...(await importOriginal<object>()), orderedRows: () => f.rows }));
vi.mock("../../store/ui", async (importOriginal) => ({ ...(await importOriginal<object>()), now: () => 0, setSelectedKey: f.select, setPage: f.page, setDetailTab: f.detail }));
vi.mock("../teams/openNode", () => ({ setOpenTeamNode: f.open, openTeamNode: () => null }));

let store: typeof import("../../store/teams").teams;
let WaitingScreen: typeof import("./Waiting").WaitingScreen;
let dispose: (() => void) | undefined;
beforeEach(async () => {
  vi.resetModules();
  ({ teams: store } = await import("../../store/teams"));
  ({ WaitingScreen } = await import("./Waiting"));
  f.rows = []; f.pages = []; f.follows = []; f.acts = [];
  f.act = async (call) => ({ outcome: "done", receipt: {}, request_id: call.request_id });
  f.select.mockClear(); f.page.mockClear(); f.detail.mockClear(); f.open.mockClear();
});
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const summary = (id: string, lead: unknown = null) => ({ id, project: "example-project", name: "Fixes", status: "running", lead: "lead", home: MACHINE, revision: 1, lead_session: lead });
const leadBinding = { machine: MACHINE, session_id: "launch_id:launch-a" };
const card = { id: `${TEAM}:1`, team: TEAM, card: 1, kind: "integration", node: 5, text: "approve", state: "open", by: "service", created_ms: 0 };
const leadRow = { key: "lead", name: "Lead", computer: "", machine: "workstation", launchId: "launch-a", sessionId: "native-session-a", runId: "run-a", attention: "none" };
function show() {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(WaitingScreen, {}), host);
  return host;
}
const button = (host: HTMLElement, label: string) => [...host.querySelectorAll("button")].find((candidate) => candidate.textContent === label) as HTMLButtonElement;

it("Open task selects the lead by machine and launch, not by its native session id (finding 10)", async () => {
  store.apply([entry("list", "list", { teams: [summary(TEAM, leadBinding)], truncated: false, next: null }), entry("waiting", "waiting", { items: [card], next: null })]);
  f.rows = [leadRow];
  const host = show();
  button(host, "Open task").click();
  await vi.waitFor(() => expect(f.select).toHaveBeenCalledWith("lead"));
  expect(f.open).toHaveBeenCalledWith({ team: TEAM, node: 5 });
  expect(f.detail).toHaveBeenCalledWith("team");
});

it("a row of another machine with the same launch id is not the lead", async () => {
  store.apply([entry("list", "list", { teams: [summary(TEAM, { ...leadBinding, machine: "00000000-0000-4000-8000-0000000000bb" })], truncated: false, next: null }), entry("waiting", "waiting", { items: [card], next: null })]);
  f.rows = [leadRow];
  const host = show();
  host.querySelector<HTMLButtonElement>(".waiting-team")!.click();
  await vi.waitFor(() => expect(host.textContent).toContain("The team's lead session is not on the session list."));
  expect(f.select).not.toHaveBeenCalled();
});

it("reads the next list page for a team not on the first, then opens it (finding 10)", async () => {
  store.apply([entry("list", "list", { teams: [summary("00000000-0000-4000-8000-000000000001")], truncated: false, next: "c1" }), entry("waiting", "waiting", { items: [card], next: null })]);
  f.rows = [leadRow];
  const host = show();
  host.querySelector<HTMLButtonElement>(".waiting-team")!.click();
  await vi.waitFor(() => expect(f.pages).toContainEqual(["list", "c1"]));
  expect(host.textContent).toContain("Reading the team list…");
  store.apply([entry("list", "list", { teams: [summary(TEAM, leadBinding)], truncated: false, next: null, after: "c1" })]);
  await vi.waitFor(() => expect(f.select).toHaveBeenCalledWith("lead"));
  expect(f.pages.at(-1), "the reader goes back to the first page").toEqual(["list", null]);
});

it("a quiet supervised member question is not a person's request (finding 11)", () => {
  store.apply([entry("list", "list", { teams: [summary(TEAM, leadBinding)], truncated: false, next: null }), entry("waiting", "waiting", { items: [], next: null })]);
  f.rows = [
    { key: "quiet-lead", name: "Lead", computer: "", launchId: "launch-a", quiet: true, status: "attention", attention: "question", actionNeeded: true },
    { key: "child", name: "Child", computer: "", launchId: "launch-child", parent: { machine: MACHINE, session_id: "launch_id:launch-a", state: "live" }, status: "attention", attention: "question", actionNeeded: true },
  ];
  const host = show();
  expect(host.querySelector("[data-waiting-request]")).toBeNull();
  f.rows = [{ ...f.rows[0], key: "asking-lead", quiet: false }];
  dispose?.(); document.body.replaceChildren();
  const again = show();
  expect(again.querySelector('[data-waiting-request="asking-lead"]')).not.toBeNull();
});

it("a later empty page offers the first page and does not say nothing waits (finding 13)", async () => {
  store.apply([entry("waiting", "waiting", { items: [card], next: "w1" })]);
  store.apply([entry("waiting", "waiting", { items: [], next: null, after: "w1" })]);
  const host = show();
  expect(host.textContent).not.toContain("Nothing waits for you.");
  expect(host.textContent).toContain("No open cards on this page.");
  expect(store.waitingCount()).toBe(2);
  f.pages = [];
  button(host, "First page").click();
  await vi.waitFor(() => expect(f.pages).toContainEqual(["waiting", null]));
});

it("leaving Waiting goes back to its first page", async () => {
  store.apply([entry("waiting", "waiting", { items: [], next: null, after: "w1" })]);
  show();
  f.pages = [];
  dispose?.(); dispose = undefined;
  await vi.waitFor(() => expect(f.pages).toContainEqual(["waiting", null]));
});

it("with Teams off it reads nothing and says so", async () => {
  store.apply([entry("list", "failed", { code: "teams_inactive", message: "off" })]);
  f.follows = [];
  const host = show();
  expect(host.querySelector("[data-waiting-off]")).not.toBeNull();
  await Promise.resolve();
  expect(f.follows).toEqual([]);
});

it("a refused decision shows the team's reason once, as this view's own", async () => {
  f.act = async (call) => ({ outcome: "failed", detail: "invalid_state: The card was already decided.", code: "invalid_state", request_id: call.request_id });
  store.apply([entry("waiting", "waiting", { items: [{ ...card, node: null }], next: null })]);
  const host = show();
  host.querySelector<HTMLButtonElement>("[data-decision]")!.click();
  await vi.waitFor(() => expect(f.follows.some((follow) => (follow as { team: string | null }).team === TEAM)).toBe(true));
  store.apply([entry(`team:${TEAM}`, "team", { ...teamView, bar: { ...teamView.bar, id: TEAM } })]);
  await vi.waitFor(() => expect(host.querySelector("[data-waiting-action-failure]")?.textContent).toContain("already decided"));
  expect(host.textContent!.split("The card was already decided.")).toHaveLength(2);
  expect(f.acts[0]).toMatchObject({ team: TEAM, expected_revision: 16, action: { kind: "decide", card: 1 } });
});

it("a member's person-only request shows from the team's list row, with no team loaded (second confirmation, finding 3)", async () => {
  const live = { ...summary(TEAM, leadBinding), member_sessions: [{ member: "m1", machine: MACHINE, session_id: "launch_id:launch-m1" }] };
  store.apply([entry("list", "list", { teams: [live], truncated: false, next: null }), entry("waiting", "waiting", { items: [], next: null })]);
  // The Team tab was opened and left, as navigating to Waiting does.
  await store.openTeam(TEAM);
  store.apply([entry(`team:${TEAM}`, "team", { ...teamView, bar: { ...teamView.bar, id: TEAM } })]);
  await store.closeTeam();
  expect(store.team()).toBeUndefined();
  f.rows = [{ key: "member", name: "Member", computer: "", launchId: "launch-m1", status: "attention", attention: "approval", actionNeeded: true }];
  const host = show();
  expect(host.querySelector('[data-waiting-request="member"]')).not.toBeNull();
  // The same row's status decides: a stopped team's member is not listed.
  store.apply([entry("list", "list", { teams: [{ ...live, status: "stopped" }], truncated: false, next: null })]);
  await vi.waitFor(() => expect(host.querySelector('[data-waiting-request="member"]')).toBeNull());
});
