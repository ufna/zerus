import { describe, expect, it } from "vitest";
import { teamRequests, type RequestRow } from "./waiting-requests";

// The requests of team sessions: only a question or an approval that asks for
// the person, and only a session that belongs to a team by the lead's session
// or by its parent link.
const byLead = new Map([["launch_id:lead", { team: "team-a", name: "Fixes" }]]);
const row = (over: Partial<RequestRow>): RequestRow => ({
  key: "k", name: "member", attention: "question", caption: "Needs input", actionNeeded: true,
  identity: "launch_id:member", parentIdentity: "launch_id:lead", ...over,
});

describe("the requests of team sessions", () => {
  it("takes a member's question that nests under the lead", () => {
    expect(teamRequests([row({})], byLead)).toEqual([{ key: "k", name: "member", team: "team-a", caption: "Needs input", attention: "question" }]);
  });

  it("takes the lead's own question", () => {
    expect(teamRequests([row({ identity: "launch_id:lead", parentIdentity: "" })], byLead)[0].team).toBe("team-a");
  });

  it("leaves out a reply, an acknowledged request and another session", () => {
    expect(teamRequests([
      row({ attention: "reply" }),
      row({ actionNeeded: false }),
      row({ parentIdentity: "", identity: "launch_id:other" }),
      row({ attention: "approval", identity: "launch_id:other", parentIdentity: "" }),
    ], byLead)).toEqual([]);
  });

  it("reads no rows when no team has a lead session", () => {
    expect(teamRequests([row({})], new Map())).toEqual([]);
  });

  it("bounds the list", () => {
    const rows = Array.from({ length: 30 }, (_, index) => row({ key: `k${index}` }));
    expect(teamRequests(rows, byLead)).toHaveLength(20);
  });
});
