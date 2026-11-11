import { afterEach, describe, expect, it, vi } from "vitest";
import { createTeams, HIDDEN_RECHECK_MS, type TeamFollow, type TeamsTransport } from "./teams";
import type { TeamsOut } from "../types";

// Discovery and visibility belong to the application: a positive off or
// unavailable answer hides the entry points, unfollows and stops the reads; a
// hidden window pauses them whether or not a Team tab was ever opened, and a
// window hidden for ten minutes or more asks again when it returns.
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });

function fake() {
  const calls = { follow: [] as TeamFollow[], hidden: [] as boolean[], pages: [] as [string, string | null][] };
  const transport: TeamsTransport = {
    async follow(follow) { calls.follow.push(follow); },
    async hidden(hidden) { calls.hidden.push(hidden); },
    async tasksChanged() {},
    async page(key, cursor) { calls.pages.push([key, cursor]); },
    async act(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async actAgain(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss() {}, async catalog() { return {}; }, async saveTemplate() {}, async project() { return {}; },
  };
  return { transport, calls };
}

afterEach(() => { vi.useRealTimers(); });

describe("discovery and visibility", () => {
  it("a positive off answer unfollows, stops the reads and hides the entry points", async () => {
    const { transport, calls } = fake();
    const store = createTeams(transport);
    await store.probe();
    store.apply([entry("list", "failed", { code: "teams_inactive", message: "off" })]);
    expect(store.available()).toBe(false);
    expect(store.off()).toBe("inactive");
    expect(calls.follow.at(-1)).toEqual({ list: false, waiting: false, team: null });
    expect(calls.hidden.at(-1)).toBe(true);
  });

  it("a team read that answers unavailable stops it too", async () => {
    const { transport, calls } = fake();
    const store = createTeams(transport);
    store.apply([entry("team:00000000-0000-4000-8000-000000000001", "failed", { code: "unavailable", message: "no hgs" })]);
    expect(store.available()).toBe(false);
    expect(store.off()).toBe("unavailable");
    expect(calls.follow.at(-1)).toEqual({ list: false, waiting: false, team: null });
    expect(calls.hidden.at(-1)).toBe(true);
  });

  it("a hidden window pauses the reads and a quick return does not ask again", async () => {
    const { transport, calls } = fake();
    const store = createTeams(transport);
    await store.probe();
    calls.follow.length = 0;
    await store.hidden(true);
    expect(calls.hidden.at(-1)).toBe(true);
    await store.hidden(false);
    expect(calls.hidden.at(-1)).toBe(false);
    expect(calls.follow, "no re-ask within ten minutes").toHaveLength(0);
  });

  it("a window hidden ten minutes or more asks again when it returns", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const { transport, calls } = fake();
    const store = createTeams(transport);
    await store.probe();
    await store.hidden(true);
    vi.setSystemTime(HIDDEN_RECHECK_MS);
    calls.follow.length = 0;
    await store.hidden(false);
    expect(calls.follow.at(-1)).toEqual({ list: true, waiting: true, team: null });
  });

  it("asking again starts from a fresh list", async () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", { teams: [{ id: "t", project: "p", name: "n", status: "running", lead: "l", home: "h", revision: 1, lead_session: null }], truncated: false, next: "c1" })]);
    expect(store.listPages()).toHaveLength(1);
    await store.recheck();
    expect(store.listPages()).toHaveLength(0);
    expect(store.listNext()).toBeNull();
  });
});
