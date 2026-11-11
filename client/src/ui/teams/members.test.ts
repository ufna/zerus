import { describe, expect, it } from "vitest";
import { accountLabel, memberMenu } from "./members";
import { teamView } from "./fixtures";

// The member column's M1 vocabulary: the account and the operations the core
// has for a member (pause, resume, stop, remove).
describe("the member column", () => {
  it("labels the account a member runs on", () => {
    expect(accountLabel({ ...teamView.members[0], account: "account-example" })).toBe("account-example");
    expect(accountLabel({ ...teamView.members[0], account: "" })).toBe("the default account");
  });

  it("offers pause and stop to an active member", () => {
    expect(memberMenu(teamView.members[1]).filter((item) => item.enabled).map((item) => item.id)).toEqual(["pause", "stop", "archive"]);
  });

  it("offers resume to a paused member", () => {
    const paused = { ...teamView.members[1], state: "paused" };
    expect(memberMenu(paused).filter((item) => item.enabled).map((item) => item.id)).toEqual(["resume", "stop", "archive"]);
  });

  it("offers nothing to a member that was never created", () => {
    expect(memberMenu(teamView.members[2]).filter((item) => item.enabled)).toEqual([]);
  });
});
