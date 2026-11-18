// @vitest-environment happy-dom
// Waiting for you shows each card's age from the application's own clock:
// the real Teams store holds the page and the real UI store's clock ticks in
// seconds, while a card's creation time is in milliseconds (zerus-rzvct G4).
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import list from "../../../../crates/team-plan/fixtures/cli-list.json";
import type { TeamsOut } from "../../types";

const f = vi.hoisted(() => ({ calls: [] as string[] }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string) => { f.calls.push(command); return undefined; }, isTauri: () => false }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => [] }));
let store: typeof import("../../store/teams").teams;
let ui: typeof import("../../store/ui");
let WaitingScreen: typeof import("./Waiting").WaitingScreen;
const disposers: (() => void)[] = [];
const TEAM = list.rows[0].id;
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });
const card = (seq: number, ageMs: number) => ({
  id: `${TEAM}:${seq}`, team: TEAM, card: seq, kind: "note", node: null, text: `Card ${seq}`, state: "open", by: "local",
  created_ms: Date.now() - ageMs, decisions: ["acknowledged"],
});

beforeEach(async () => {
  vi.resetModules(); f.calls = [];
  ({ teams: store } = await import("../../store/teams")); ui = await import("../../store/ui");
  ({ WaitingScreen } = await import("./Waiting"));
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("ages each card by the real clock, not as now", () => {
  store.apply([entry("list", "list", { teams: [{ ...list.rows[0], lead_session: list.rows[0].lead_session }], truncated: false, next: null })]);
  store.apply([entry("waiting", "waiting", { items: [card(1, 5 * 60_000 + 5_000), card(2, 2 * 3_600_000 + 60_000), card(3, 3 * 86_400_000)], next: null })]);
  ui.setNow(Date.now() / 1000);
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => createComponent(WaitingScreen, {}), host));
  const ages = Object.fromEntries([...host.querySelectorAll("[data-waiting]")].map((item) => [item.textContent!.match(/Card \d/)![0], item.querySelector(".waiting-age")?.textContent]));
  expect(ages).toEqual({ "Card 1": "5 m", "Card 2": "2 h", "Card 3": "3 d" });
});
