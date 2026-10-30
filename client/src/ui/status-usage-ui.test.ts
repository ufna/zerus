import { describe, expect, it } from "vitest";
import { statusUsageProfile, statusUsageVisible } from "./status-usage-ui";
import { toSession } from "../store/catalog";
import { rowOut, usageOf } from "../store/testing";
import { summaryText } from "./lib/usage";

const session = () => toSession("studio", "run:1", rowOut("studio", 1, { archive_id: null, tracked: true }).row);

describe("status bar account usage", () => {
  it("shows only a tracked live session on Sessions without a subagent", () => {
    const s = session();
    expect(statusUsageVisible(s, "sessions", false)).toBe(true);
    expect(statusUsageVisible(s, "sessions", true)).toBe(false);
    expect(statusUsageVisible(s, "accounts", false)).toBe(false);
    expect(statusUsageVisible({ ...s, tracked: false }, "sessions", false)).toBe(false);
    expect(statusUsageVisible({ ...s, archiveId: "archive-1" }, "sessions", false)).toBe(false);
    expect(statusUsageVisible(undefined, "sessions", false)).toBe(false);
  });

  it("opens the exact bound account profile when its id is known", () => {
    const s = session();
    expect(statusUsageProfile(s, usageOf({ id: "claude-work" }))).toBe("studio\u001fclaude-work");
    expect(statusUsageProfile(s, usageOf())).toBeUndefined();
    expect(summaryText(usageOf({ status: "unavailable" }))).toBe("limits n/a");
  });
});
