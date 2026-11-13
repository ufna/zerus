// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { TeamsTransport } from "../../store/teams";

const f = vi.hoisted(() => ({
  on: false,
  reads: vi.fn(),
  store: undefined as unknown as ReturnType<typeof import("../../store/teams").createTeams>,
}));
vi.mock("../../store/teams", async (original) => {
  const actual = await original<typeof import("../../store/teams")>();
  const transport: TeamsTransport = {
    async follow(scope) {
      f.reads(scope);
      if (scope.list) f.store.apply([{ computer: "", key: "list", kind: f.on ? "list" : "failed", payload: f.on
        ? { teams: [], next: null, truncated: false }
        : { code: "teams_inactive", message: "off" } }]);
    },
    async hidden() {}, async tasksChanged() {}, async page() {},
    async act(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async actAgain(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss() {}, async catalog() { return {}; }, async saveTemplate() {},
    async project() { return {}; }, async preview() { throw Error("a Settings read must never change the preview"); },
  };
  f.store = actual.createTeams(transport);
  return { ...actual, teams: f.store };
});
vi.mock("./settings/sections", async () => {
  const { createSignal } = await import("solid-js");
  const { TeamsSection } = await import("./settings/Teams");
  const [section, setSection] = createSignal("appearance");
  return { section, setSection, available: () => [
    { id: "appearance", label: "Appearance", icon: "palette", component: () => null },
    { id: "teams", label: "Agent Teams", icon: "users", component: TeamsSection },
  ] };
});
import { SettingsScreen } from "./Settings";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); vi.useRealTimers(); });
const settle = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

it("reads an externally changed preview once on each Settings opening, including another section, without polling", async () => {
  vi.useFakeTimers();
  await f.store.probe();
  expect(f.store.off()).toBe("inactive");
  f.reads.mockClear();
  f.on = true; // Another CLI changes the flag while Settings is closed.
  const host = document.createElement("div");
  document.body.append(host);
  const open = () => { dispose = render(() => createComponent(SettingsScreen, {}), host); };
  const reads = () => f.reads.mock.calls.filter(([scope]) => scope.list).length;
  open();
  await settle();
  expect(f.store.available()).toBe(true);
  expect(reads()).toBe(1);
  (host.querySelector("#set-nav-teams") as HTMLButtonElement).click();
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true");
  await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
  expect(reads()).toBe(1);
  dispose!();
  f.on = false;
  open();
  await settle();
  expect(f.store.off()).toBe("inactive");
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("false");
  expect(reads()).toBe(2);
});
