import { describe, expect, it } from "vitest";
import { teamRequests, teamSessions, type RequestRow } from "./waiting-requests";
import type { TeamSummary } from "../../store/teams";

// The requests of team sessions (finding 11 of the second review): only a
// question or an approval that asks for the person, not a supervised child's,
// and only a session with a verified binding of a live team. A parent link
// never makes a session a team's.
const MACHINE = "00000000-0000-4000-8000-0000000000aa";
const key = (session: string) => `${MACHINE}\n${session}`;
const lead = (status = "running", machine = MACHINE): TeamSummary => ({
  id: "team-a", project: "example-project", name: "Fixes", status, lead: "lead", home: MACHINE, revision: 1,
  lead_session: { machine, session_id: "launch_id:lead" },
});
const sessions = teamSessions([lead()]);
const row = (over: Partial<RequestRow>): RequestRow => ({
  key: "k", name: "lead", attention: "question", caption: "Needs input", actionNeeded: true, quiet: false,
  identities: [key("launch_id:lead")], ...over,
});

describe("the requests of team sessions", () => {
  it("takes the lead's own question", () => {
    expect(teamRequests([row({})], sessions)).toEqual([{ key: "k", name: "lead", team: "team-a", caption: "Needs input", attention: "question" }]);
  });

  it("never takes an ordinary child by its parent link", () => {
    const child = { ...row({ key: "ordinary-child", identities: [key("launch_id:unrelated")] }), parentIdentity: "launch_id:lead" } as RequestRow;
    expect(teamRequests([child], sessions)).toEqual([]);
  });

  it("leaves out a quiet supervised question, a reply, an acknowledged request and another session", () => {
    expect(teamRequests([
      row({ quiet: true }),
      row({ attention: "reply" }),
      row({ actionNeeded: false }),
      row({ identities: [key("launch_id:other")] }),
      row({ identities: [`00000000-0000-4000-8000-0000000000bb\nlaunch_id:lead`] }),
    ], sessions)).toEqual([]);
  });

  it("takes a member by the member_sessions of its team's live list row (second confirmation, finding 3)", () => {
    const withMembers = { ...lead(), member_sessions: [{ member: "m1", machine: MACHINE, session_id: "launch_id:member-1" }] };
    const member = row({ key: "m1", name: "developer", identities: [key("launch_id:member-1")] });
    expect(teamRequests([member], teamSessions([withMembers]))[0]).toMatchObject({ key: "m1", team: "team-a" });
    // The same row's status decides: a stopped team's members never count.
    expect(teamRequests([member], teamSessions([{ ...withMembers, status: "stopped" }]))).toEqual([]);
    // Another machine with the same session id is not the member.
    expect(teamRequests([row({ identities: [`00000000-0000-4000-8000-0000000000bb\nlaunch_id:member-1`] })], teamSessions([withMembers]))).toEqual([]);
  });

  it("counts only live teams and reads no rows without a team session", () => {
    expect(teamSessions([lead("stopped"), lead("archived"), lead("finished")]).size).toBe(0);
    let read = 0;
    function* rows() { for (;;) { read++; yield row({}); } }
    expect(teamRequests(rows(), new Map())).toEqual([]);
    expect(read).toBe(0);
  });

  it("reads the rows lazily and stops at the bound", () => {
    let read = 0;
    function* rows() { for (let n = 0; n < 10_000; n++) { read++; yield row({ key: `k${n}` }); } }
    expect(teamRequests(rows(), sessions)).toHaveLength(20);
    expect(read).toBe(20);
  });
});
