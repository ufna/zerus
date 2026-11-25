import { expect, it } from "vitest";
import { leadChatBound, leadChatProblem } from "./lead-chat";
import { draftKey } from "../../store/draftKeys";
import { teamView } from "./fixtures";

it("handwritten lead chat controls require the bound machine, session, live run, conversation and composer", () => {
  const row = { computer: "", catalogKey: "run:current", name: "codex/example/lead", sessionId: "11111111-1111-4111-8111-111111111111", state: "live", runId: "current", launchId: "launch", conversationId: "conversation" };
  const key = draftKey({ computer: "", key: row.catalogKey, name: row.name, session_id: row.sessionId, archive_id: null });
  const view = { ...teamView, members: teamView.members.map((member) => member.lead ? { ...member, session: { machine: "machine", session_id: "launch_id:launch" } } : member) };
  expect(leadChatBound(view, row, "machine", key, false)).toBe(true);
  for (const args of [
    [view, row, "other", key, false], [view, { ...row, launchId: "other" }, "machine", key, false],
    [view, { ...row, conversationId: "" }, "machine", key, false], [view, { ...row, runId: "" }, "machine", key, false],
    [view, { ...row, state: "archived" }, "machine", key, false], [view, row, "machine", "old-key", false], [view, row, "machine", key, true],
    [view, undefined, "machine", key, false],
  ] as const) expect(leadChatBound(args[0], args[1], args[2], args[3], args[4])).toBe(false);
});

it("names each refusal's cause, so Lead chat can say what to do (audit C4)", () => {
  const row = { computer: "", catalogKey: "run:current", name: "codex/example/lead", sessionId: "11111111-1111-4111-8111-111111111111", state: "live", runId: "current", launchId: "launch", conversationId: "conversation" };
  const key = draftKey({ computer: "", key: row.catalogKey, name: row.name, session_id: row.sessionId, archive_id: null });
  const view = { ...teamView, members: teamView.members.map((member) => member.lead ? { ...member, session: { machine: "machine", session_id: "launch_id:launch" } } : member) };
  const leadless = { ...view, members: view.members.map((member) => member.lead ? { ...member, session: null } : member) };
  expect(leadChatProblem(view, row, "machine", key, false)).toBeUndefined();
  for (const [args, cause] of [
    [[view, undefined, "machine", key, false], "no_row"], [[view, row, "machine", key, true], "subagent"],
    [[view, { ...row, state: "archived" }, "machine", key, false], "archived"],
    [[view, { ...row, runId: "" }, "machine", key, false], "no_conversation"], [[view, { ...row, conversationId: "" }, "machine", key, false], "no_conversation"],
    [[leadless, row, "machine", key, false], "no_lead"], [[view, row, undefined, key, false], "unknown_computer"],
    [[view, row, "other", key, false], "other_computer"], [[view, { ...row, launchId: "other" }, "machine", key, false], "other_session"],
    [[view, row, "machine", "old-key", false], "composer"],
  ] as const) expect(leadChatProblem(args[0], args[1], args[2], args[3], args[4]), cause).toBe(cause);
});
