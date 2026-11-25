// @vitest-environment happy-dom
// Recovery regressions through normal TeamTab and independent TeamPage.
// Card/list rows were emitted by the product writers and production Rust
// projector in member-recovery-review-75909911-20261125; only native I/O is faked.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { teamView, TEAM_ID, catalog } from "./fixtures";
import list from "./fixtures/member-recovery-list.json";
import mixed from "./fixtures/member-recovery-mixed.json";
import type { DrawerRead, DrawerPage } from "../../store/teamDrawers";
import type { TeamCall, TeamOutcome } from "../../store/teams";
const f = vi.hoisted(() => ({
  selected: () => undefined as any,
  choose: (_row: any) => {},
  model: null as any,
  active: 0,
  maximum: 0,
  pages: new Map<string | null, any>(),
  reads: [] as DrawerRead[],
  calls: [] as TeamCall[],
  checks: [] as TeamCall[],
  outcomes: [] as ((call: TeamCall) => Promise<TeamOutcome>)[],
}));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: async (command: string, args?: { read: DrawerRead }) => {
    if (command !== "teams_drawer_page") return undefined;
    const read = args!.read;
    f.reads.push({ ...read });
    f.active++;
    f.maximum = Math.max(f.maximum, f.active);
    try {
      const page = f.pages.get(read.after);
      if (page instanceof Error) throw page;
      return await (typeof page === "function"
        ? page(read)
        : (page ?? { team: read.team, kind: "cards", rows: [], after: read.after, next: null }));
    } finally {
      f.active--;
    }
  },
}));
vi.mock("../../store/ui", async () => {
  const { createSignal } = await import("solid-js");
  const [row, set] = createSignal<any>();
  const [menu, setMenu] = createSignal<any>();
  f.selected = row;
  f.choose = set;
  return {
    selected: row,
    page: () => "sessions",
    detailTab: () => "team",
    setPage: vi.fn(),
    setDetailTab: vi.fn(),
    setSelectedKey: vi.fn(),
    setSubagentOpen: vi.fn(),
    setMenu,
    menu,
    visibleMenu: menu,
  };
});
vi.mock("../../store/machines", async (original) => ({
  ...(await original<object>()),
  machineCards: { workstation: { machine_id: "00000000-0000-4000-8000-0000000000aa" } },
}));
vi.mock("../../store/taskNavigation", async (original) => ({
  ...(await original<object>()),
  openTask: vi.fn(),
}));
vi.mock("../../store/catalog", async (original) => ({
  ...(await original<object>()),
  agentKind: (agent: string) => agent,
  orderedRows: () => [],
}));
vi.mock("../sessions/SessionCard", () => ({ AgentTile: () => null, ProviderGlyph: () => null }));
vi.mock("../../store/teamTaskIntake", () => ({
  teamTaskBlocked: () => undefined,
  canDropTeamTask: () => false,
  requestTeamTask: vi.fn(),
  teamTaskDragOver: vi.fn(),
  teamTaskDrop: vi.fn(),
}));
vi.mock("../../store/teams", async (original) => {
  const actual = await original<any>();
  const transport = {
    async follow(scope: any) {
      if (scope.team)
        queueMicrotask(() =>
          f.model.apply([{ computer: "", key: `team:${TEAM_ID}`, kind: "team", payload: teamView }]),
        );
    },
    async hidden() {},
    async tasksChanged() {},
    async page() {},
    async act(call: TeamCall) {
      f.calls.push(call);
      return (
        f.outcomes.shift()?.(call) ?? { outcome: "done", receipt: { ok: true }, request_id: call.request_id }
      );
    },
    async actAgain(call: TeamCall) {
      f.checks.push(call);
      return { outcome: "done", receipt: { ok: true }, request_id: call.request_id };
    },
    async dismiss() {},
    async catalog() {
      return { ok: true, catalog };
    },
    async saveTemplate() {},
    async project() {
      return { state: "ok" };
    },
    async preview() {
      return {};
    },
  };
  f.model = actual.createTeams(transport);
  return { ...actual, teams: f.model };
});
const { TeamTab } = await import("./TeamTab");
const { TeamPage } = await import("./TeamPage");
const { openTeamPage } = await import("./team-navigation");
const { MenuLayer } = await import("../components/Overlays");
let dispose: (() => void) | undefined;
const page = (after: string | null, rows: DrawerPage["rows"], next: string | null = null): DrawerPage => ({
  team: TEAM_ID,
  kind: "cards",
  after,
  rows,
  next,
});
const own = mixed.rows.find((row) =>
  row.card?.parameters?.some((pair) => pair[0] === "member" && pair[1] === "m2"),
)!;
const other = mixed.rows.find((row) =>
  row.card?.parameters?.some((pair) => pair[0] === "member" && pair[1] === "m3"),
)!;
beforeEach(() => {
  f.active = 0;
  f.maximum = 0;
  f.pages.clear();
  f.reads = [];
  f.calls = [];
  f.checks = [];
  f.outcomes = [];
  f.model.apply([{ computer: "", key: "list", kind: "list", payload: list }]);
  f.choose({
    key: "selected-lead",
    computer: "workstation",
    launchId: "00000000-0000-4000-8000-000000000064",
    sessionId: "unused",
    runId: "",
  });
});
afterEach(async () => {
  dispose?.();
  dispose = undefined;
  f.choose(undefined);
  await f.model.closeTeam();
  document.body.replaceChildren();
});
async function setup(entry: "tab" | "page") {
  if (entry === "page") openTeamPage(TEAM_ID);
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(
    () => [createComponent(entry === "tab" ? TeamTab : TeamPage, {}), createComponent(MenuLayer, {})],
    host,
  );
  await vi.waitFor(() => expect(host.querySelector("[data-team-tab]")).not.toBeNull());
  host.querySelector<HTMLButtonElement>("[data-team-members-toggle]")!.click();
  return host;
}
function action(host: HTMLElement, member: string, label: string) {
  host.querySelector<HTMLButtonElement>(`[data-member="${member}"] [aria-label="Member actions"]`)!.click();
  const button = [...document.querySelectorAll<HTMLButtonElement>(".menu-item")].find(
    (b) => b.textContent === label,
  );
  expect(button).toBeDefined();
  button!.click();
}
const focus = (host: HTMLElement) => host.querySelector("[data-launch-focus]")?.textContent;

const entries = ["tab", "page"] as const;
const noCard = (host: HTMLElement) =>
  expect(focus(host)).toBe("m2 has no open start card. Zerus starts the member when there is room.");
async function focused(host: HTMLElement, member = "m2") {
  const seq = member === "m2" ? own.seq : other.seq;
  await vi.waitFor(() => expect(focus(host)).toBe(`Retry on ${member}'s start card starts it again.`));
  const retry = host.querySelector<HTMLButtonElement>(
    `[data-proposal="${seq}"] [data-card-decision="retry"]`,
  )!;
  await vi.waitFor(() => expect(document.activeElement).toBe(retry));
  expect(f.calls).toHaveLength(0);
  return retry;
}
it.each(entries)(
  "Refresh retires a stale no-card result and focuses the fresh exact Retry (%s)",
  async (entry) => {
    f.pages.set(null, page(null, []));
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => noCard(host));
    let resolve!: (value: DrawerPage) => void;
    f.pages.set(
      null,
      () =>
        new Promise<DrawerPage>((r) => {
          resolve = r;
        }),
    );
    host.querySelector<HTMLButtonElement>("[data-drawer-refresh]")!.click();
    expect(focus(host)).toBeUndefined();
    expect(f.calls).toHaveLength(0);
    resolve(mixed as unknown as DrawerPage);
    const retry = await focused(host);
    retry.click();
    await vi.waitFor(() => expect(f.calls).toHaveLength(1));
    expect(f.calls[0]).toMatchObject({
      team: TEAM_ID,
      expected_revision: teamView.revision,
      action: { kind: "decide", card: own.seq, decision: "retry" },
    });
    expect(f.reads.map((r) => r.after)).toEqual([null, null]);
  },
);
it.each(entries)(
  "the same Launch again is a fresh bounded request while Cards stays open (%s)",
  async (entry) => {
    f.pages.set(null, page(null, []));
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => noCard(host));
    f.pages.set(null, mixed);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => expect(f.reads).toHaveLength(2));
    await focused(host);
    expect(f.reads.map((r) => r.after)).toEqual([null, null]);
  },
);
it.each(entries)(
  "a different member retires the pending forward read and restarts at page one (%s)",
  async (entry) => {
    let resolve!: (value: DrawerPage) => void;
    f.pages.set(null, page(null, [other as any], "p1"));
    f.pages.set(
      "p1",
      () =>
        new Promise<DrawerPage>((r) => {
          resolve = r;
        }),
    );
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => expect(f.reads).toHaveLength(2));
    action(host, "m3", "Launch again…");
    expect(f.reads).toHaveLength(2);
    expect(focus(host)).toBeUndefined();
    resolve(page("p1", [own as any]));
    const retry = await focused(host, "m3");
    expect(f.maximum).toBe(1);
    expect(f.reads.map((r) => r.after)).toEqual([null, "p1", null]);
    retry.click();
    await vi.waitFor(() => expect(f.calls).toHaveLength(1));
    expect(f.calls[0].action).toEqual({ kind: "decide", card: other.seq, decision: "retry" });
  },
);
it.each(entries)(
  "a current revision restarts lookup and its Retry uses that revision (%s)",
  async (entry) => {
    f.pages.set(null, page(null, []));
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => noCard(host));
    f.pages.set(null, mixed);
    const revision = teamView.revision + 1;
    f.model.apply([
      {
        computer: "",
        key: `team:${TEAM_ID}`,
        kind: "team",
        payload: { ...teamView, revision, bar: { ...teamView.bar, revision } },
      },
    ]);
    const retry = await focused(host);
    retry.click();
    await vi.waitFor(() => expect(f.calls).toHaveLength(1));
    expect(f.calls[0].expected_revision).toBe(revision);
  },
);
it.each(entries)(
  "unavailable refreshed reads never keep an absence claim or enable old Retry (%s)",
  async (entry) => {
    f.pages.set(null, page(null, []));
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await vi.waitFor(() => noCard(host));
    f.pages.set(null, new Error("drawer_busy: finishing"));
    host.querySelector<HTMLButtonElement>("[data-drawer-refresh]")!.click();
    await vi.waitFor(() => expect(host.textContent).toContain("Use Refresh."));
    expect(focus(host)).toBeUndefined();
    expect(f.calls).toHaveLength(0);
    f.pages.set(null, mixed);
    host.querySelector<HTMLButtonElement>("[data-drawer-refresh]")!.click();
    await focused(host);
  },
);
it.each(entries)("fresh member lookup stays bounded and makes no decision (%s)", async (entry) => {
  for (let n = 0; n < 6; n++)
    f.pages.set(n ? `p${n}` : null, page(n ? `p${n}` : null, [other as any], `p${n + 1}`));
  const host = await setup(entry);
  action(host, "m2", "Launch again…");
  await vi.waitFor(() =>
    expect(focus(host)).toBe("m2's start card is not on the Cards pages that were read."),
  );
  expect(f.reads).toHaveLength(4);
  expect(f.maximum).toBe(1);
  expect(f.calls).toHaveLength(0);
});

it.each(entries)(
  "manual paging retires the previous lookup result until Refresh restarts it (%s)",
  async (entry) => {
    f.pages.set(null, page(null, [own as unknown as DrawerPage["rows"][number]], "p1"));
    f.pages.set("p1", page("p1", [other as unknown as DrawerPage["rows"][number]]));
    const host = await setup(entry);
    action(host, "m2", "Launch again…");
    await focused(host);
    host.querySelector<HTMLButtonElement>("[data-drawer-next]")!.click();
    expect(focus(host)).toBeUndefined();
    await vi.waitFor(() => expect(host.querySelector(`[data-proposal="${other.seq}"]`)).not.toBeNull());
    expect(focus(host)).toBeUndefined();
    host.querySelector<HTMLButtonElement>("[data-drawer-refresh]")!.click();
    await focused(host);
    expect(f.reads.map((read) => read.after)).toEqual([null, "p1", null]);
  },
);
