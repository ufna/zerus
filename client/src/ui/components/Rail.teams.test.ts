// @vitest-environment happy-dom
// The Teams entry points: no rail button and no command dialog while Teams are
// off or hgs cannot run; the button appears once the core says Teams exist.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({ setAvailable: (_: boolean | undefined) => {} }));
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
  const { createSignal: signal } = await import("solid-js");
  const [available, setAvailable] = signal<boolean | undefined>(undefined);
  const openNewTeam = vi.fn(async () => {});
  f.setAvailable = setAvailable;
  return { teams: { available, openNewTeam } };
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
const newTeam = (host: HTMLElement) =>
  [...host.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "New team");

it("shows no New team entry while Teams are off or not yet known", () => {
  const host = setup();
  expect(newTeam(host)).toBeUndefined();
  f.setAvailable(false);
  expect(newTeam(host)).toBeUndefined();
});

it("shows the New team entry once the core says Teams exist", () => {
  const host = setup();
  f.setAvailable(true);
  expect(newTeam(host)).toBeDefined();
});
