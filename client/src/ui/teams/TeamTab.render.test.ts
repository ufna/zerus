// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

// The Team tab from the pinned view: the bar's counts, the member cards, the
// Board with the queue as its own group, a card's stages (assignee and
// implementer apart) and the later page. No store, no hgs.
vi.mock("../../store/ui", () => ({
  detailTab: () => "team", page: () => "sessions", selected: () => undefined, setPage: vi.fn(),
}));
vi.mock("../../store/visibility", () => ({ pageHidden: () => false }));
vi.mock("../../store/teams", () => ({
  teams: {
    team: () => undefined, available: () => true, busy: () => false, failure: () => null,
    uncertain: () => null, confirmation: () => null, sessionTeams: () => ({}), list: () => undefined,
    probe: vi.fn(async () => {}), leadTeamFor: vi.fn(async () => null), hidden: vi.fn(async () => {}),
    closeTeam: vi.fn(async () => {}), page: vi.fn(async () => {}), run: vi.fn(async () => ({ outcome: "done" })),
    check: vi.fn(), dismiss: vi.fn(), confirm: vi.fn(), cancelConfirmation: vi.fn(),
  },
  newRequestId: () => "11111111-2222-4333-8444-555555555555",
}));
import { TeamPanel } from "./TeamTab";
import { teamView } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup(overrides: Partial<Parameters<typeof TeamPanel>[0]> = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const actions: [string, string, Record<string, unknown> | undefined][] = [];
  const pages: string[] = [];
  dispose = render(() => createComponent(TeamPanel, {
    view: teamView, busy: false, failure: null, onAction: (card, kind, fields) => actions.push([card.title, kind, fields]),
    onPage: (cursor) => pages.push(cursor), onAddTask: () => {}, ...overrides,
  }), host);
  return { host, actions, pages };
}

it("shows the team bar with the counts the core sent", () => {
  const { host } = setup();
  expect(host.querySelector(".tm-bar h2")?.textContent).toBe("Fixes");
  expect(host.querySelector(".tm-bar .tm-counts")?.textContent).toContain("2active");
  expect(host.querySelector(".tm-bar .tm-counts")?.textContent).toContain("1inbox");
});

it("shows the member cards, the lead marked", () => {
  const { host } = setup();
  const members = host.querySelectorAll("[data-team-members] .tm-member");
  expect(members).toHaveLength(4);
  expect(members[0].classList.contains("tm-lead")).toBe(true);
  expect(members[0].textContent).toContain("lead");
  expect(members[2].textContent).toContain("not_created");
});

it("draws the queue's own board groups and the card", () => {
  const { host } = setup();
  const groups = [...host.querySelectorAll("[data-team-board] .tm-group")].map((group) => group.getAttribute("data-group"));
  expect(groups).toEqual(["waiting", "pending", "in_progress", "reported", "accepted", "cancelled"]);
  const reported = host.querySelector('[data-group="reported"]');
  expect(reported?.querySelector(".tm-title")?.textContent).toBe("Task 2");
});

it("shows a stage's own worker apart from the task's implementer", () => {
  const { host } = setup();
  const stage = host.querySelector('[data-group="reported"] .tm-stage');
  expect(stage?.textContent).toContain("reviewer: m3, checks the work of m1");
});

it("a selected card shows its task, role and stages and offers its decisions", () => {
  const { host, actions } = setup();
  const card = host.querySelector('[data-group="reported"] .tm-card') as HTMLElement;
  card.click();
  const detail = host.querySelector("[data-team-detail]")?.textContent ?? "";
  expect(detail).toContain("example-project");
  expect(detail).toContain("00000000-0000-4000-8000-000000000002");
  const accept = [...host.querySelectorAll(".tm-card-actions button")].find((button) => button.textContent === "Accept") as HTMLButtonElement;
  accept.click();
  expect(actions.at(-1)).toEqual(["Task 2", "accept", { node: 2 }]);
});

it("offers the next page through its cursor and says when cards are missing", () => {
  const { host, pages } = setup({ view: { ...teamView, next: "t1.next", truncated: true } });
  expect(host.textContent).toContain("Some cards are not shown.");
  const more = [...host.querySelectorAll("button")].find((button) => button.textContent?.startsWith("More")) as HTMLButtonElement;
  more.click();
  expect(pages).toEqual(["t1.next"]);
});

it("says the queue is empty without inventing cards", () => {
  const empty = { ...teamView, groups: teamView.groups.map((group) => ({ ...group, cards: [] })) };
  const { host } = setup({ view: empty });
  expect(host.textContent).toContain("The queue is empty.");
});

it("shows a read failure, including a cursor reset's own words", () => {
  const { host } = setup({ failure: { code: "cursor_reset", message: "The page could not be read; the first page will be shown." } });
  expect(host.querySelector("[data-team-failure]")?.textContent).toContain("first page");
});
