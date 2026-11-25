// @vitest-environment happy-dom
// zerus-63rru.198, from the #2558 review: an archived team whose lead's row is
// not loaded had no route to its team; the list only said "The team's lead
// session is not on the session list." The page opens it by its ID, read-only,
// and the parts that need the lead say why. The list row and the team view are
// the Rust product writer's (team_page_screen.rs: the person's own Stop and
// Archive through the product operations, then the list and status reads).
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Show, createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "./fixtures/team-page-archived.json";
import type { TeamList, TeamView } from "../../store/teams";
import type { TeamsOut } from "../../types";

const data = fixture as unknown as { list: TeamList; team: TeamView };
const f = vi.hoisted(() => ({ actions: [] as unknown[], follows: [] as { team?: string | null }[] }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string, args?: Record<string, unknown>) => {
  if (command === "teams_follow") f.follows.push(args as { team?: string | null });
  if (command === "team_action") { f.actions.push(args!.call); return { outcome: "done", request_id: (args!.call as { request_id: string }).request_id, receipt: {} }; }
  return undefined;
}, isTauri: () => false }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
// The lead's row is not loaded: no session row at all.
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => [] }));

const archived = data.list.teams[0];
const load = <T,>(path: string) => import(/* @vite-ignore */ path) as Promise<T>;
let teams: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
const disposers: (() => void)[] = [];
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
const button = (host: HTMLElement, text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);
function host() { const element = document.createElement("div"); document.body.append(element); return element; }
async function list() {
  const { TeamsListPage } = await load<typeof import("../teams/TeamRowMenu")>("../teams/TeamRowMenu");
  const element = host(); disposers.push(render(() => createComponent(TeamsListPage, {}), element)); await flush();
  // Wave 3a (#2558) lists archived teams under Show archived.
  element.querySelector<HTMLButtonElement>("[data-teams-archived]")?.click(); await flush();
  return element;
}
beforeEach(async () => {
  vi.resetModules(); f.actions = []; f.follows = [];
  ({ teams } = await import("../../store/teams")); ui = await import("../../store/ui");
  ui.setPage("teams"); ui.setSelectedKey(undefined); ui.setMenu(undefined);
  teams.apply([entry("list", "list", data.list)]);
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("the product's archived team, with its lead bound but not loaded, opens its page from the list", async () => {
  expect([archived.status, !!archived.lead_session, data.team.bar.status]).toEqual(["archived", true, "archived"]);
  const element = await list();
  element.querySelector<HTMLButtonElement>("[data-team-row]")!.click(); await flush();
  expect(ui.page()).toBe("team");
  expect(element.textContent).not.toContain("The team's lead session is not on the session list.");
  const navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation");
  expect(navigation.teamPage()?.team).toBe(archived.id);
});

it("its row menu opens the page and says why the lead's session and Settings are not offered", async () => {
  const element = await list();
  element.querySelector<HTMLButtonElement>(`[data-team-menu="${archived.id}"]`)!.click();
  const items = (ui.menu()?.items ?? []).filter((item): item is Extract<typeof item, { label: string }> => "label" in item);
  expect(items.map((item) => item.label)).toEqual(["Open", "Open lead session", "Settings…"]);
  expect(items[1]).toMatchObject({ disabled: true, note: "The team's lead session is not on the session list." });
  expect(items[2]).toMatchObject({ disabled: true });
  items[0].action!(); expect(ui.page()).toBe("team");
});

it("the page shows the archived team read-only, and Lead chat says why the lead is not reachable here", async () => {
  const navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation");
  const { TeamPage } = await load<typeof import("../teams/TeamPage")>("../teams/TeamPage");
  navigation.openTeamPage(archived.id);
  const element = host();
  disposers.push(render(() => createComponent(Show, { keyed: true, get when() { return ui.page() === "team"; }, get children() { return createComponent(TeamPage, {}); } }), element));
  await flush();
  expect(f.follows.at(-1)?.team).toBe(archived.id);
  teams.apply([entry(`team:${archived.id}`, "team", data.team)]); await flush();
  expect(element.querySelector("[data-team-tab]")).not.toBeNull();
  expect(element.querySelector("[data-team-page-archived]")?.textContent).toBe("This team is archived. It is shown read-only.");
  expect(element.querySelector<HTMLButtonElement>("[data-team-add-task]")!.disabled).toBe(true);
  const memberActions = [...element.querySelectorAll<HTMLButtonElement>('button[aria-label="Member actions"]')];
  expect(memberActions.length).toBe(data.team.members.length);
  expect(memberActions.every((action) => action.disabled)).toBe(true);
  const rename = button(element, "Rename team");
  if (rename) { expect(rename.disabled).toBe(true); rename.click(); await flush(); }
  expect(f.actions).toEqual([]);
  button(element, "Lead chat")!.click();
  await vi.waitFor(() => expect(element.querySelector(".team-lead-chat")).not.toBeNull(), { timeout: 10_000 }); // the lazy drawer loads the Composer
  const chat = element.querySelector<HTMLElement>(".team-lead-chat")!;
  expect(chat.textContent).toContain("Lead chat lives in the lead's session. Open it to talk to the lead.");
  expect(chat.textContent).toContain("The team's lead session is not on the session list.");
  expect(button(chat, "Open lead session")).toBeUndefined();
});
