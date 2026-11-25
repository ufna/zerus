// @vitest-environment happy-dom
// Review of #2558 (P2, Launch again): the member's button leads to that
// member's own start card, found by a bounded lookup through the Cards pages;
// its Retry is focused, and a card without Retry or no card is explained in one
// sentence. The first two tests are the reviewer's probes
// (LaunchAgain.independent2558), rebuilt from their recorded reads. The rows
// are the product's launch_blocked card (core waiting-decisions.json) with the
// writer's member parameter and decisions (src/desktop/teams/launch/card.rs).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import waiting from "../../../core/src/teams/fixtures/waiting-decisions.json";
import { teamView, TEAM_ID } from "./fixtures";
import type { DrawerPage, DrawerRead, DrawerRow } from "../../store/teamDrawers";
import type { WaitingItem } from "../../store/teams";

const f = vi.hoisted(() => ({ pages: new Map<string | null, unknown>(), reads: [] as unknown[] }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: async (command: string, args?: { read: DrawerRead }) => {
    if (command !== "teams_drawer_page") return undefined;
    f.reads.push(args!.read);
    return f.pages.get(args!.read.after) ?? { team: args!.read.team, kind: "cards", rows: [], after: args!.read.after, next: null };
  },
}));
const { TeamWorkspace } = await import("./TeamTab");
const { MenuLayer } = await import("../components/Overlays");

const launch = (waiting.items as WaitingItem[]).find((item) => item.kind === "launch_blocked")!;
function card(seq: number, member: string, decisions: string[], kind = "launch_blocked"): DrawerRow {
  const item: WaitingItem = { ...launch, id: `${TEAM_ID}:${seq}`, card: seq, kind, decisions, text: `Member ${member}: worktree_unavailable.`,
    parameters: [["member", member], ["code", "worktree_unavailable"]] };
  return { seq, kind, by: "", by_kind: "service", text: item.text, text_truncated: false, at_ms: 1, node: null, generation: null, state: "open", reply: null, notes: [], card: item } as DrawerRow;
}
const page = (after: string | null, rows: DrawerRow[], next: string | null): DrawerPage => ({ team: TEAM_ID, kind: "cards", rows, after, next });

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); f.pages.clear(); f.reads = []; });
function launchAgain(member = "m2") {
  const action = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => [createComponent(TeamWorkspace, { view: teamView, busy: false, failure: null, onAction: action, onPage: vi.fn(), onAddTask: vi.fn() }), createComponent(MenuLayer, {})], host);
  host.querySelector<HTMLButtonElement>("[data-team-members-toggle]")!.click();
  host.querySelector<HTMLButtonElement>(`[data-member="${member}"] [aria-label="Member actions"]`)!.click();
  [...document.querySelectorAll<HTMLButtonElement>(".menu-item")].find((button) => button.textContent === "Launch again…")!.click();
  return { host, action };
}

it("finds the selected member's start card beyond the first page and focuses its Retry", async () => {
  f.pages.set(null, page(null, [card(11, "m3", ["retry", "acknowledged"])], "next-cards-page"));
  f.pages.set("next-cards-page", page("next-cards-page", [card(22, "m2", ["retry", "acknowledged"])], null));
  const { host, action } = launchAgain();
  await vi.waitFor(() => expect(f.reads).toHaveLength(2));
  expect(f.reads.map((read) => (read as DrawerRead).after)).toEqual([null, "next-cards-page"]);
  const retry = await vi.waitFor(() => {
    const button = host.querySelector<HTMLButtonElement>('[data-proposal="22"] [data-card-decision="retry"]');
    expect(button).not.toBeNull(); return button!;
  });
  await vi.waitFor(() => expect(document.activeElement).toBe(retry));
  expect(host.querySelector('[data-proposal="22"]')!.classList.contains("team-proposal-focus")).toBe(true);
  expect(host.querySelector("[data-launch-focus]")?.textContent).toBe("Retry on m2's start card starts it again.");
  retry.click();
  expect(action).toHaveBeenCalledWith("decide", { card: 22, decision: "retry" });
});

it("says in one sentence that a never-started member has no start card", async () => {
  f.pages.set(null, page(null, [], null));
  const { host } = launchAgain();
  await vi.waitFor(() => expect(host.querySelector("[data-launch-focus]")?.textContent)
    .toBe("m2 has no open start card. Zerus starts the member when there is room."));
});

it("handwritten a start card without Retry says so instead of focusing anything", async () => {
  f.pages.set(null, page(null, [card(11, "m3", ["retry", "acknowledged"]), card(12, "m2", ["acknowledged"], "launch_uncertain")], null));
  const { host } = launchAgain();
  await vi.waitFor(() => expect(host.querySelector("[data-launch-focus]")?.textContent)
    .toBe("m2's start card offers no Retry now; the card says why."));
  expect(host.querySelector('[data-proposal="12"]')!.classList.contains("team-proposal-focus")).toBe(true);
});

it("handwritten an older card that names its member only in its text is still found", async () => {
  const older = card(31, "m2", ["retry", "acknowledged"]);
  delete (older.card as WaitingItem).parameters;
  f.pages.set(null, page(null, [older], null));
  const { host } = launchAgain();
  await vi.waitFor(() => expect(host.querySelector("[data-launch-focus]")?.textContent).toBe("Retry on m2's start card starts it again."));
});

it("handwritten the lookup is bounded and says when the card is beyond the pages it read", async () => {
  for (let n = 0; n < 6; n++) f.pages.set(n ? `p${n}` : null, page(n ? `p${n}` : null, [card(100 + n, "m3", ["retry", "acknowledged"])], `p${n + 1}`));
  const { host } = launchAgain();
  await vi.waitFor(() => expect(host.querySelector("[data-launch-focus]")?.textContent).toBe("m2's start card is not on the Cards pages that were read."));
  expect(f.reads).toHaveLength(4);
});
