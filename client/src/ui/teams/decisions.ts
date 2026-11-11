import { tr } from "../../i18n";
import type { WaitingItem } from "../../store/teams";

// Waiting for you's vocabulary: a card's kind in words and with an icon, its
// age, and the one-click decisions it offers. A decision is a word the store
// sends with one request id; the core answers done, failed, uncertain, confirm
// or pending, and the view never decides by itself.

export interface WaitingDecision {
  /** The decision word the store sends (`decide {card, decision}`). */
  decision: string;
  text: string;
  danger?: boolean;
}

/** The card's kind in words, keyed by the exact kind the store sends: the
 * store opens `integration` and `pipeline_acceptance` itself (the acceptance
 * a pipeline asks the person for; phases are milestone 2), and any other kind
 * through an opened card. A kind not in this table shows its plain name,
 * never another kind's label. */
export function waitingKindLabel(item: Pick<WaitingItem, "kind">): string {
  switch (item.kind) {
    case "integration": return tr("Integration");
    case "note": return tr("Your word");
    case "owner_word_asked": return tr("Your word");
    case "report_escalated": return tr("Report");
    case "permission_escalated": return tr("Permission");
    case "task_waits_owner": return tr("Held task");
    case "team_paused": return tr("Team paused");
    case "pipeline_acceptance": return tr("Work acceptance");
    case "lead_stalled": return tr("Lead stalled");
    default: return item.kind;
  }
}

/** The card's icon (ui/lib/icons names), keyed like the label; a kind not in
 * the table gets a neutral icon of its own. */
export function waitingKindIcon(kind: string): string {
  switch (kind) {
    case "integration": return "git-merge";
    case "note": case "owner_word_asked": return "key-round";
    case "report_escalated": return "message-square";
    case "permission_escalated": return "shield-check";
    case "task_waits_owner": return "clock";
    case "team_paused": return "pause";
    case "pipeline_acceptance": return "check";
    case "lead_stalled": return "triangle-alert";
    default: return "info";
  }
}

/** The decisions the card offers. A verdict kind (`integration`,
 * `pipeline_acceptance`) takes exactly approve or reject; every other kind is
 * closed by any non-empty word, so it is an acknowledgement and not a verdict.
 * Package 10 owns the kinds beyond these; its words are added in this table. */
export function waitingDecisions(item: Pick<WaitingItem, "kind">): WaitingDecision[] {
  if (item.kind === "integration" || item.kind === "pipeline_acceptance") {
    return [
      { decision: "approve", text: tr("Approve") },
      { decision: "reject", text: tr("Reject"), danger: true },
    ];
  }
  return [{ decision: "acknowledged", text: tr("Acknowledge") }];
}

/** The card's age in words, from its creation time. */
export function waitingAge(created_ms: number, now_ms: number): string {
  if (!created_ms) return "";
  const seconds = Math.max(0, Math.floor((now_ms - created_ms) / 1000));
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return tr("now");
  if (minutes < 60) return tr("%1 m", minutes);
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return tr("%1 h", hours);
  return tr("%1 d", Math.floor(hours / 24));
}
