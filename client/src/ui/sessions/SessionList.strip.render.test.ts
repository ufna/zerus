// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { Row } from "./model";
import type { Session } from "./types";

// A strip tile carries its session's state in its frame, as Qt's paintStripCard
// (SessionCardDelegate.h:611-613 at 7aeb1a0af), including Mark needs attention.

const fixture = vi.hoisted(() => ({ rows: () => [] as Row[] }));
vi.mock("../../bridge", async (original) => ({ ...await original<object>(), gitRows: vi.fn().mockResolvedValue(undefined) }));
vi.mock("./model", async (original) => ({ ...await original<object>(), listRows: () => fixture.rows() }));
vi.mock("@tanstack/solid-virtual", () => ({
  createVirtualizer: (options: { count: number; getItemKey(n: number): string }) => ({
    measure() {}, _willUpdate() {}, scrollToIndex() {}, getTotalSize: () => options.count * 94,
    getVirtualItems: () => Array.from({ length: options.count }, (_, index) => ({ key: options.getItemKey(index), start: index * 94, index })),
  }),
}));
import { SessionList } from "./SessionList";
import { applyBatch, catalog } from "../../store/catalog";
import { emptyBatch } from "../../types";
import { setPage, setPeek, setStripMode } from "../../store/ui";

const KEY = "\nrun:tile";
let dispose: (() => void) | undefined;
let host!: HTMLElement;
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const tile = (extra: Partial<Session>): Row => ({
  type: "card", id: KEY, session: { ...catalog.rows[KEY], ...extra } as Session, expandable: false, expanded: false, compactRow: false,
});
const frame = async (extra: Partial<Session>) => {
  fixture.rows = () => [tile(extra)];
  dispose = render(() => createComponent(SessionList, {}), host);
  await settle();
  return host.querySelector(".strip-tile")?.getAttribute("data-frame");
};

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  setPage("sessions"); setPeek(false); setStripMode(true);
  applyBatch({ ...emptyBatch(), replaces: [{ computer: "", rows: [{
    computer: "", key: "run:tile", row: { name: "codex/fixture/tile", agent: "codex", status: "ready", state: "running", cwd: "/fixture/tile", run_id: "tile" },
  }] }] });
  host = document.createElement("div"); document.body.append(host);
});
afterEach(() => {
  dispose?.(); dispose = undefined; document.body.replaceChildren();
  setStripMode(false); vi.unstubAllGlobals();
});

describe("collapsed session list tiles", () => {
  it("frames a session marked as needing attention in the attention colour", async () => {
    expect(await frame({ reviewLater: true, unreadReply: false })).toBe("attention");
  });

  it("frames a muted mark grey and an unmarked ready session neutral", async () => {
    expect(await frame({ reviewLater: true, muted: true, unreadReply: false })).toBe("muted");
    dispose?.(); host.replaceChildren();
    expect(await frame({ reviewLater: false, unreadReply: false })).toBe("neutral");
  });
});
