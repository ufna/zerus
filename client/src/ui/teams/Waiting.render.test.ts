// @vitest-environment happy-dom
// Waiting for you (the fork's Waiting.tsx, adapted to M1): the owner cards,
// oldest first, each with its team, its kind, its age and its decisions, and
// the next page through its cursor.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { WaitingList } from "./Waiting";
import type { WaitingItem, WaitingView } from "../../store/teams";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

const TEAM_A = "00000000-0000-4000-8000-00000000aaaa";
const TEAM_B = "00000000-0000-4000-8000-00000000bbbb";
const NOW = 1_700_000_000_000;
const item = (over: Partial<WaitingItem>): WaitingItem => ({
  id: "x:1", team: TEAM_A, card: 1, kind: "note", node: null, text: "", state: "open", by: "local", created_ms: NOW - 300_000, ...over,
});
const view: WaitingView = {
  items: [
    item({ id: `${TEAM_A}:1`, card: 1, kind: "note", text: "Tell the team the release moved to Monday." }),
    item({ id: `${TEAM_B}:2`, card: 2, team: TEAM_B, kind: "integration", text: "Accept the merge of Task 2?", created_ms: NOW - 7_200_000 }),
  ],
  next: "w1.next",
};
const names = new Map([[TEAM_A, "Fixes"], [TEAM_B, "Docs"]]);

function setup(over: { view?: WaitingView; busy?: boolean } = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const decided: [string, string][] = [];
  const pages: string[] = [];
  dispose = render(() => createComponent(WaitingList, {
    view: over.view ?? view, teamNames: names, now: NOW, busy: over.busy ?? false,
    onDecide: (entry, decision) => decided.push([entry.id, decision]),
    onOpenTeam: vi.fn(), onPage: (cursor) => pages.push(cursor),
  }), host);
  return { host, decided, pages };
}

it("lists the cards oldest first with their team, kind, text and age", () => {
  const { host } = setup();
  const cards = [...host.querySelectorAll("[data-waiting]")];
  expect(cards.map((card) => card.getAttribute("data-kind"))).toEqual(["note", "integration"]);
  expect(cards[0].textContent).toContain("Your word");
  expect(cards[0].textContent).toContain("Fixes");
  expect(cards[0].textContent).toContain("Tell the team the release moved to Monday.");
  expect(cards[0].textContent).toContain("5 m");
  expect(cards[1].textContent).toContain("Integration");
  expect(cards[1].textContent).toContain("Docs");
  expect(cards[1].textContent).toContain("2 h");
});

it("offers the decisions of each kind", () => {
  const { host } = setup();
  const note = [...host.querySelectorAll(`[data-waiting="${TEAM_A}:1"] button[data-decision]`)].map((button) => button.getAttribute("data-decision"));
  expect(note).toEqual(["done"]);
  const integration = [...host.querySelectorAll(`[data-waiting="${TEAM_B}:2"] button[data-decision]`)].map((button) => button.getAttribute("data-decision"));
  expect(integration).toEqual(["approve", "reject"]);
});

it("sends the decision from the card", () => {
  const { host, decided } = setup();
  (host.querySelector(`[data-waiting="${TEAM_A}:1"] [data-decision="done"]`) as HTMLButtonElement).click();
  (host.querySelector(`[data-waiting="${TEAM_B}:2"] [data-decision="reject"]`) as HTMLButtonElement).click();
  expect(decided).toEqual([[`${TEAM_A}:1`, "done"], [`${TEAM_B}:2`, "reject"]]);
});

it("disables the decisions while one is sent", () => {
  const { host } = setup({ busy: true });
  expect((host.querySelector("[data-decision]") as HTMLButtonElement).disabled).toBe(true);
});

it("goes to the next page through its cursor", () => {
  const { host, pages } = setup();
  const more = [...host.querySelectorAll("button")].find((button) => button.textContent?.startsWith("More")) as HTMLButtonElement;
  more.click();
  expect(pages).toEqual(["w1.next"]);
});

it("says nothing waits for you when the page is empty", () => {
  const { host } = setup({ view: { items: [], next: null } });
  expect(host.textContent).toContain("Nothing waits for you.");
});
