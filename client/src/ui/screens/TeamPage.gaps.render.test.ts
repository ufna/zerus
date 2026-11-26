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
const f = vi.hoisted(() => ({
  rows: [] as Session[], archives: [] as Session[], groups: {} as Record<string, string>,
  actions: [] as unknown[], stop: "" as "" | "confirm" | "pending", swap: false,
  restorePending: false, batchBusy: false,
}));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string, args?: Record<string, unknown>) => {
  if (command === "team_action") {
    const call = args!.call as { request_id: string; action: { kind: string } };
    f.actions.push(call);
    if (call.action.kind === "stop_member" && f.stop === "confirm") return { outcome: "confirm", confirmation: {
      name: "Stop member", title: "Stop member", text: "Stop the lead seat", action: "Stop",
    } };
    if (call.action.kind === "stop_member" && f.stop === "pending") return { outcome: "pending", request_id: call.request_id };
    if (call.action.kind === "swap_member" && f.swap) return { outcome: "confirm", confirmation: {
      name: "Swap member", title: "Swap member", text: "Swap the lead seat", action: "Swap",
    } };
    return { outcome: "done", request_id: call.request_id, receipt: {} };
  }
  return undefined;
}, isTauri: () => false }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()),
  machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa", host: "workstation.example" }, studio: { machine_id: "00000000-0000-4000-8000-0000000000bb", host: "studio.example" } } }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => f.rows }));
vi.mock("../../store/archive", async (original) => {
  const fixture = await original<typeof import("../../store/archive")>();
  return { ...fixture, archive: { ...fixture.archive, rows: () => f.archives } };
});
vi.mock("../../store/restores", async (original) => ({ ...(await original<object>()), isRestorePending: () => f.restorePending }));
vi.mock("../../store/batch", async (original) => ({ ...(await original<object>()), running: () => f.batchBusy }));
vi.mock("../../store/organization", async (original) => ({ ...(await original<object>()), groupOf: (key: string) => f.groups[key] ?? "inbox" }));
vi.mock("../../store/ui", async (original) => {
  const ui = await original<typeof import("../../store/ui")>();
  return { ...ui, selected: () => [...f.rows, ...f.archives].find((row) => row.key === ui.selectedKey()) };
});

const view = data.team;
const lead = view.members.find((member) => member.lead)!.session!;
const listed = (extra: Partial<TeamSummary> = {}): TeamSummary => ({ ...data.running.teams[0], id: view.bar.id, name: view.bar.name, status: view.bar.status,
  revision: view.revision, project: view.bar.project, home: HERE, lead_session: { machine: lead.machine, session_id: lead.session_id }, ...extra });
const load = <T,>(path: string) => import(/* @vite-ignore */ path) as Promise<T>;
let teams: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
let toSession: typeof import("../../store/catalog").toSession;
let archivedSession: typeof import("../../store/archive").archivedSession;
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
async function openPage(summary: TeamSummary = listed(), shown = view) {
  teams.apply([entry("list", "list", { ...data.running, teams: [summary] })]);
  const navigation = await load<typeof import("../teams/team-navigation")>("../teams/team-navigation");
  const { TeamPage } = await load<typeof import("../teams/TeamPage")>("../teams/TeamPage");
  navigation.openTeamPage(view.bar.id);
  const element = host();
  disposers.push(render(() => createComponent(Show, { keyed: true, get when() { return ui.page() === "team"; }, get children() { return createComponent(TeamPage, {}); } }), element));
  await flush(); teams.apply([entry(`team:${view.bar.id}`, "team", shown)]); await flush();
  expect(element.querySelector("[data-team-tab]")).not.toBeNull();
  return element;
}
async function openChat(element: HTMLElement) {
  button(element, "Lead chat")!.click();
  await vi.waitFor(() => expect(element.querySelector(".team-lead-chat")).not.toBeNull(), { timeout: 10_000 }); // the lazy drawer loads the Composer
  return element.querySelector<HTMLElement>(".team-lead-chat")!;
}
beforeEach(async () => {
  vi.resetModules(); f.rows = []; f.archives = []; f.groups = {}; f.actions = []; f.stop = "";
  f.restorePending = false; f.batchBusy = false; f.swap = false;
  ({ teams } = await import("../../store/teams")); ui = await import("../../store/ui");
  ({ toSession } = await import("../../store/catalog")); ({ desktopService: service } = await import("../../store/desktopService"));
  ({ archivedSession } = await import("../../store/archive"));
  ui.setPage("teams"); ui.setSelectedKey(undefined); ui.setDetailTab("activity"); ui.setSubagentOpen(undefined);
  ui.setStateFilter("all");
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

const a = (host: HTMLElement) => host.querySelector<HTMLElement>("[data-team-lead-availability]")!;
const leadAction = (host: HTMLElement) => a(host).querySelector<HTMLButtonElement>("[data-team-lead-review]");
const leadOpen = (host: HTMLElement) => a(host).querySelector<HTMLButtonElement>("[data-team-lead-open]")!;
const rosterLaunch = (i: number) => `00000000-0000-4000-8000-00000000d00${i}`;
const liveMembers = () => [1, 2, 3].map((i) => row(`m${i}`, rosterLaunch(i)));
function emptyRoster(openWork: number | null = 0, leadBinding = "run_id:run-lead-row") {
  const members = [
    { ...view.members[0], open_work: openWork, session: { machine: HERE, session_id: leadBinding } },
    ...[1, 2, 3, 4].map((i) => ({ ...view.members[1], id: `m${i}`, state: "active",
      session: { machine: HERE, session_id: `launch_id:${rosterLaunch(i)}` } })),
  ];
  return { ...view, bar: { ...view.bar, counts: { ...view.bar.counts,
    members: { active: 5, total: 5, not_created: 0, paused: 0, removed: 0 } } }, members };
}
function archivedLead(allowed = "restore") {
  return archivedSession("", "workstation.example", {
    ...presentation.ready as unknown as ViewRow,
    name: "codex/example/lead-row", launch_id: leadRow().launchId, run_id: leadRow().runId,
    state: "archived", archive_id: "lead-archive", resumable: true, allowed_actions: [allowed],
  });
}
const savedLead = (allowed = "forget"): Session => ({ ...leadRow(), state: "stopped", processState: "exited", resumable: true, allowed });

const languages = [
  { lang: "en", roster: "Active roster seats (5)", liveCount: "Live sessions shown (3)",
    empty: "The queue is empty.", archived: "The lead's session is archived.", live: "The lead's session is live.",
    missing: "The lead's session is missing from the current list.", browse: "Browse Archive",
    open: "Open archived lead session", restore: "choose Restore session to recover its conversation", unavailable: "Resume is unavailable for the saved lead session.",
    originalRun: "Restoring the conversation starts a new run", resume: "choose Resume session",
    stop: "Review Stop member", swap: "Review Swap", dialog: "Swap lead", receipts: "checks the saved receipts" },
  { lang: "ru", roster: "Активные места в составе (5)", liveCount: "Сессии, работающие по данным списка (3)",
    empty: "Очередь пуста.", archived: "Сессия лида в архиве.", live: "Сессия лида работает.",
    missing: "Сессии лида нет в текущем списке.", browse: "Открыть архив",
    open: "Открыть архивную сессию лида", restore: "выберите «Восстановить сессию»", unavailable: "Сохранённую сессию лида нельзя возобновить.",
    originalRun: "Восстановление диалога создаёт новый запуск", resume: "выберите «Возобновить сессию»",
    stop: "Открыть остановку участника", swap: "Открыть замену", dialog: "Заменить lead", receipts: "проверит сохранённые результаты" },
];
async function useLanguage(lang: string) {
  const settings = await import("../../store/settings");
  await settings.settingsReady;
  await settings.setSetting("workspace/language", lang, { persist: false });
  const i18n = await import("../../i18n");
  i18n.installUiLanguage();
  await vi.waitFor(() => expect(i18n.uiLanguage()).toBe(lang));
}

it.each(languages)("shows the archived lead and separate roster/live counts in $lang", async (copy) => {
  await useLanguage(copy.lang);
  f.rows = [...liveMembers(), { ...row("m4", rosterLaunch(4)), state: "stopped", processState: "exited" }];
  f.archives = [archivedLead()];
  const host = await openPage(listed(), emptyRoster());
  expect(host.querySelector("[data-team-bar]")?.textContent).toContain(copy.roster);
  expect(host.querySelector("[data-team-bar]")?.textContent).toContain(copy.liveCount);
  expect(host.querySelector("[data-team-board]")?.textContent).toContain(copy.empty);
  expect(a(host).dataset.teamLeadAvailability).toBe("archived");
  expect(a(host).textContent).toContain(copy.archived);
  expect(a(host).textContent).toContain(copy.restore);
  expect(a(host).textContent).toContain(copy.originalRun);
  expect(leadOpen(host).textContent).toBe(copy.open);
  expect(leadAction(host)).toBeNull();
  leadOpen(host).click();
  expect(ui.page()).toBe("sessions");
  expect(ui.stateFilter()).toBe("archived");
  expect(ui.selected()?.archiveId).toBe("lead-archive");
  expect(ui.detailTab()).toBe("activity");
  expect(f.actions).toEqual([]);
});

it.each(languages)("keeps a missing lead unknown until the exact live row is listed in $lang", async (copy) => {
  await useLanguage(copy.lang);
  f.rows = liveMembers();
  f.archives = [{ ...archivedLead(), computer: "studio" }];
  const current = emptyRoster();
  const host = await openPage(listed(), current);
  expect(a(host).dataset.teamLeadAvailability).toBe("unavailable");
  expect(a(host).textContent).toContain(copy.missing);
  expect(leadOpen(host).textContent).toBe(copy.browse);
  expect(leadAction(host)).toBeNull();
  f.rows.push(leadRow());
  teams.apply([entry(`team:${view.bar.id}`, "team", { ...current, revision: current.revision + 1 })]);
  await flush();
  expect(a(host).dataset.teamLeadAvailability).toBe("live");
  expect(a(host).textContent).toContain(copy.live);
  expect(f.actions).toEqual([]);
});

it.each(languages)("uses the existing Stop confirmation then offers Swap on a stopped seat in $lang", async (copy) => {
  await useLanguage(copy.lang);
  f.rows = [...liveMembers(), savedLead()];
  f.stop = "confirm";
  f.swap = true;
  const current = emptyRoster(0, lead.session_id);
  const host = await openPage(listed(), current);
  expect(a(host).textContent).toContain(copy.unavailable);
  expect(a(host).textContent).toContain(copy.receipts);
  expect(leadAction(host)?.textContent).toBe(copy.stop);
  leadAction(host)!.click();
  await vi.waitFor(() => expect(teams.confirmation()?.call.action).toEqual({ kind: "stop_member", member: "lead" }));
  expect(f.actions).toHaveLength(1);
  expect(teams.confirmation()?.call.confirmed).toBe(false);
  teams.cancelConfirmation();
  const stopped = { ...current, revision: current.revision + 1,
    members: current.members.map((member) => member.lead ? { ...member, state: "paused", person_stopped: true } : member) };
  teams.apply([entry(`team:${view.bar.id}`, "team", stopped)]);
  await flush();
  expect(leadAction(host)?.textContent).toBe(copy.swap);
  leadAction(host)!.click();
  await vi.waitFor(() => expect(host.querySelector("dialog[open] h2")?.textContent).toBe(copy.dialog));
  expect(f.actions).toHaveLength(1);
  host.querySelector<HTMLButtonElement>("dialog[open] button[type=submit]")!.click();
  await vi.waitFor(() => expect(teams.confirmation()?.call.action).toMatchObject({ kind: "swap_member", member: "lead" }));
  expect(teams.confirmation()?.call.confirmed).toBe(false);
  expect(f.actions).toHaveLength(2);
});

it.each(languages)("offers Resume before replacement for a resumable saved lead in $lang", async (copy) => {
  await useLanguage(copy.lang);
  f.rows = [savedLead("resume")];
  const host = await openPage(listed(), emptyRoster(0, lead.session_id));
  expect(a(host).textContent).toContain(copy.resume);
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toEqual([]);
});

it("points a pending lead action to Check and withholds the replacement path", async () => {
  f.rows = [savedLead()];
  f.stop = "pending";
  const host = await openPage(listed(), emptyRoster(0, lead.session_id));
  leadAction(host)!.click();
  await vi.waitFor(() => expect(a(host).textContent).toContain("Use Check on the pending lead action"));
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toHaveLength(1);
});

it.each(["restore", "forget"])("ignores a single archived run's %s capability for a launch-only lead", async (allowed) => {
  // The Archive may show only the earlier run on a filtered page. Its
  // capability cannot identify or determine recovery for the lead's run.
  f.archives = [{ ...archivedLead(allowed), key: "\narchive:earlier", archiveId: "earlier", runId: "earlier-run" }];
  const host = await openPage(listed(), emptyRoster(0, lead.session_id));
  expect(a(host).dataset.teamLeadAvailability).toBe("unavailable");
  expect(a(host).textContent).toContain("Browse Archive and verify the lead's run.");
  expect(leadAction(host)).toBeNull();
  leadOpen(host).click();
  expect(ui.page()).toBe("sessions");
  expect(ui.stateFilter()).toBe("archived");
  expect(ui.selected()).toBeUndefined();
  expect(f.actions).toEqual([]);
});

it("does not use another archived run of the same launch for an exact run binding", async () => {
  f.archives = [{ ...archivedLead(), key: "\narchive:earlier", archiveId: "earlier", runId: "earlier-run" }];
  const host = await openPage(listed(), emptyRoster());
  expect(a(host).dataset.teamLeadAvailability).toBe("unavailable");
  expect(leadOpen(host).textContent).toBe("Browse Archive");
  expect(leadAction(host)).toBeNull();
});

it.each([
  ["restorePending", "Check the pending restore in the archived session."],
  ["batchBusy", "Wait for the session update to finish before checking Restore session."],
] as const)("withholds Restore guidance while %s is set", async (flag, reason) => {
  f.archives = [archivedLead()];
  f[flag] = true;
  const host = await openPage(listed(), emptyRoster());
  expect(a(host).textContent).toContain(reason);
  expect(a(host).textContent).not.toContain("choose Restore session");
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toEqual([]);
});

it("explains the unsettled-work restriction without suggesting a replacement", async () => {
  f.rows = [savedLead()];
  const host = await openPage(listed(), emptyRoster(1, lead.session_id));
  expect(a(host).textContent).toContain("The lead has open or unsettled work (1). Swap requires that work to settle.");
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toEqual([]);
});

it("waits for a pending restore when a stopped launch-bound row is listed", async () => {
  f.rows = [savedLead()];
  f.restorePending = true;
  const host = await openPage(listed(), emptyRoster(0, lead.session_id));
  expect(a(host).textContent).toContain("Wait for the pending restore before changing the lead seat.");
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toEqual([]);
});

it("withholds Stop and Swap for a run-bound archive with no Restore capability", async () => {
  f.archives = [archivedLead("forget")];
  const host = await openPage(listed(), emptyRoster());
  expect(a(host).dataset.teamLeadAvailability).toBe("archived");
  expect(a(host).textContent).toContain("Restore is unavailable for this archive.");
  expect(a(host).textContent).toContain("Swap requires a launch-bound seat.");
  expect(leadAction(host)).toBeNull();
  expect(f.actions).toEqual([]);
});

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

it.each([["question", "ask_lead"], ["note", "leave_lead_note"]])(
  "a running team with an archived lead saves the %s for that team",
  async (mode, kind) => {
    const unrelated = row("unrelated-lead", "00000000-0000-4000-8000-0000000000d1");
    f.rows = [unrelated, { ...leadRow(), state: "archived" }];
    ui.setSelectedKey(unrelated.key);
    const element = await openPage();
    const chat = await openChat(element);
    expect(chat.textContent).toContain("The lead's session is archived. Restore it to talk to the lead.");
    expect(chat.querySelector(".cmp-col")).toBeNull();

    const contact = element.querySelector<HTMLButtonElement>("[data-team-contact]")!;
    expect(contact.disabled).toBe(false);
    contact.click();
    const form = element.querySelector(".team-lead-contact form")!;
    const select = form.querySelector("select")!;
    select.value = mode;
    select.dispatchEvent(new Event("change", { bubbles: true }));
    const text = form.querySelector("textarea")!;
    const body = "For our lead\nKeep this with the team.";
    text.value = body;
    text.dispatchEvent(new Event("input", { bubbles: true }));
    form.dispatchEvent(new Event("submit", { cancelable: true }));
    await vi.waitFor(() => expect(f.actions).toHaveLength(1));
    expect(f.actions[0]).toMatchObject({
      team: view.bar.id, expected_revision: view.revision,
      action: { kind, text: body },
    });
    expect(ui.page()).toBe("team");
  },
);

it("Lead chat opens the restored lead when its archived row also matches the launch", async () => {
  const archived = { ...leadRow(), state: "archived" } as const;
  const restored = row("restored-lead", lead.session_id.slice("launch_id:".length));
  f.rows = [archived, restored];
  const chat = await openChat(await openPage());
  expect(chat.querySelector("[data-lead-chat-reason]")).toBeNull();
  button(chat, "Open lead session")!.click();
  expect(ui.selectedKey()).toBe(restored.key);
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
  expect(teamTaskCreation()).toEqual({ project: view.bar.project, team: view.bar.id, fromPage: { bar: view.bar, routing_pipelines: view.routing_pipelines } });
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
