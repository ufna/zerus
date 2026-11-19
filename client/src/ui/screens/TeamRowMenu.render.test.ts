// @vitest-environment happy-dom
// zerus-63rru.198 slice 2: each Teams row's menu. The rows are the Rust
// product writer's (teams_screen.rs: screens/fixtures/teams-screen.json); the
// lead row is a synthetic core row mapped by the product's toSession; only
// the desktop invoke is replaced, and it records what the window sends.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "./fixtures/teams-screen.json";
import presentation from "../sessions/fixtures/team-session-presentation.json";
import type { TeamList, TeamSummary } from "../../store/teams";
import type { Session } from "../sessions/types";
import type { TeamsOut, ViewRow } from "../../types";

const data = fixture as unknown as { running: TeamList };
type Reply = Record<string, unknown> | ((call: Record<string, unknown>) => Record<string, unknown>);
const f = vi.hoisted(() => ({ rows: [] as Session[], actions: [] as Record<string, unknown>[], pages: [] as unknown[], reply: undefined as unknown }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string, args?: Record<string, unknown>) => {
  if (command === "teams_page") f.pages.push([args!.key, args!.cursor]);
  if (command === "team_action") {
    const call = args!.call as Record<string, unknown>; f.actions.push(call);
    const reply = f.reply as Reply | undefined;
    return typeof reply === "function" ? reply(call) : reply ?? { outcome: "done", receipt: {}, request_id: call.request_id };
  }
  return undefined;
}, isTauri: () => false }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => f.rows }));
vi.mock("../../store/ui", async (original) => {
  const ui = await original<typeof import("../../store/ui")>();
  return { ...ui, selected: () => f.rows.find((row) => row.key === ui.selectedKey()) };
});

const running = data.running.teams[0];
const as = (status: string, extra: Partial<TeamSummary> = {}): TeamSummary => ({ ...running, id: `00000000-0000-4000-8000-0000000${status.length}c0${status.length}`, name: `Team ${status}`, status, ...extra });
const load = <T,>(path: string) => import(/* @vite-ignore */ path) as Promise<T>;
let teams: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
let toSession: typeof import("../../store/catalog").toSession;
const disposers: (() => void)[] = [];
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
function leadRow(team: TeamSummary): Session {
  return toSession("", `lead:${team.lead_session!.session_id}`, { ...presentation.ready as unknown as ViewRow,
    name: `codex/unrelated/${team.id}`, title: "Lead display title", launch_id: team.lead_session!.session_id.slice("launch_id:".length),
    session_id: "native-id-is-not-the-launch", run_id: "unrelated-run" });
}
async function page(rows: TeamSummary[]) {
  teams.apply([entry("list", "list", { ...data.running, teams: rows })]);
  const { TeamsListPage } = await load<typeof import("../teams/TeamRowMenu")>("../teams/TeamRowMenu");
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => createComponent(TeamsListPage, {}), host)); await flush(); return host;
}
function items(host: HTMLElement, team: TeamSummary) {
  host.querySelector<HTMLButtonElement>(`[data-team-menu="${team.id}"]`)!.click();
  return (ui.menu()?.items ?? []).filter((item): item is Extract<typeof item, { label: string }> => "label" in item);
}
const labels = (host: HTMLElement, team: TeamSummary) => items(host, team).map((item) => item.label);
const pick = (host: HTMLElement, team: TeamSummary, label: string) => items(host, team).find((item) => item.label === label)!;
beforeEach(async () => {
  vi.resetModules(); f.rows = []; f.actions = []; f.pages = []; f.reply = undefined;
  ({ teams } = await import("../../store/teams")); ui = await import("../../store/ui");
  ({ toSession } = await import("../../store/catalog"));
  ui.setPage("teams"); ui.setSelectedKey(undefined); ui.setMenu(undefined);
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("offers the page, the lead session, Settings and the status's lifecycle actions", async () => {
  const stopped = as("stopped"), planning = as("planning"), paused = as("paused");
  f.rows = [leadRow(running)];
  const host = await page([running, stopped, planning, paused]);
  expect(labels(host, running)).toEqual(["Open", "Open lead session", "Settings…", "Pause", "Stop…"]);
  expect(labels(host, paused)).toEqual(["Open", "Open lead session", "Settings…", "Resume", "Stop…"]);
  // Pausing planning needs the team's planning state, which only the page reads.
  expect(labels(host, planning)).toEqual(["Open", "Open lead session", "Settings…", "Pause or resume planning…", "Stop…"]);
  pick(host, planning, "Pause or resume planning…").action!();
  expect(ui.page()).toBe("team"); expect(f.actions).toEqual([]); ui.setPage("teams");
  expect(labels(host, stopped)).toEqual(["Open", "Open lead session", "Settings…", "Archive…"]);
  expect(pick(host, stopped, "Settings…")).toMatchObject({ disabled: true, note: "Settings are available while the team runs or is paused." });
});

it("says why the lead session cannot be opened, and opens it when it is listed", async () => {
  const leadless = as("running", { lead_session: null });
  const unlisted = as("paused", { lead_session: { machine: running.lead_session!.machine, session_id: "launch_id:00000000-0000-4000-8000-0000000000c9" } });
  f.rows = [leadRow(running)];
  const host = await page([running, leadless, unlisted]);
  expect(pick(host, leadless, "Open lead session")).toMatchObject({ disabled: true, note: "The team's lead has no session yet." });
  expect(pick(host, unlisted, "Open lead session")).toMatchObject({ disabled: true, note: "The team's lead session is not on the session list." });
  pick(host, running, "Open lead session").action!();
  expect(ui.page()).toBe("sessions"); expect(ui.selectedKey()).toBe(f.rows[0].key); expect(ui.detailTab()).toBe("team");
});

it("opens the team page, or the page with its Settings, from the row", async () => {
  const host = await page([running]);
  pick(host, running, "Open").action!();
  expect(ui.page()).toBe("team");
  const navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation");
  expect(navigation.teamPage()).toEqual({ team: running.id, settings: undefined });
  ui.setPage("teams");
  pick(host, running, "Settings…").action!();
  expect(navigation.teamPage()).toEqual({ team: running.id, settings: true });
  expect(navigation.takeTeamSettings(running.id)).toBe(true);
  expect(navigation.takeTeamSettings(running.id)).toBe(false);
});

it("sends a lifecycle action from the row with the list's revision and no session", async () => {
  const host = await page([running]);
  pick(host, running, "Pause").action!(); await flush();
  expect(f.actions).toHaveLength(1);
  expect(f.actions[0]).toMatchObject({ team: running.id, team_name: running.name, expected_revision: running.revision, action: { kind: "pause" } });
  expect(Object.keys(f.actions[0]).sort()).toEqual(["action", "expected_revision", "request_id", "team", "team_name"]);
});

it("asks the native confirmation for Stop and sends the same call confirmed", async () => {
  f.reply = (call: Record<string, unknown>) => call.confirmed ? { outcome: "done", receipt: {}, request_id: call.request_id }
    : { outcome: "confirm", confirmation: { name: "teamStopConfirm", title: "Stop team?", text: "Stop Fixes? Every unfinished task is cancelled.", action: "Stop team" } };
  const host = await page([running]);
  pick(host, running, "Stop…").action!(); await flush();
  const dialog = document.querySelector<HTMLDialogElement>('dialog[data-name="teamStopConfirm"]')!;
  expect(dialog.textContent).toContain("Stop Fixes?");
  [...dialog.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Stop team")!.click(); await flush();
  expect(f.actions.map((call) => [call.action, call.confirmed ?? false])).toEqual([[{ kind: "stop" }, false], [{ kind: "stop" }, true]]);
  expect(host.querySelector("[data-team-row-notice]")).toBeNull();
});

it("refuses a stale revision under its row and reads the list again on Review again", async () => {
  f.reply = (call: Record<string, unknown>) => ({ outcome: "failed", code: "stale_revision", detail: "The team changed. Read it again.", request_id: call.request_id });
  const host = await page([running]);
  pick(host, running, "Pause").action!(); await flush();
  const notice = host.querySelector(`[data-team-row-notice="${running.id}"]`)!;
  expect(notice.textContent).toContain("The team changed. Read it again.");
  notice.querySelector<HTMLButtonElement>("[data-team-review-again]")!.click(); await flush();
  expect(f.pages.at(-1)).toEqual(["list", data.running.after ?? null]);
  expect(host.querySelector("[data-team-row-notice]")).toBeNull();
});
