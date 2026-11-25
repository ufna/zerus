// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

// Hover expand of the collapsed session list follows Qt's SessionPanelDock
// (SessionPanelDock.h:47,68,117 at 7aeb1a0af): only with
// workspace/expandSessionsOnHover on, and turning it off stops a pending open.

vi.mock("../../bridge", async (original) => ({ ...await original<object>(), gitRows: vi.fn().mockResolvedValue(undefined) }));
vi.mock("./model", async (original) => ({ ...await original<object>(), listRows: () => [] }));
vi.mock("@tanstack/solid-virtual", () => ({
  createVirtualizer: () => ({ measure() {}, _willUpdate() {}, scrollToIndex() {}, getTotalSize: () => 0, getVirtualItems: () => [] }),
}));
import { SessionList } from "./SessionList";
import { setSetting } from "../../store/settings";
import { peek, setPage, setPeek, setStripMode } from "../../store/ui";

const KEY = "workspace/expandSessionsOnHover";
let dispose: (() => void) | undefined;
let host!: HTMLElement;
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const enterStrip = () => host.querySelector(".strip-col")!.dispatchEvent(new Event("pointerenter"));

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  setPage("sessions"); setPeek(false); setStripMode(true);
  host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(SessionList, {}), host);
  await settle();
});
afterEach(async () => {
  dispose?.(); dispose = undefined; document.body.replaceChildren();
  setPeek(false); setStripMode(false);
  await setSetting(KEY, null);
  vi.useRealTimers(); vi.unstubAllGlobals();
});

describe("collapsed session list hover", () => {
  it("opens after resting on the strip when the setting is on", async () => {
    await setSetting(KEY, true);
    enterStrip();
    vi.advanceTimersByTime(200);
    expect(peek()).toBe(true);
  });

  it("stays collapsed on hover when the setting is off", async () => {
    await setSetting(KEY, false);
    enterStrip();
    vi.advanceTimersByTime(1000);
    expect(peek()).toBe(false);
  });

  it("does not open when the setting turns off during the delay", async () => {
    await setSetting(KEY, true);
    enterStrip();
    vi.advanceTimersByTime(100);
    await setSetting(KEY, false);
    vi.advanceTimersByTime(1000);
    expect(peek()).toBe(false);
  });
});
