import { Show } from "solid-js";
import { tr, trc } from "../../i18n";
import type { TeamFailure } from "../../store/teams";
import { Icon } from "../lib/icons";
import "../styles/team-action-failure.css";

// The banner after a refused team action (zerus-uiy9p G10): which action was
// refused, the store's reason, and Dismiss. The asking view's next action that
// goes through clears it as well (store/teams.ts).

/** The person's name of each action kind the core knows
 * (client/core/src/teams/fixtures/action-kinds.json); undefined for another. */
const NAMES: Readonly<Record<string, () => string>> = {
  accept: () => tr("Accept"), accept_phase: () => tr("Accept phase"), add_member: () => tr("Add member"), admit: () => tr("Admit"),
  advance_base: () => tr("Advance base"), approve_phase: () => tr("Approve phase"), approve_phase_corrections: () => tr("Approve corrections"),
  archive: () => trc("team action", "Archive"), archive_member: () => tr("Remove member"), ask: () => tr("Ask"), cancel: () => tr("Cancel task"),
  create: () => tr("Create team"), decide: () => tr("Decide"), edit_draft_phase: () => tr("Edit draft phase"),
  edit_phase_corrections: () => tr("Edit corrections"), hold: () => tr("Hold"), move_phase: () => tr("Move phase"), pause: () => tr("Pause"),
  pause_member: () => tr("Pause member"), pause_phase: () => tr("Pause phase"), reassign: () => tr("Reassign"), reject_phase: () => tr("Reject phase"),
  reject_phase_corrections: () => tr("Reject corrections"), release: () => tr("Release"), rename_team: () => tr("Rename team"), reply: () => tr("Reply"),
  resume: () => tr("Resume"), resume_member: () => tr("Resume member"), resume_phase: () => tr("Resume phase"), return: () => tr("Return"),
  route: () => tr("Route"), send_back_phase: () => tr("Send back phase"), set_deps: () => tr("Dependencies"), set_goal: () => tr("Edit goal"),
  set_phase_acceptance: () => tr("Phase acceptance"), set_phase_barrier: () => tr("Phase barrier"), settings: () => tr("Settings"),
  start_planning: () => tr("Start planning"), stop: () => tr("Stop"), stop_member: () => tr("Stop member"), submit_draft_phase: () => tr("Submit draft phase"),
  submit_phase_corrections: () => tr("Submit corrections"), swap_member: () => tr("Swap member"),
  undo_phase_correction_rejection: () => tr("Undo correction rejection"), undo_reject_phase: () => tr("Undo phase rejection"),
  ask_lead: () => tr("Ask the lead…"), leave_lead_note: () => tr("Leave a note"), read_lead_notice: () => tr("Open update"),
  validation_config: () => tr("Tests and Builds"),
};
export const actionName = (kind: string): string | undefined => Object.hasOwn(NAMES, kind) ? NAMES[kind]() : undefined;

export function teamActionFailureText(message: string): string {
  switch (message) {
    case "Enter a message for the lead.": return tr("Enter a message for the lead.");
    case "The lead's notice is unavailable or already read.": return tr("The lead's notice is unavailable or already read.");
    case "The message contains unsupported control characters.": return tr("The message contains unsupported control characters.");
    case "The person's note is unavailable or already closed.": return tr("The person's note is unavailable or already closed.");
    case "The person's question is unavailable or already closed.": return tr("The person's question is unavailable or already closed.");
    case "Too many open messages of this kind for the lead.": return tr("Too many open messages of this kind for the lead.");
    case "Update the selected desktop service to use messages for the lead.": return tr("Update the selected desktop service to use messages for the lead.");
    case "Wait 30 seconds between messages to the lead.": return tr("Wait 30 seconds between messages to the lead.");
    default: return message;
  }
}

export function ActionFailureNotice(props: { failure: TeamFailure; onDismiss: () => void }) {
  const name = () => props.failure.action ? actionName(props.failure.action) : undefined;
  return (
    <div class="notice danger team-notice team-action-failure" role="alert" data-team-action-failure data-action={props.failure.action}>
      <Icon name="triangle-alert" size={16} />
      <span class="team-action-failure-text">
        <Show when={name()}>{(shown) => <b data-team-action-failure-action>{shown()}</b>}</Show>
        <span data-team-action-failure-message>{teamActionFailureText(props.failure.message)}</span>
      </span>
      <button type="button" class="btn small" data-team-action-failure-dismiss onClick={() => props.onDismiss()}>{tr("Dismiss")}</button>
    </div>
  );
}
