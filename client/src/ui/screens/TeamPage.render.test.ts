// @vitest-environment happy-dom
// zerus-63rru.198 (wave 3b): a team page that works without the lead's
// session. The list rows and the team view are the Rust product writer's
// (teams_screen.rs: screens/fixtures/teams-screen.json); only the desktop
// invoke is replaced, and it records what the window sends.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Show, createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "./fixtures/teams-screen.json";
import type { TeamList, TeamSummary, TeamView } from "../../store/teams";
import type { Session } from "../sessions/types";
import type { TeamsOut } from "../../types";

const data = fixture as unknown as { running: TeamList; team: TeamView };
const f = vi.hoisted(() => ({ rows: [] as Session[], follows: [] as { team?: string | null }[], actions: [] as unknown[] }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string, args?: Record<string, unknown>) => {
  if (command === "teams_follow") f.follows.push(args as { team?: string | null });
  if (command === "team_action") { f.actions.push(args!.call); return { outcome: "done", request_id: (args!.call as { request_id: string }).request_id, receipt: {} }; }
  return undefined;
}, isTauri: () => false }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => f.rows }));

const view = data.team;
// The fixture's team, listed with no lead session: today's dead end.
const leadless: TeamSummary = { ...data.running.teams[0], id: view.bar.id, name: view.bar.name, status: view.bar.status, revision: view.revision, lead_session: null };
let teams: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
let navigation: typeof import("../teams/team-navigation");
// New modules load by a computed path, so the RED run on unmodified code
// fails on its assertions instead of at transform time.
const load = <T,>(path: string) => import(/* @vite-ignore */ path) as Promise<T>;
const disposers: (() => void)[] = [];
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
async function mount<T>(load: () => Promise<(props: T) => unknown>, props: T) {
  const component = await load();
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => createComponent(component as never, props as never), host)); return host;
}
const button = (host: HTMLElement, text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);
beforeEach(async () => {
  vi.resetModules(); f.rows = []; f.follows = []; f.actions = [];
  ({ teams } = await import("../../store/teams")); ui = await import("../../store/ui");
  navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation").catch(() => undefined as never);
  ui.setPage("teams"); ui.setSelectedKey(undefined);
  teams.apply([entry("list", "list", { ...data.running, teams: [leadless] })]);
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("opens the team page from a Teams row whose lead has no session", async () => {
  const host = await mount(async () => (await import("./Teams")).TeamsScreen, {});
  host.querySelector<HTMLButtonElement>(`[data-team-row="${view.bar.id}"]`)!.click();
  await flush();
  expect(ui.page()).toBe("team");
  expect(host.textContent).not.toContain("The team's lead has no session yet.");
  expect(navigation?.teamPage()?.team).toBe(view.bar.id);
});

it("follows the team by its ID, shows its workspace, and sends a Pause with no session", async () => {
  navigation.openTeamPage(view.bar.id);
  // As App's page Switch does: the page is mounted while it is the page.
  const { TeamPage } = await load<typeof import("../teams/TeamPage")>("../teams/TeamPage");
  const host = await mount(async () => () => createComponent(Show, { keyed: true, get when() { return ui.page() === "team"; }, get children() { return createComponent(TeamPage, {}); } }), {});
  await flush();
  expect(f.follows.at(-1)?.team).toBe(view.bar.id);
  teams.apply([entry(`team:${view.bar.id}`, "team", view)]);
  await flush();
  expect(host.querySelector("[data-team-tab]")).not.toBeNull();
  expect(host.querySelector("[data-team-page] h1")?.textContent).toBe(view.bar.name);
  button(host, "Pause")!.click(); await flush();
  expect(f.actions).toHaveLength(1);
  const sent = f.actions[0] as Record<string, unknown>;
  expect(sent).toMatchObject({ team: view.bar.id, team_name: view.bar.name, expected_revision: view.revision, action: { kind: "pause" } });
  expect(Object.keys(sent).sort()).toEqual(["action", "expected_revision", "request_id", "team", "team_name"]);
  // Back returns to where the page was opened and ends the follow.
  button(host, "Back")!.click(); await flush();
  expect(ui.page()).toBe("teams");
  expect(f.follows.at(-1)?.team ?? null).toBeNull();
});

it("keeps the lead session's tab as a shortcut that opens the same page", async () => {
  const shortcut = await mount(async () => (await load<typeof import("../teams/team-navigation")>("../teams/team-navigation")).OpenTeamPageButton, { team: view.bar.id });
  shortcut.querySelector<HTMLButtonElement>("[data-open-team-page]")!.click();
  expect(ui.page()).toBe("team");
  expect(navigation.teamPage()?.team).toBe(view.bar.id);
});
