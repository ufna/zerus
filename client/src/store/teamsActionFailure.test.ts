// zerus-uiy9p G10: a refused action's reason names the action, and the view
// that asked for it drops the reason once its next action goes through. The
// store alone with a fake transport, as in teams.test.ts; the refusal shape is
// the transport's own TeamOutcome.
import { expect, it } from "vitest";
import { createTeams, newRequestId, type TeamCall, type TeamOutcome } from "./teams";
import { catalog, TEAM_ID } from "../ui/teams/fixtures";

function fake() {
  const outcomes: TeamOutcome[] = [];
  const transport = {
    async follow() {}, async hidden() {}, async tasksChanged() {}, async page() {},
    async act(call: TeamCall) { return outcomes.shift() ?? { outcome: "done", receipt: { ok: true }, request_id: call.request_id }; },
    async actAgain(call: TeamCall) { return outcomes.shift() ?? { outcome: "done", receipt: {}, request_id: call.request_id }; },
    async dismiss() {}, async catalog() { return { ok: true, catalog }; }, async saveTemplate() {},
    async project() { return { state: "ok", head: "0123456789abcdef0123456789abcdef01234567", branch: "main" }; }, async preview() { return {}; },
  };
  return { teams: createTeams(transport as never), outcomes };
}
const call = (kind: string, fields: Record<string, unknown>): TeamCall => ({
  team: TEAM_ID, team_name: "Fixes", expected_revision: 16, request_id: newRequestId(), action: { kind, ...fields },
});

it("a refused action keeps its kind beside its reason", async () => {
  const { teams, outcomes } = fake();
  const refused = call("pause_member", { member: "m1" });
  outcomes.push({ outcome: "failed", detail: "This member cannot be paused now.", code: "invalid_state", request_id: refused.request_id });
  await teams.run(refused, "team-tab:a");
  expect(teams.failureFor("team-tab:a")).toMatchObject({ action: "pause_member", message: "This member cannot be paused now.", code: "invalid_state" });
});

it("the asking view's next action that goes through clears its reason, and another view's does not", async () => {
  const { teams, outcomes } = fake();
  const refused = call("accept", { node: 2 });
  outcomes.push({ outcome: "failed", detail: "The task changed.", code: "stale_revision", request_id: refused.request_id });
  await teams.run(refused, "team-tab:a");
  await teams.run(call("accept", { node: 3 }), "team-tab:b");
  expect(teams.failureFor("team-tab:a")?.message).toBe("The task changed.");
  await teams.run(call("accept", { node: 2 }), "team-tab:a");
  expect(teams.failureFor("team-tab:a")).toBeNull();
  expect(teams.failure()).toBeNull();
});
