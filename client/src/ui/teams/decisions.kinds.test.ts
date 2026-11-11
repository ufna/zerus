import { describe, expect, it } from "vitest";
import waiting from "../../../core/src/teams/fixtures/cli-waiting.json";
import cards from "../../../core/src/teams/fixtures/cli-cards.json";
import { waitingDecisions, waitingKindIcon, waitingKindLabel } from "./decisions";

// A card's kind in words and with an icon follows the exact kind the store
// sends. The store opens integration and pipeline_acceptance itself (the
// acceptance a pipeline asks the person for; phases are milestone 2); the
// pinned waiting and card replies carry the kinds below. A kind not in the
// table shows its plain name and a neutral icon, never another kind's.

/** The kinds the pinned replies contain, and the ones the store opens itself. */
const PINNED = [...new Set([...waiting.rows, ...cards.rows].map((row) => row.kind))];
const STORE_OPENS = ["integration", "pipeline_acceptance"];
const TABLE: Record<string, { label: string; icon: string }> = {
  integration: { label: "Integration", icon: "git-merge" },
  note: { label: "Your word", icon: "key-round" },
  owner_word_asked: { label: "Your word", icon: "key-round" },
  report_escalated: { label: "Report", icon: "message-square" },
  permission_escalated: { label: "Permission", icon: "shield-check" },
  task_waits_owner: { label: "Held task", icon: "clock" },
  team_paused: { label: "Team paused", icon: "pause" },
  pipeline_acceptance: { label: "Work acceptance", icon: "check" },
  lead_stalled: { label: "Lead stalled", icon: "triangle-alert" },
};

describe("a card's kind", () => {
  it("covers every kind of the pinned replies and the store's own", () => {
    expect(PINNED.length).toBeGreaterThan(0);
    for (const kind of [...PINNED, ...STORE_OPENS]) {
      expect(TABLE[kind], `${kind} has a label of its own`).toBeDefined();
      expect(waitingKindLabel({ kind })).toBe(TABLE[kind].label);
      expect(waitingKindIcon(kind)).toBe(TABLE[kind].icon);
    }
  });

  it("labels and icons each kind of the table as its own", () => {
    for (const [kind, { label, icon }] of Object.entries(TABLE)) {
      expect(waitingKindLabel({ kind }), kind).toBe(label);
      expect(waitingKindIcon(kind), kind).toBe(icon);
    }
  });

  it("calls the pipeline's acceptance by its own name, not a phase's", () => {
    expect(waitingKindLabel({ kind: "pipeline_acceptance" })).toBe("Work acceptance");
    expect(waitingKindIcon("pipeline_acceptance")).toBe("check");
    expect(waitingDecisions({ kind: "pipeline_acceptance" }).map((choice) => choice.decision)).toEqual(["approve", "reject"]);
  });

  it.each(["phase_acceptance", "unknown_kind", "milestone_review", ""])("an unknown kind %j shows its plain name and a neutral icon", (kind) => {
    const labels = new Set(Object.values(TABLE).map((entry) => entry.label));
    const icons = new Set(Object.values(TABLE).map((entry) => entry.icon));
    expect(waitingKindLabel({ kind })).toBe(kind);
    if (kind) expect(labels.has(waitingKindLabel({ kind })), "never another kind's label").toBe(false);
    expect(waitingKindIcon(kind)).toBe("info");
    expect(icons.has(waitingKindIcon(kind)), "never another kind's icon").toBe(false);
    expect(waitingDecisions({ kind }).map((choice) => choice.decision)).toEqual(["acknowledged"]);
  });
});
