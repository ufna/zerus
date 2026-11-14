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
    item({ id: `${TEAM_B}:2`, card: 2, team: TEAM_B, kind: "integration", text: "Accept the merge of Task 2?", created_ms: NOW - 7_200_000, decisions: ["approve", "reject"] }),
    item({ id: `${TEAM_A}:3`, card: 3, kind: "pipeline_acceptance", text: "Accept the pipeline's work on Task 3?", node: 3, created_ms: NOW - 3_600_000, decisions: ["approve", "reject"] }),
    item({ id: `${TEAM_A}:4`, card: 4, kind: "task_waits_owner", text: "Task 1 is reported; its work is decided on the team's board.", node: 1, created_ms: NOW - 60_000 }),
  ],
  next: "w1.next",
};
const names = new Map([[TEAM_A, "Fixes"], [TEAM_B, "Docs"]]);

function setup(over: { view?: WaitingView; busy?: boolean } = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const decided: [string, string][] = [];
  const pages: string[] = [];
  const opened: string[] = [];
  dispose = render(() => createComponent(WaitingList, {
    view: over.view ?? view, teamNames: names, now: NOW, busy: over.busy ?? false,
    onDecide: (entry, decision) => decided.push([entry.id, decision]),
    onOpenTeam: vi.fn(), onOpenTask: (entry) => opened.push(entry.id), onPage: (cursor) => pages.push(cursor),
  }), host);
  return { host, decided, pages, opened };
}

it("lists the cards oldest first with their team, kind, text and age", () => {
  const { host } = setup();
  const cards = [...host.querySelectorAll("[data-waiting]")];
  expect(cards.map((card) => card.getAttribute("data-kind"))).toEqual(["note", "integration", "pipeline_acceptance", "task_waits_owner"]);
  expect(cards[0].textContent).toContain("Note");
  expect(cards[3].textContent, "a kind the store does not open shows its plain kind").toContain("Card: task_waits_owner");
  expect(cards[0].textContent).toContain("Fixes");
  expect(cards[0].textContent).toContain("Tell the team the release moved to Monday.");
  expect(cards[0].textContent).toContain("5 m");
  expect(cards[1].textContent).toContain("Integration");
  expect(cards[1].textContent).toContain("Docs");
  expect(cards[1].textContent).toContain("2 h");
});

it("offers each card's own list: the store's pairs, and acknowledged where a card lists it or lists nothing", () => {
  const { host } = setup();
  const words = (id: string) => [...host.querySelectorAll(`[data-waiting="${id}"] button[data-decision]`)].map((button) => button.getAttribute("data-decision"));
  expect(words(`${TEAM_A}:1`)).toEqual(["acknowledged"]);
  expect(words(`${TEAM_B}:2`)).toEqual(["approve", "reject"]);
  expect(words(`${TEAM_A}:3`)).toEqual(["approve", "reject"]);
  expect(words(`${TEAM_A}:4`)).toEqual(["acknowledged"]);
  expect(host.querySelector(`[data-waiting="${TEAM_A}:4"] [data-decision="acknowledged"]`)?.textContent).toBe("Acknowledge");
});

it("marks only a card's first decision as the primary one, as the client's other notices do", () => {
  // The visual pass: Retry and Acknowledge drew as two primary buttons side
  // by side; Check and Dismiss of an uncertain action draw one primary.
  const retry = item({ id: `${TEAM_A}:5`, card: 5, kind: "team_actor_blocked", text: "The service could not finish a step.", decisions: ["retry", "acknowledged"] });
  const { host } = setup({ view: { items: [...view.items, retry], next: null } });
  const looks = (id: string) => [...host.querySelectorAll<HTMLButtonElement>(`[data-waiting="${id}"] button[data-decision]`)]
    .map((button) => [button.getAttribute("data-decision"), button.classList.contains("primary"), button.classList.contains("danger")]);
  expect(looks(`${TEAM_A}:5`)).toEqual([["retry", true, false], ["acknowledged", false, false]]);
  expect(looks(`${TEAM_B}:2`)).toEqual([["approve", true, false], ["reject", false, true]]);
  expect(looks(`${TEAM_A}:1`)).toEqual([["acknowledged", true, false]]);
});

it("gives a card that names a node an Open task action, and no other card", () => {
  const { host, opened } = setup();
  const open = (id: string) => host.querySelector(`[data-waiting="${id}"] [data-open-task]`);
  expect(open(`${TEAM_A}:1`)).toBeNull();
  expect(open(`${TEAM_B}:2`)).toBeNull();
  expect(open(`${TEAM_A}:3`)).not.toBeNull();
  expect(open(`${TEAM_A}:4`)).not.toBeNull();
  (open(`${TEAM_A}:4`) as HTMLButtonElement).click();
  expect(opened).toEqual([`${TEAM_A}:4`]);
});

it("sends the decision from the card", () => {
  const { host, decided } = setup();
  (host.querySelector(`[data-waiting="${TEAM_A}:1"] [data-decision="acknowledged"]`) as HTMLButtonElement).click();
  (host.querySelector(`[data-waiting="${TEAM_B}:2"] [data-decision="reject"]`) as HTMLButtonElement).click();
  expect(decided).toEqual([[`${TEAM_A}:1`, "acknowledged"], [`${TEAM_B}:2`, "reject"]]);
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

it("says nothing waits for you when the page is empty, with the Team tab's empty state", () => {
  const { host } = setup({ view: { items: [], next: null } });
  const empty = host.querySelector(".empty.team-empty")!;
  expect(empty.querySelector("svg")).not.toBeNull();
  expect(empty.querySelector("h3")?.textContent).toBe("Nothing waits for you.");
  expect(empty.querySelector("p")?.textContent).toBe("Decisions that only you can make appear here.");
});

it("describes a rejected result that is still merged, with the store's facts as its body", () => {
  const body = "Task 4 (task example-project/00000000-0000-4000-8000-000000000004, node 4) was merged as 1123456789abcdef0123456789abcdef01234567 into integration/example before this integration was rejected. The rejection did not undo the merge; only a revert on integration/example removes it.";
  const rejected = item({ id: `${TEAM_A}:6`, card: 6, kind: "rejected_merge", node: 4, text: body, decisions: ["acknowledged"] });
  const { host } = setup({ view: { items: [rejected], next: null } });
  const card = host.querySelector(`[data-waiting="${TEAM_A}:6"]`);
  expect(card?.textContent).toContain("Rejected result is merged");
  expect(card?.textContent).toContain("The rejection did not undo the merge; only a revert on the integration branch removes it.");
  expect(card?.textContent).toContain(body);
  expect(card?.textContent).not.toContain("Card: rejected_merge");
  expect(card?.querySelector('[data-decision="acknowledged"]')?.textContent).toBe("Acknowledge");
});
