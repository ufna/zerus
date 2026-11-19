// Adapted from WhiteWh's history.ts helpers. Our journal has its own text and
// actor, not the fork engine's `detail` payload; never infer missing events.
import { tr, trc } from "../../i18n";
import type { DrawerRow } from "../../store/teamDrawers";

export function feedClock(atMs: number): string {
  if (!atMs) return "";
  const date = new Date(atMs);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** What closes an open lead inbox item that is neither a question nor a
 * report (zerus-63rru.197 C6). No person operation dismisses an item; each
 * line names the store's own closer: an item of a task closes with the task's
 * next report, reopen or settlement (ops common.rs, the node-wide close); the
 * lead queue notices close when their queue is empty (lead_queue.rs); a
 * planning goal with the first submitted draft (drafts.rs); a phase rejection
 * with a new draft, new corrections or an undo (phase_review.rs); a sent-back
 * phase's note is never closed (rework.rs). An ended team closes nothing. */
export function inboxCloses(row: Pick<DrawerRow, "kind" | "node" | "state">, status: string): string | undefined {
  if (row.state !== "open" || row.kind === "report" || row.kind.startsWith("question:")) return undefined;
  if (["stopped", "finished", "archived"].includes(status)) return tr("The team has ended, so this item stays open as a record.");
  switch (row.kind.split(":")[0]) {
    case "planning_goal": return tr("This item closes when you or the lead submit a phase draft.");
    case "phase_rejected": return tr("This item closes when the phase's draft or corrections are submitted again, or its rejection is undone.");
    case "phase_sent_back": return tr("This item records that the phase was sent back; nothing closes it. Make the corrections in Layers.");
    case "lead_admission": return tr("This item closes by itself when no task waits for admission.");
    case "lead_routing": return tr("This item closes by itself when no task waits for routing.");
  }
  return row.node
    ? tr("This item closes by itself when task #%1 is reported again, reopened or settled.", row.node)
    : tr("Nothing in this window closes this item; the team's service closes it when its cause is settled.");
}

export function inboxKind(kind: string): string {
  switch (kind.split(":")[0]) {
    case "report": return tr("Report");
    case "provider_stop": return tr("Provider stop");
    case "lead_member_added": return tr("Lead added a member");
    case "question": return tr("Question");
    case "permission": return tr("Permission");
    case "planning_goal": return tr("Planning goal");
    default: return kind;
  }
}

export function inboxText(row: DrawerRow): string {
  if (!row.kind.startsWith("unstaffed:") || row.text_truncated) return row.text;
  // Retain the stored English sentence for older CLI readers. Translate
  // only the service's exact notice, with IDs kept as data.
  const notice = /^(Task|Pipeline stage) ([1-9][0-9]*) is ready\. The service cannot read any eligible member \(([a-zA-Z0-9_-]+(?:, [a-zA-Z0-9_-]+)*)\)\. It will check again\.$/.exec(row.text);
  if (!notice || notice[2] !== String(row.node)) return row.text;
  const [, subject, node, members] = notice;
  return subject === "Pipeline stage"
    ? tr("Pipeline stage %1 is ready. The service cannot read any eligible member (%2). It will check again.", node, members)
    : tr("Task %1 is ready. The service cannot read any eligible member (%2). It will check again.", node, members);
}

export const reportRows = (rows: readonly DrawerRow[]) => rows.filter((row) => row.kind === "report");
export const historyLine = (row: DrawerRow) => row.text || row.kind;

/** Who acted: you, the team service, or the member's own ID (zerus-rzvct G7).
 * `by` alone names the same person for your actions and the service's. */
export function actorLabel(row: Pick<DrawerRow, "by" | "by_kind">): string {
  switch (row.by_kind) {
    case "person": return tr("You");
    case "service": return tr("Team service");
    default: return row.by;
  }
}

/** An inbox item's or card's state word; an unknown state stays as written. */
export function itemState(state: string): string {
  switch (state) {
    case "open": return trc("team item state", "Open");
    case "closed": return trc("team item state", "Closed");
    default: return state;
  }
}

/** One journal event as a sentence (zerus-rzvct G6): the store Operation's
 * serde tag is the row's kind. A kind this window does not know reads as its
 * words, never as an op code. */
export function journalSentence(kind: string): string {
  switch (kind) {
    case "create": return tr("Team created");
    case "start_planning": return tr("Planning started");
    case "add_member": return tr("Member added");
    case "lead_add_member": return tr("Lead added a member to a role");
    case "swap_member": return tr("Member swapped");
    case "rename_team": return tr("Team renamed");
    case "set_goal": return tr("Goal changed");
    case "set_member_session": return tr("Member session linked");
    case "set_member_handle": return tr("Member session handle recorded");
    case "set_member_state": return tr("Member state changed");
    case "pause": return tr("Team paused");
    case "resume": return tr("Team resumed");
    case "stop": return tr("Team stopped");
    case "archive": return tr("Team archived");
    case "advance_base": return tr("Team base advanced");
    case "pause_member": return tr("Member paused");
    case "resume_member": return tr("Member resumed");
    case "stop_member": return tr("Member stopped");
    case "archive_member": return tr("Member removed");
    case "unbind_member": return tr("Member session unlinked");
    case "add_task": return tr("Task added");
    case "admit": return tr("Task admitted");
    case "route": return tr("Task routed");
    case "set_deps": return tr("Dependencies changed");
    case "hold": return tr("Task held");
    case "release": return tr("Task released");
    case "cancel": return tr("Task cancelled");
    case "record_cancel": return tr("Cancellation recorded");
    case "handout_intent": return tr("Hand-out prepared");
    case "handout_outcome": return tr("Hand-out result recorded");
    case "settle_left_handout": return tr("Hand-out of a departed member settled");
    case "release_provider_stop": return tr("Task of a member stopped by its provider returned");
    case "reopen_intent": return tr("Reopen prepared");
    case "reopen_outcome": return tr("Reopen result recorded");
    case "observe_task": return tr("Task state observed");
    case "record_report": return tr("Report recorded");
    case "record_late_report": return tr("Late report recorded");
    case "stage_task_intent": return tr("Stage task prepared");
    case "stage_task_outcome": return tr("Stage task result recorded");
    case "verdict": return tr("Stage verdict recorded");
    case "accept": return tr("Result accepted");
    case "return": return tr("Task returned");
    case "reassign": return tr("Task reassigned");
    case "record_integration": return tr("Result merged");
    case "record_integration_conflict": return tr("Merge conflict recorded");
    case "ask": return tr("Question asked");
    case "reply": return tr("Question answered");
    case "open_card": return tr("Card opened");
    case "close_card": return tr("Card closed");
    case "decide_card": return tr("Card decided");
    case "change_settings": return tr("Settings changed");
    case "set_validation_config": return tr("Tests and builds settings changed");
    case "fire_timer": return tr("Timer fired");
    case "record_key_clear": return tr("Team key cleared from a task");
    case "record_late_session": return tr("Late session recorded");
    case "record_member_readability": return tr("The service recorded whether member data could be read");
    case "reconcile_unstaffed": return tr("Tasks without a member checked");
    case "end_stale_unstaffed": return tr("Notice about tasks without a member closed");
    case "close_stale_reopen_notices": return tr("The service closed reopen notices that no longer need the lead's attention");
    case "reconcile_lead_queue": return tr("Lead queue checked");
    case "tell_paused_started": return tr("Start during a pause reported");
    case "close_paused_started": return tr("Start during a pause notice closed");
    case "record_timing_fact": return tr("Timing recorded");
    case "submit_draft_phase": return tr("Phase draft submitted");
    case "edit_draft_phase": return tr("Phase draft edited");
    case "approve_phase": return tr("Phase approved");
    case "reject_phase": return tr("Phase rejected");
    case "undo_reject_phase": return tr("Phase rejection undone");
    case "accept_phase": return tr("Phase accepted");
    case "send_back_phase": return tr("Phase sent back");
    case "set_phase_barrier": return tr("Phase barrier changed");
    case "pause_phase": return tr("Phase paused");
    case "resume_phase": return tr("Phase resumed");
    case "move_phase": return tr("Phase moved");
    case "set_phase_acceptance": return tr("Phase acceptance policy changed");
    case "submit_phase_corrections": return tr("Phase corrections submitted");
    case "edit_phase_corrections": return tr("Phase corrections edited");
    case "approve_phase_corrections": return tr("Phase corrections approved");
    case "reject_phase_corrections": return tr("Phase corrections rejected");
    case "undo_phase_correction_rejection": return tr("Correction rejection undone");
    case "reconcile_correction_review": return tr("Correction review checked");
    case "reconcile_phase_review": return tr("Phase review checked");
    case "reconcile_publication": return tr("Phase publication checked");
    case "publication_task_problem": return tr("Phase task problem recorded");
    case "reconcile_phase": return tr("Phase state checked");
    // Kinds a writer sets in place of its operation (`ctx.event`).
    case "person_override": return tr("You changed the task outside the team");
    case "accepted_superseded": return tr("Your own run replaced the accepted result");
    case "stale_card_decision": return tr("Decision on an outdated card");
    default: return kind.replaceAll("_", " ");
  }
}

/** A hand-out result's word as the journal writes it (ops/journal.rs). */
function handoutOutcome(word: string): string | undefined {
  switch (word) {
    case "submitted": return tr("Submitted");
    case "refused": return tr("Refused");
    case "uncertain": return tr("Outcome unknown");
    case "cutting over": return tr("Moving to another session");
    case "handed back": return tr("Handed back");
    default: return undefined;
  }
}

/** What a journal row's text says beyond its sentence, shown as visible text
 * (zerus-rzvct G6 review): the outcome and reason of a hand-out, a superseded
 * decision, or a sentence the service wrote. The generic "<kind> at revision
 * N" says nothing more and gives "". The exact text stays the row's title. */
export function journalDetail(row: Pick<DrawerRow, "kind" | "text">): string {
  const generic = new RegExp(`^${row.kind.replace(/[^a-z_]/g, "")} at revision \\d+`);
  const matched = row.text.match(generic);
  if (!matched) return row.text;
  const rest = row.text.slice(matched[0].length).replace(/^[:;]\s*/, "");
  if (row.kind === "record_report" || row.kind === "verdict") {
    const skip = /^Land stages skipped for task #([1-9][0-9]*) because landing is disabled in Team settings\.(?=; |$)/.exec(rest);
    if (skip) {
      return tr("Land stages skipped for task #%1 because landing is disabled in Team settings.", skip[1]) + rest.slice(skip[0].length);
    }
  }
  if (row.kind === "release_provider_stop"
      && rest === ". The service confirmed that the member stopped after a provider error.") {
    return tr("The service confirmed that the member stopped after a provider error.");
  }
  if (row.kind === "handout_outcome" && rest) {
    const [word, ...reason] = rest.split(": ");
    const translated = handoutOutcome(word);
    if (translated) return [translated, ...reason].join(": ");
  }
  return rest;
}
