import { describe, expect, it } from "vitest";
import { KNOWN_KINDS, waitingDecisions } from "./decisions";

// A card offers exactly the decisions its owner acts on (second guide review,
// number 12). integration and pipeline_acceptance: the store takes approve or
// reject (ops/talk.rs decide_integration, ops/pipeline/overrides.rs). The
// launch cards (desktop/teams/launch/retry.rs due) and team_actor_blocked
// (teams/wp08c-actor desktop/teams/actor/retry.rs due_read) act on retry; the
// store closes any other card on any word, and no other owner reads it.
const OFFERS: Record<string, string[]> = {
  note: ["acknowledged"],
  integration: ["approve", "reject"],
  pipeline_acceptance: ["approve", "reject"],
  launch_blocked: ["retry", "acknowledged"],
  launch_uncertain: ["retry", "acknowledged"],
  launch_capacity: ["retry", "acknowledged"],
  standing_confirmation: ["acknowledged"],
  team_actor_blocked: ["retry", "acknowledged"],
  report_waiting: ["acknowledged"],
  silent_lead: ["acknowledged"],
  late_session: ["acknowledged"],
};

describe("the decisions a card offers", () => {
  it("are exactly the ones its owner acts on, for every kind", () => {
    expect([...KNOWN_KINDS].sort()).toEqual(Object.keys(OFFERS).sort());
    for (const kind of KNOWN_KINDS) expect(waitingDecisions({ kind }).map((choice) => choice.decision), kind).toEqual(OFFERS[kind]);
  });

  it("offer Retry where the card's text asks for it", () => {
    expect(waitingDecisions({ kind: "team_actor_blocked" }).map((choice) => choice.text)).toEqual(["Retry", "Acknowledge"]);
  });

  it("mark only a rejection as dangerous", () => {
    for (const kind of KNOWN_KINDS) {
      for (const choice of waitingDecisions({ kind })) expect(!!choice.danger, `${kind} ${choice.decision}`).toBe(choice.decision === "reject");
    }
  });

  it("are an acknowledgement for a kind not in the list", () => {
    expect(waitingDecisions({ kind: "unknown_kind" })).toEqual([{ decision: "acknowledged", text: "Acknowledge" }]);
  });
});
