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

/** Every card kind the store and the team service open on integrate/teams
 * and the branches that add kinds (packages 7, 8, 10 and 17): the store opens
 * integration and pipeline_acceptance itself; the launch worker
 * launch_blocked, launch_uncertain and launch_capacity; standing
 * confirmations standing_confirmation; the team actor team_actor_blocked;
 * the timers report_waiting, silent_lead and late_session; a person, a lead
 * or the service may open a note. */
export const KNOWN_KINDS = [
  "note", "integration", "pipeline_acceptance", "launch_blocked", "launch_uncertain", "launch_capacity",
  "standing_confirmation", "team_actor_blocked", "report_waiting", "silent_lead", "late_session",
] as const;

/** The card's kind in words, keyed by the exact kind the store sends. A kind
 * not in the table shows a neutral label with its plain kind, never another
 * kind's text. */
export function waitingKindLabel(item: Pick<WaitingItem, "kind">): string {
  switch (item.kind) {
    case "note": return tr("Note");
    case "integration": return tr("Integration");
    case "pipeline_acceptance": return tr("Work acceptance");
    case "launch_blocked": return tr("Member not started");
    case "launch_uncertain": return tr("Start not confirmed");
    case "launch_capacity": return tr("No room for members");
    case "standing_confirmation": return tr("Member needs you");
    case "team_actor_blocked": return tr("Team service blocked");
    case "report_waiting": return tr("Report waiting");
    case "silent_lead": return tr("Lead not working");
    case "late_session": return tr("Late session");
    default: return tr("Card: %1", item.kind);
  }
}

/** One sentence that says what a card of this kind is about. */
export function waitingKindSentence(item: Pick<WaitingItem, "kind">): string {
  switch (item.kind) {
    case "note": return tr("A note that the lead, the service or you left for the person.");
    case "integration": return tr("The lead accepted a result; merging it into the integration branch waits for your approval.");
    case "pipeline_acceptance": return tr("A task's pipeline finished and asks you to accept its work.");
    case "launch_blocked": return tr("A team member could not be started; the card says why and whether Zerus tries again.");
    case "launch_uncertain": return tr("Whether a team member started is not known; Zerus checks before it starts anything again.");
    case "launch_capacity": return tr("New team members wait until there is room for another session or account.");
    case "standing_confirmation": return tr("A member's session asks for something Zerus does not confirm on your behalf.");
    case "team_actor_blocked": return tr("The team's service could not finish a step and keeps trying.");
    case "report_waiting": return tr("A member's report has waited for the lead's decision longer than the team allows.");
    case "silent_lead": return tr("The lead has not worked for a while although items wait in its inbox.");
    case "late_session": return tr("A member's session started after the team stopped or the member was removed; Zerus does not stop it.");
    default: return tr("A card of a kind this version does not describe.");
  }
}

/** The card's icon (ui/lib/icons names), keyed like the label; a kind not in
 * the table gets a neutral icon of its own. */
export function waitingKindIcon(kind: string): string {
  switch (kind) {
    case "note": return "message-square";
    case "integration": return "git-merge";
    case "pipeline_acceptance": return "check";
    case "launch_blocked": return "circle-alert";
    case "launch_uncertain": return "circle-question-mark";
    case "launch_capacity": return "hourglass";
    case "standing_confirmation": return "hand";
    case "team_actor_blocked": return "triangle-alert";
    case "report_waiting": return "clock";
    case "silent_lead": return "bell";
    case "late_session": return "timer";
    default: return "info";
  }
}

/** The kinds whose owner acts on the decision `retry`: the launch worker
 * (package 7) for its three launch cards and the team actor for its blocked
 * step check again on the next service pass. */
const RETRY_KINDS = new Set(["launch_blocked", "launch_uncertain", "launch_capacity", "team_actor_blocked"]);

/** The decisions the card's owner accepts, by its kind. The card row does not
 * say which decisions it accepts, so the view knows them by kind, from the
 * store and the service:
 * - integration and pipeline_acceptance: the store takes exactly approve or
 *   reject;
 * - the launch cards and team_actor_blocked: the service acts on retry (it
 *   checks again on the next pass), and any other word closes the card;
 * - every other kind: the store closes the card on any word and nothing
 *   reads it, so the one decision is an acknowledgement. */
export function waitingDecisions(item: Pick<WaitingItem, "kind">): WaitingDecision[] {
  if (item.kind === "integration" || item.kind === "pipeline_acceptance") {
    return [
      { decision: "approve", text: tr("Approve") },
      { decision: "reject", text: tr("Reject"), danger: true },
    ];
  }
  if (RETRY_KINDS.has(item.kind)) {
    return [
      { decision: "retry", text: tr("Retry") },
      { decision: "acknowledged", text: tr("Acknowledge") },
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
