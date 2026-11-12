// @vitest-environment happy-dom
// The Team tab (the fork's TeamWorkspace, adapted to M1) from the pinned view:
// the bar and its counts, the member cards, the Board with the queue as its own
// group, a card's stages (assignee and implementer apart), its decisions, the
// next page and the empty queue. No store, no hgs.
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamWorkspace } from "./TeamTab";
import { teamView } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup(overrides: Partial<Parameters<typeof TeamWorkspace>[0]> = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const actions: [string, Record<string, unknown> | undefined][] = [];
  const pages: (string | null)[] = [];
  dispose = render(() => createComponent(TeamWorkspace, {
    view: teamView, busy: false, failure: null,
    onAction: (kind, fields) => actions.push([kind, fields]), onPage: (cursor) => pages.push(cursor), onAddTask: () => {}, ...overrides,
  }), host);
  return { host, actions, pages };
}

it("shows the team bar with the counts the core sent", () => {
  const { host } = setup();
  expect(host.querySelector("[data-team-bar] .team-name")?.textContent).toBe("Fixes");
  expect(host.querySelector("[data-team-bar] .team-count")?.textContent).toBe("0/2");
  expect(host.querySelector("[data-team-bar]")?.textContent).toContain("2 active");
});

it("shows the member cards, the lead marked", () => {
  const { host } = setup();
  const members = host.querySelectorAll("[data-team-members] .team-member-card");
  expect(members).toHaveLength(4);
  expect(members[0].classList.contains("team-lead")).toBe(true);
  expect(members[2].textContent).toContain("not created");
});

it("draws the queue's own board groups and the cards", () => {
  const { host } = setup();
  const groups = [...host.querySelectorAll("[data-team-board] .team-column")].map((group) => group.getAttribute("data-group"));
  expect(groups).toEqual(["queue", "work", "submitted", "role:reviewer", "accepted"]);
  const reported = host.querySelector('[data-group="submitted"]');
  expect(reported?.querySelector(".team-task-title")?.textContent).toBe("Task 2");
});

it("shows a stage's own worker apart from the task's implementer", () => {
  const { host } = setup();
  const stage = host.querySelector('[data-group="submitted"] .team-stage-chip');
  expect(stage?.getAttribute("title")).toContain("reviewer: m3, checks the work of m1");
});

it("a selected card shows its facts and decisions, sending by node", () => {
  const { host, actions } = setup();
  (host.querySelector('[data-group="submitted"] .team-task-card') as HTMLButtonElement).click();
  const detail = host.querySelector("[data-team-detail]")?.textContent ?? "";
  expect(detail).toContain("example-project");
  expect(detail).toContain("00000000-0000-4000-8000-000000000002");
  expect(detail).toContain("m1");
  (host.querySelector('[data-task-action="accept"]') as HTMLButtonElement).click();
  expect(actions.at(-1)).toEqual(["accept", { node: 2 }]);
});

it("a task with no action shows neither the actions' divider nor an empty row", () => {
  const cancelled = { ...teamView.groups[2].cards[0], node: 9, title: "Cancelled task", state: "cancelled", decided_by: "person" };
  const view = { ...teamView, groups: teamView.groups.map((group) => group.state === "cancelled" ? { ...group, cards: [cancelled] } : group) };
  const { host } = setup({ view });
  (host.querySelector('.team-task-card[data-node="9"]') as HTMLButtonElement).click();
  expect(host.querySelector("[data-team-detail]")?.textContent).toContain("Cancelled task");
  expect(host.querySelector("[data-team-detail] [data-task-actions]")).toBeNull();
  // A task with actions keeps its row.
  (host.querySelector('.team-task-card[data-node="1"]') as HTMLButtonElement).click();
  expect(host.querySelector("[data-team-detail] [data-task-actions]")?.children.length).toBeGreaterThan(0);
});

it("asks for a reason before a return", () => {
  const { host, actions } = setup();
  (host.querySelector('[data-group="submitted"] .team-task-card') as HTMLButtonElement).click();
  (host.querySelector('[data-task-action="return"]') as HTMLButtonElement).click();
  const input = host.querySelector(".team-reason input") as HTMLInputElement;
  input.value = "Add a test"; input.dispatchEvent(new Event("input", { bubbles: true }));
  const send = [...host.querySelectorAll(".team-reason button")].find((button) => button.textContent === "Send") as HTMLButtonElement;
  expect(send.disabled).toBe(false);
  send.click();
  expect(actions.at(-1)).toEqual(["return", { node: 2, reason: "Add a test" }]);
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
