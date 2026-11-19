// @vitest-environment happy-dom
// zerus-63rru.198 slice 4: what only the team page lacks. Lead chat names its
// cause and the way to the lead (audit C4); Add task reaches the project's
// New task form; Open task switches to Sessions; a team homed on another
// computer, or a window without its desktop service, is read-only with one
// banner. The team view and list rows are the Rust product writer's
// (teams_screen.rs: screens/fixtures/teams-screen.json); session rows are
// synthetic core rows mapped by the product's toSession; only the desktop
// invoke is replaced, and it records what the window sends.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Show, createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "./fixtures/teams-screen.json";
import presentation from "../sessions/fixtures/team-session-presentation.json";
import type { TeamList, TeamSummary, TeamView } from "../../store/teams";
import type { Session } from "../sessions/types";
import type { DesktopService, TeamsOut, ViewRow } from "../../types";

const data = fixture as unknown as { running: TeamList; team: TeamView };
const HERE = "00000000-0000-4000-8000-0000000000aa", STUDIO = "00000000-0000-4000-8000-0000000000bb";
const f = vi.hoisted(() => ({ rows: [] as Session[], groups: {} as Record<string, string>, actions: [] as unknown[] }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string, args?: Record<string, unknown>) => {
  if (command === "team_action") { f.actions.push(args!.call); return { outcome: "done", request_id: (args!.call as { request_id: string }).request_id, receipt: {} }; }
  return undefined;
}, isTauri: () => false }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()),
  machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa", host: "workstation.example" }, studio: { machine_id: "00000000-0000-4000-8000-0000000000bb", host: "studio.example" } } }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => f.rows }));
vi.mock("../../store/organization", async (original) => ({ ...(await original<object>()), groupOf: (key: string) => f.groups[key] ?? "inbox" }));
vi.mock("../../store/ui", async (original) => {
  const ui = await original<typeof import("../../store/ui")>();
  return { ...ui, selected: () => f.rows.find((row) => row.key === ui.selectedKey()) };
});

const view = data.team;
const lead = view.members.find((member) => member.lead)!.session!;
const listed = (extra: Partial<TeamSummary> = {}): TeamSummary => ({ ...data.running.teams[0], id: view.bar.id, name: view.bar.name, status: view.bar.status,
  revision: view.revision, project: view.bar.project, home: HERE, lead_session: { machine: lead.machine, session_id: lead.session_id }, ...extra });
const load = <T,>(path: string) => import(/* @vite-ignore */ path) as Promise<T>;
let teams: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
let toSession: typeof import("../../store/catalog").toSession;
let service: typeof import("../../store/desktopService").desktopService;
const disposers: (() => void)[] = [];
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
const button = (host: HTMLElement, text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);
function row(key: string, launch: string): Session {
  return toSession("", key, { ...presentation.ready as unknown as ViewRow, name: `codex/example/${key}`, title: key, launch_id: launch,
    session_id: `native-${key}`, run_id: `run-${key}` });
}
const leadRow = () => row("lead-row", lead.session_id.slice("launch_id:".length));
function host() { const element = document.createElement("div"); document.body.append(element); return element; }
/** The page as App's page Switch mounts it, with the team's view applied. */
async function openPage(summary: TeamSummary = listed()) {
  teams.apply([entry("list", "list", { ...data.running, teams: [summary] })]);
  const navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation");
  const { TeamPage } = await load<typeof import("../teams/TeamPage")>("../teams/TeamPage");
  navigation.openTeamPage(view.bar.id);
  const element = host();
  disposers.push(render(() => createComponent(Show, { keyed: true, get when() { return ui.page() === "team"; }, get children() { return createComponent(TeamPage, {}); } }), element));
  await flush(); teams.apply([entry(`team:${view.bar.id}`, "team", view)]); await flush();
  expect(element.querySelector("[data-team-tab]")).not.toBeNull();
  return element;
}
async function openChat(element: HTMLElement) {
  button(element, "Lead chat")!.click();
  await vi.waitFor(() => expect(element.querySelector(".team-lead-chat")).not.toBeNull());
  return element.querySelector<HTMLElement>(".team-lead-chat")!;
}
beforeEach(async () => {
  vi.resetModules(); f.rows = []; f.groups = {}; f.actions = [];
  ({ teams } = await import("../../store/teams")); ui = await import("../../store/ui");
  ({ toSession } = await import("../../store/catalog")); ({ desktopService: service } = await import("../../store/desktopService"));
  ui.setPage("teams"); ui.setSelectedKey(undefined); ui.setDetailTab("activity"); ui.setSubagentOpen(undefined);
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("Lead chat on the page says where the lead is talked to, and opens its listed session (audit C4)", async () => {
  f.rows = [leadRow()];
  const chat = await openChat(await openPage());
  expect(chat.textContent).toContain("Lead chat lives in the lead's session. Open it to talk to the lead.");
  expect(chat.querySelector(".cmp-col")).toBeNull();
  button(chat, "Open lead session")!.click();
  expect(ui.page()).toBe("sessions"); expect(ui.selectedKey()).toBe(f.rows[0].key); expect(ui.detailTab()).toBe("team");
});

it("Lead chat on the page names why the lead's session cannot be opened", async () => {
  const chat = await openChat(await openPage());
  expect(chat.textContent).toContain("The team's lead session is not on the session list.");
  expect(button(chat, "Open lead session")).toBeUndefined();
});

it("Lead chat in the lead's tab names the cause and its fix instead of one sentence for all (audit C4)", async () => {
  f.rows = [leadRow()]; ui.setPage("sessions"); ui.setSelectedKey(f.rows[0].key); ui.setSubagentOpen("child-agent");
  const { TeamLeadChat } = await load<typeof import("../teams/TeamLeadChat")>("../teams/TeamLeadChat");
  const element = host(); disposers.push(render(() => createComponent(TeamLeadChat, { view }), element)); await flush();
  expect(element.textContent).toContain("Close the subagent view to talk to the lead.");
  ui.setSubagentOpen(undefined); f.rows = [{ ...f.rows[0], state: "archived" }]; ui.setSelectedKey(undefined); ui.setSelectedKey(f.rows[0].key); await flush();
  expect(element.textContent).toContain("The lead's session is archived. Restore it to talk to the lead.");
  expect(element.textContent).not.toContain("The lead's current conversation is not available for sending.");
});

it("Add task on the page opens the project's New task form in Sessions with this team chosen", async () => {
  const project = row("project-row", "00000000-0000-4000-8000-0000000000d1"); f.rows = [project]; f.groups = { [project.key]: view.bar.project };
  const element = await openPage();
  const add = element.querySelector<HTMLButtonElement>("[data-team-add-task]")!;
  expect(add.disabled).toBe(false);
  add.click(); await flush();
  const { teamTaskCreation } = await import("../../store/taskCreation");
  const { inspectorOpen, inspectorTab } = await import("../../store/inspector");
  expect(ui.page()).toBe("sessions"); expect(ui.selectedKey()).toBe(project.key);
  expect(inspectorOpen()).toBe(true); expect(inspectorTab()).toBe("tasks");
  expect(teamTaskCreation()).toEqual({ project: view.bar.project, team: view.bar.id });
});

it("Add task on the page is disabled with its reason when the project has no session listed", async () => {
  f.rows = [row("elsewhere", "00000000-0000-4000-8000-0000000000d2")];
  const element = await openPage();
  const add = element.querySelector<HTMLButtonElement>("[data-team-add-task]")!;
  expect(add.disabled).toBe(true);
  expect(add.title).toBe("This team's project has no session on the session list to add a task from.");
});

it("Open task switches to Sessions from the page and stays put elsewhere", async () => {
  const { openTeamTask } = await load<typeof import("../teams/team-task-navigation")>("../teams/team-task-navigation");
  const { selectedTask } = await import("../../store/taskNavigation");
  ui.setPage("team"); openTeamTask(view.bar.project, "00000000-0000-4000-8000-0000000000e1");
  expect(ui.page()).toBe("sessions"); expect(ui.detailTab()).toBe("taskview");
  expect(selectedTask()).toEqual({ project: view.bar.project, uuid: "00000000-0000-4000-8000-0000000000e1" });
  ui.setPage("waiting"); openTeamTask(view.bar.project, "00000000-0000-4000-8000-0000000000e2");
  expect(ui.page()).toBe("waiting");
});

it("shows a team homed on another computer read-only, naming that computer", async () => {
  const element = await openPage(listed({ home: STUDIO }));
  expect(element.querySelector("[data-team-page-home]")?.textContent).toBe("This team's home is another computer (studio). Open it there.");
  const pause = button(element, "Pause")!;
  expect(pause.disabled).toBe(true);
  pause.click(); await flush(); expect(f.actions).toEqual([]);
  expect(element.querySelector<HTMLButtonElement>("[data-team-add-task]")!.disabled).toBe(true);
});

it("says once that the desktop service is not connected, and allows changes again when it is", async () => {
  service.apply({ connected: false } as DesktopService);
  const element = await openPage();
  expect(element.querySelector("[data-team-page-service]")?.textContent).toBe("Connect this window to its desktop service before changing a team.");
  expect(button(element, "Pause")!.disabled).toBe(true);
  service.apply({ connected: true } as DesktopService); await flush();
  expect(element.querySelector("[data-team-page-service]")).toBeNull();
  expect(element.querySelector("[data-team-page-home]")).toBeNull();
  button(element, "Pause")!.click(); await flush();
  expect(f.actions).toHaveLength(1);
});
