import { describe, expect, it } from "vitest";
import { createEffect, createRoot } from "solid-js";
import { createTeams, type TeamOutcome, type TeamsTransport } from "./teams";
import { teamView, TEAM_ID } from "../ui/teams/fixtures";
import type { TeamsOut } from "../types";

// The reactive lead binding and the per-view failure reason: the two shapes the
// Team tab's design branch wires. Both read signals, so a view re-evaluates.

const MACHINE = "00000000-0000-4000-8000-0000000000aa";
const LEAD = "launch_id:00000000-0000-4000-8000-000000000064";
const list = (rows: unknown[], next: string | null = null) => ({ teams: rows, truncated: false, next });
const summary = (lead: unknown) => ({ id: TEAM_ID, project: "example-project", name: "Fixes", status: "running", lead: "lead", home: MACHINE, revision: 16, lead_session: lead });
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });

function fake(outcome?: TeamOutcome) {
  const calls = { follow: [] as unknown[], pages: [] as [string, string | null][], acts: [] as unknown[] };
  const transport: TeamsTransport = {
    async follow(follow) { calls.follow.push(follow); },
    async hidden() {}, async tasksChanged() {},
    async page(key, cursor) { calls.pages.push([key, cursor]); },
    async act(call) { calls.acts.push(call); return outcome ?? { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async actAgain(call) { return { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss() {}, async catalog() { return {}; }, async saveTemplate() {}, async project() { return {}; }, async preview() { return {}; },
  };
  return { transport, calls };
}

describe("the reactive lead binding", () => {
  it("answers unknown before the list, this team when it arrives, and re-evaluates", async () => {
    const { transport } = fake();
    const store = createTeams(transport);
    const seen: string[] = [];
    let stop!: () => void;
    createRoot((dispose) => {
      stop = dispose;
      createEffect(() => seen.push(store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-000000000064" }).state));
    });
    await Promise.resolve();
    expect(seen).toEqual(["unknown"]);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: LEAD })]))]);
    await Promise.resolve();
    stop();
    expect(seen).toEqual(["unknown", "lead"]);
  });

  it("follows the list page by page and never answers none from an incomplete list", () => {
    const { transport, calls } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([{ ...summary(null), id: "00000000-0000-4000-8000-00000000aaaa" }], "c1"))]);
    // The lead is not on the first page yet: ask for the next, answer unknown.
    expect(store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-000000000064" })).toEqual({ state: "unknown" });
    expect(calls.pages).toEqual([["list", "c1"]]);
    // The same page cursor is never asked twice, and the next page finds it.
    store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-000000000064" });
    expect(calls.pages).toHaveLength(1);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: LEAD })], null))]);
    expect(store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-000000000064" })).toEqual({ state: "lead", team: TEAM_ID });
  });

  it("answers none only when the list is complete", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([summary(null)]))]);
    expect(store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-0000000000ff" })).toEqual({ state: "none" });
  });

  it("matches the machine and the session together", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: LEAD })]))]);
    expect(store.teamOfSession({ machine: MACHINE, launchId: "00000000-0000-4000-8000-000000000064" })).toEqual({ state: "lead", team: TEAM_ID });
    expect(store.teamOfSession({ machine: "00000000-0000-4000-8000-0000000000bb", launchId: "00000000-0000-4000-8000-000000000064" })).toEqual({ state: "none" });
  });

  it("keeps no per-session cache: a repeated lookup reads the same list", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: LEAD })]))]);
    for (let n = 0; n < 128; n++) store.teamOfSession({ machine: MACHINE, launchId: `00000000-0000-4000-9000-${String(n).padStart(12, "0")}` });
    expect(store.listPages()).toHaveLength(1);
  });
});

describe("the failure reason for the view that asked", () => {
  it("names the view, reads its own reason and clears only its own", async () => {
    const { transport } = fake({ outcome: "failed", detail: "The member cannot be removed.", code: "invalid_state", request_id: "r1" });
    const store = createTeams(transport);
    const call = { team: TEAM_ID, expected_revision: 16, request_id: "r1", action: { kind: "archive_member", member: "lead" } };
    const outcome = await store.run(call, "team-tab");
    expect(outcome.outcome).toBe("failed");
    expect(store.failureFor("team-tab")?.message).toBe("The member cannot be removed.");
    expect(store.failureFor("new-team")).toBeNull();
    store.clearFailure("new-team");
    expect(store.failureFor("team-tab")).not.toBeNull();
    store.clearFailure("team-tab");
    expect(store.failure()).toBeNull();
  });

  it("a lost reply is uncertain, not a failure, and keeps its id", async () => {
    const { transport } = fake();
    transport.act = async () => { throw new Error("reply channel disconnected"); };
    const store = createTeams(transport);
    const call = { team: TEAM_ID, expected_revision: 16, request_id: "r2", action: { kind: "accept", node: 2 } };
    const outcome = await store.run(call, "team-tab");
    expect(outcome.outcome).toBe("uncertain");
    expect(store.failureFor("team-tab")).toBeNull();
    expect(store.unresolved()[0].call.request_id).toBe("r2");
  });

  it("keeps two unknown intents and refuses a ninth until one is settled", async () => {
    const { transport } = fake();
    transport.act = async (call) => ({ outcome: "uncertain", detail: "unknown", request_id: call.request_id });
    const store = createTeams(transport);
    for (let n = 0; n < 8; n++) await store.run({ team: TEAM_ID, request_id: `u${n}`, action: { kind: "accept", node: n } });
    expect(store.unresolved().map((entry) => entry.call.request_id)).toEqual(["u0", "u1", "u2", "u3", "u4", "u5", "u6", "u7"]);
    const refused = await store.run({ team: TEAM_ID, request_id: "u8", action: { kind: "accept", node: 8 } });
    expect(refused.outcome).toBe("failed");
    expect(store.failure()?.code).toBe("unresolved_full");
    expect(store.unresolved()).toHaveLength(8);
  });

  it("does not lose the first unknown intent when another becomes unknown", async () => {
    const { transport } = fake();
    transport.act = async (call) => ({ outcome: "uncertain", detail: "reply lost", request_id: call.request_id });
    const store = createTeams(transport);
    await store.run({ team: TEAM_ID, request_id: "a", action: { kind: "accept", node: 2 } });
    await store.run({ team: TEAM_ID, request_id: "b", action: { kind: "accept", node: 2 } });
    expect(store.unresolved().map((entry) => entry.call.request_id)).toEqual(["a", "b"]);
  });

  it("keeps the unknown intent through a re-follow and never resends it", async () => {
    const { transport, calls } = fake();
    const sent: string[] = [];
    transport.act = async (call) => { sent.push(call.request_id); return { outcome: "uncertain", detail: "reply lost", request_id: call.request_id }; };
    const store = createTeams(transport);
    await store.run({ team: TEAM_ID, request_id: "a", action: { kind: "accept", node: 2 } });
    await store.openTeam(TEAM_ID);
    await store.closeTeam();
    expect(store.unresolved()[0].call.request_id).toBe("a");
    expect(sent).toEqual(["a"]);
    expect(calls.acts).toEqual([]);
  });

  it("checks with the same id and dismisses only that call", async () => {
    const { transport } = fake();
    transport.act = async (call) => ({ outcome: "uncertain", detail: "unknown", request_id: call.request_id });
    const again: string[] = [];
    transport.actAgain = async (call) => { again.push(call.request_id); return { outcome: "done", receipt: {}, request_id: call.request_id }; };
    const store = createTeams(transport);
    await store.run({ team: TEAM_ID, request_id: "a", action: { kind: "accept", node: 2 } });
    await store.check(store.unresolved()[0], "team-tab");
    expect(again).toEqual(["a"]);
    expect(store.unresolved()).toEqual([]);
  });
});

describe("the launch identity, not the row's session id", () => {
  const launch = "00000000-0000-4000-8000-000000000064";
  const otherSession = "00000000-0000-4000-8000-0000000000ff";
  it("matches only the launch id and not a different session id", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: `launch_id:${launch}` })]))]);
    // A row carries both ids; only the launch one is the team's binding.
    const row = { machine: MACHINE, sessionId: otherSession, launchId: launch, runId: null };
    expect(store.teamOfSession(row)).toEqual({ state: "lead", team: TEAM_ID });
    expect(store.teamOfSession({ ...row, launchId: otherSession })).toEqual({ state: "none" });
  });

  it("matches nothing when the machine differs, even with the same launch", () => {
    const { transport } = fake();
    const store = createTeams(transport);
    store.apply([entry("list", "list", list([summary({ machine: MACHINE, session_id: `launch_id:${launch}` })]))]);
    expect(store.teamOfSession({ machine: "00000000-0000-4000-8000-0000000000bb", launchId: launch, runId: null })).toEqual({ state: "none" });
  });
});

describe("a late view after unfollow", () => {
  it("cannot resurrect the retired team", async () => {
    const { transport } = fake();
    const store = createTeams(transport);
    await store.openTeam(TEAM_ID);
    await store.closeTeam();
    store.apply([entry(`team:${TEAM_ID}`, "team", teamView)]);
    expect(store.team()).toBeUndefined();
  });
});
