import { afterEach, describe, expect, it, vi } from "vitest";
import { createTeams, HIDDEN_RECHECK_MS, type TeamFollow, type TeamsTransport } from "./teams";
import type { TeamsOut } from "../types";

// Discovery and visibility belong to the application: a positive off or
// unavailable answer hides the entry points, unfollows and stops the reads; a
// hidden window pauses them whether or not a Team tab was ever opened, and a
// window hidden for ten minutes or more asks again when it returns.
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });

function fake() {
  const calls = { follow: [] as TeamFollow[], refresh: [] as boolean[], hidden: [] as boolean[], pages: [] as [string, string | null][] };
  const transport: TeamsTransport = {
    async follow(follow, refresh = false) { calls.follow.push(follow); calls.refresh.push(refresh); },
    async hidden(hidden) { calls.hidden.push(hidden); },
    async tasksChanged() {},
    async page(key, cursor) { calls.pages.push([key, cursor]); },
    async act(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async actAgain(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss() {}, async catalog() { return {}; }, async saveTemplate() {}, async project() { return {}; }, async preview() { return {}; },
  };
  return { transport, calls };
}

afterEach(() => { vi.useRealTimers(); });

describe("discovery and visibility", () => {
  it("an explicit retry asks for a fresh answer even with an unchanged follow", async () => {
    const { transport, calls } = fake();
    const store = createTeams(transport);
    await store.probe();
    await store.recheck();
    expect(calls.follow[1]).toEqual(calls.follow[0]);
    expect(calls.refresh).toEqual([true, true]);
  });

  it("a failed first read never defaults the preview to on and keeps its reason", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "failed", { code: "invalid_reply", message: "team reply gives no actor" })]);
    expect(store.available()).toBe(false);
    expect(store.off()).toBe("unavailable");
    expect(store.readFailureFor("list")?.message).toBe("team reply gives no actor");
  });

  it.each(["invalid_reply", "failed"])("a transient %s list failure keeps the confirmed preview on", (code) => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", { teams: [], truncated: false, next: null })]);
    store.apply([entry("list", "failed", { code, message: "The read did not answer." })]);
    expect(store.available()).toBe(true);
    expect(store.off()).toBeNull();
    expect(store.readFailureFor("list")).toMatchObject({ code, message: "The read did not answer." });
  });

  it("a cursor reset keeps the confirmed preview on", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", { teams: [], truncated: false, next: null })]);
    store.apply([entry("list", "failed", { code: "cursor_reset", message: "Read the first page again." })]);
    expect(store.available()).toBe(true);
    expect(store.off()).toBeNull();
    expect(store.readFailureFor("list")?.code).toBe("cursor_reset");
  });

  it("a real off answer turns a confirmed-on preview off", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", { teams: [], truncated: false, next: null })]);
    store.apply([entry("list", "failed", { code: "teams_inactive", message: "Teams are off." })]);
    expect(store.available()).toBe(false);
    expect(store.off()).toBe("inactive");
  });

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

  it("asking again keeps the published list until a read replaces it", async () => {
    const { transport } = fake();
    const store = createTeams(transport);
    const team = { id: "t", project: "p", name: "n", status: "running", lead: "l", home: "h", revision: 1, lead_session: null };
    store.apply([entry("list", "list", { teams: [team], truncated: false, next: "c1" })]);
    expect(store.listPages()).toHaveLength(1);
    await store.recheck();
    // The core sends an unchanged list only once, so nothing would refill it.
    expect(store.listPages()).toHaveLength(1);
    expect(store.listNext()).toBe("c1");
    store.apply([entry("list", "list", { teams: [{ ...team, name: "renamed" }], truncated: false, next: null })]);
    expect(store.listPages()[0].name).toBe("renamed");
    expect(store.listNext()).toBeNull();
  });
});

describe("the preview switch", () => {
  it("asks the command once with the chosen state and rechecks after success", async () => {
    const { transport, calls } = fake();
    const preview = vi.fn(async () => ({}));
    transport.preview = preview;
    const store = createTeams(transport);
    await store.probe();
    calls.follow.length = 0;
    expect(await store.preview(true)).toEqual({ outcome: "done" });
    expect(preview).toHaveBeenCalledTimes(1);
    expect(preview).toHaveBeenCalledWith(true);
    expect(calls.follow.at(-1)).toEqual({ list: true, waiting: true, team: null });
  });

  it("a refusal keeps its reason and asks discovery nothing", async () => {
    const { transport, calls } = fake();
    transport.preview = async () => { throw "not_allowed: Only a person turns the Teams preview on or off."; };
    const store = createTeams(transport);
    await store.probe();
    calls.follow.length = 0;
    const outcome = await store.preview(false);
    expect(outcome.outcome).toBe("failed");
    expect(outcome).toMatchObject({ detail: expect.stringContaining("Only a person") });
    expect(calls.follow).toHaveLength(0);
  });

  it("an unknown result changes nothing and asks discovery nothing", async () => {
    const { transport, calls } = fake();
    transport.preview = async () => { throw new Error("reply channel disconnected"); };
    const store = createTeams(transport);
    await store.probe();
    calls.follow.length = 0;
    const outcome = await store.preview(true);
    expect(outcome.outcome).toBe("uncertain");
    expect(calls.follow).toHaveLength(0);
  });
});

describe("a storage failure (zerus-63rru.19)", () => {
  it("of the preview switch is an unknown result, never a refusal", async () => {
    const { transport, calls } = fake();
    transport.preview = async () => { throw "storage_failed: The result is not known; repeat the same request with the same request ID."; };
    const store = createTeams(transport);
    await store.probe();
    calls.follow.length = 0;
    const outcome = await store.preview(false);
    expect(outcome).toEqual({ outcome: "uncertain", detail: "storage_failed: The result is not known; repeat the same request with the same request ID." });
    expect(calls.follow).toHaveLength(0);
    transport.preview = async () => { throw "invalid_state: The preview cannot change now."; };
    expect((await store.preview(false)).outcome, "a rule's refusal stays a refusal").toBe("failed");
  });
});
describe("a busy store (the store's codes)", () => {
  it("refuses the preview switch: nothing was written, never an unknown result", async () => {
    const { transport } = fake();
    transport.preview = async () => { throw "store_busy: The team store is busy. Nothing was written; try again."; };
    const store = createTeams(transport);
    await store.probe();
    expect(await store.preview(false)).toEqual({ outcome: "failed", detail: "store_busy: The team store is busy. Nothing was written; try again." });
  });
});
