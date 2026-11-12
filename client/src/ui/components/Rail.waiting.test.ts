// @vitest-environment happy-dom
// The Waiting rail entry: no entry while Teams are off or unknown; with Teams
// on, the entry and its count (capped in the badge).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({ setAvailable: (_: boolean | undefined) => {}, setCount: (_: number) => {} }));
vi.mock("../../store/ui", () => ({ page: () => "sessions", setPage: vi.fn(), counts: () => ({ attention: 0 }), setMenu: vi.fn(), logoMotion: () => "none" }));
vi.mock("../../skin/runtime", () => ({ skin: () => ({ layout: { rail: "icons" } }) }));
vi.mock("../../store/windowLayer", () => ({ windowLayer: () => ({ on: false, supported: true, hint: "" }), setWindowLayer: vi.fn() }));
vi.mock("../sessions/mark-entries", () => ({ markAllEntry: () => ({ label: "Mark all read" }) }));
vi.mock("../sessions/background-menu", () => ({ expandAllCollapsedEntry: () => ({ label: "Expand all" }) }));
vi.mock("../sessions/newsession-view", () => ({ newSessionFromRail: vi.fn() }));
vi.mock("../../store/desk", () => ({ openAbout: vi.fn() }));
vi.mock("../../store/buildInfo", () => ({ appBuildInfo: () => undefined, refreshAppBuildInfo: vi.fn(async () => {}) }));
vi.mock("../../store/shortcuts", () => ({ shortcutTooltip: (text: string) => text }));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [available, setAvailable] = createSignal<boolean | undefined>(undefined);
  const [count, setCount] = createSignal(0);
  f.setAvailable = setAvailable;
  f.setCount = setCount;
  return { teams: { available, waitingCount: count, openNewTeam: vi.fn(async () => {}) } };
});
import { Rail } from "./Rail";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const setup = () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(Rail, {}), host);
  return host;
};
const waiting = (host: HTMLElement) => [...host.querySelectorAll("button")].find((button) => button.getAttribute("aria-label")?.startsWith("Waiting for you"));

it("shows no Waiting entry while Teams are off or unknown", () => {
  const host = setup();
  expect(waiting(host)).toBeUndefined();
  f.setAvailable(false);
  expect(waiting(host)).toBeUndefined();
});

it("shows the entry with the exact count and caps only the badge", () => {
  const host = setup();
  f.setAvailable(true);
  f.setCount(3);
  expect(waiting(host)?.getAttribute("aria-label")).toBe("Waiting for you — 3");
  expect(host.querySelector(".rail-badge")?.textContent).toBe("3");
  f.setCount(120);
  expect(waiting(host)?.getAttribute("aria-label")).toBe("Waiting for you — 120");
  expect(host.querySelector(".rail-badge")?.textContent).toBe("99+");
  f.setCount(0);
  expect(waiting(host)?.getAttribute("aria-label")).toBe("Waiting for you");
  expect(host.querySelector(".rail-badge")).toBeNull();
});
