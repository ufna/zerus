import { describe, expect, it } from "vitest";
import { createTeams, newRequestId, type TeamCall, type TeamOutcome } from "./teams";
import { teamView, TEAM_ID } from "../ui/teams/fixtures";
import type { TeamsOut } from "../types";

// Waiting for you's store: the one bounded waiting page, its count, the paging
// and one card's decision against the team's revision, with the five outcomes
// the core answers. A fake transport stands for package 12's commands.

const LIST = {
  teams: [{
    id: TEAM_ID, project: "example-project", name: "Fixes", status: "running", lead: "lead",
    home: "00000000-0000-4000-8000-0000000000aa", revision: 16,
    lead_session: { machine: "00000000-0000-4000-8000-0000000000aa", session_id: "launch_id:00000000-0000-4000-8000-000000000064" },
  }],
  truncated: false,
  next: null,
};
const WAITING = {
  items: [
    { id: `${TEAM_ID}:1`, team: TEAM_ID, card: 1, kind: "note", node: null, text: "Tell the team the release moved to Monday.", state: "open", by: "local", created_ms: 1_700_000_000_000 },
  ],
  next: "w1.next",
};
const entry = (key: string, kind: string, payload: unknown): TeamsOut => ({ computer: "", key, kind, payload });

function fake() {
  const calls = { follow: [] as { list: boolean; waiting: boolean; team: string | null }[], pages: [] as [string, string | null][], acts: [] as TeamCall[], again: [] as TeamCall[], dismissed: [] as TeamCall[] };
  const outcomes: TeamOutcome[] = [];
  const transport = {
    async follow(follow: { list: boolean; waiting: boolean; team: string | null }) { calls.follow.push(follow); },
    async hidden() {}, async tasksChanged() {},
    async page(key: string, cursor: string | null) { calls.pages.push([key, cursor]); },
    async act(call: TeamCall) { calls.acts.push(call); return outcomes.shift() ?? { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async actAgain(call: TeamCall) { calls.again.push(call); return outcomes.shift() ?? { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss(call: TeamCall) { calls.dismissed.push(call); },
    async catalog() { return {}; }, async saveTemplate() {}, async project() { return {}; },
  };
  return { transport, calls, outcomes };
}

const decideOn = async (teams: ReturnType<typeof createTeams>, decision = "done") => {
  const pending = teams.decide(WAITING.items[0], decision);
  await Promise.resolve();
  teams.apply([entry(`team:${TEAM_ID}`, "team", teamView)]);
  return pending;
};

describe("Waiting for you", () => {
  it("takes the one waiting page and counts it, one more when a page follows", async () => {
    const { transport } = fake();
    const teams = createTeams(transport);
    expect(teams.waitingCount()).toBe(0);
    teams.apply([entry("waiting", "waiting", WAITING)]);
    expect(teams.waitingView()?.items).toHaveLength(1);
    expect(teams.waitingCount(), "one card and a next page").toBe(2);
    teams.apply([entry("waiting", "waiting", { items: WAITING.items, next: null })]);
    expect(teams.waitingCount()).toBe(1);
  });

  it("follows the waiting read and asks for its next page", async () => {
    const { transport, calls } = fake();
    const teams = createTeams(transport);
    await teams.openWaiting();
    expect(calls.follow.at(-1)).toEqual({ list: true, waiting: true, team: null });
    await teams.pageWaiting("w1.next");
    expect(calls.pages.at(-1)).toEqual(["waiting", "w1.next"]);
  });

  it("sends a card's decision once with the team's current revision", async () => {
    const { transport, calls } = fake();
    const teams = createTeams(transport);
    teams.apply([entry("list", "list", LIST)]);
    const outcome = await decideOn(teams);
    expect(outcome?.outcome).toBe("done");
    expect(calls.acts).toHaveLength(1);
    expect(calls.acts[0]).toMatchObject({
      team: TEAM_ID, expected_revision: 16,
      action: { kind: "decide", card: 1, decision: "done" },
    });
    expect(calls.acts[0].request_id).toMatch(/^[0-9a-f-]{36}$/);
  });

  it("keeps the five outcomes of a decision", async () => {
    const { transport, outcomes } = fake();
    const teams = createTeams(transport);
    for (const outcome of [
      { outcome: "failed", detail: "refused" },
      { outcome: "uncertain", detail: "may have been applied" },
      { outcome: "confirm", confirmation: { name: "teamStopConfirm", title: "Stop team?", text: "Stop it?", action: "Stop team" } },
      { outcome: "pending", request_id: "p" },
    ] as TeamOutcome[]) {
      outcomes.push(outcome as TeamOutcome);
      const answer = await decideOn(teams);
      expect(answer?.outcome).toBe(outcome.outcome);
      if (outcome.outcome === "uncertain") expect(teams.uncertain()?.detail).toBe("may have been applied");
      if (outcome.outcome === "confirm") expect(teams.confirmation()?.confirmation.title).toBe("Stop team?");
    }
  });

  it("an uncertain decision leaves only by Check with the same id or by Dismiss", async () => {
    const { transport, outcomes, calls } = fake();
    const teams = createTeams(transport);
    outcomes.push({ outcome: "uncertain", detail: "may have been applied", request_id: "u" });
    const asking = await decideOn(teams, "approve");
    expect(asking?.outcome).toBe("uncertain");
    const id = teams.uncertain()!.call.request_id;
    outcomes.push({ outcome: "done", receipt: {}, request_id: id });
    await teams.check();
    expect(calls.again.at(-1)?.request_id).toBe(id);
    expect(teams.uncertain()).toBeNull();
    outcomes.push({ outcome: "uncertain", detail: "still unknown", request_id: id });
    await decideOn(teams, "approve");
    const second = teams.uncertain()!.call.request_id;
    await teams.dismiss();
    expect(calls.dismissed.at(-1)?.request_id).toBe(second);
    expect(teams.uncertain()).toBeNull();
  });

  it("hides the entry points when the preview is off", () => {
    const { transport } = fake();
    const teams = createTeams(transport);
    teams.apply([entry("list", "failed", { code: "teams_inactive", message: "Teams are off." })]);
    expect(teams.available()).toBe(false);
    expect(teams.off()).toBe("inactive");
    teams.apply([entry("waiting", "waiting", WAITING)]);
    expect(teams.waitingCount()).toBe(2);
  });
});
