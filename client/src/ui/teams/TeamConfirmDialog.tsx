import { Show, onCleanup, onMount } from "solid-js";
import { Icon } from "../lib/icons";
import { tr, trc } from "../../i18n";
import { teams } from "../../store/teams";

type Asking = NonNullable<ReturnType<typeof teams.confirmation>>;
type Words = { title: string; text: string; action: string };

/** The core's confirmation in the window's language (zerus-63rru.197). The
 * English of each sentence is the core's own (client/core teams/action/input.rs
 * `confirmation`), which the shared confirmations.json fixture pins; the values
 * come from the call the core confirmed. A confirmation this version does not
 * know keeps the core's words. */
export function confirmationWords(asking: Asking): Words {
  const call = asking.call;
  const name = call.team_name?.trim() || tr("this team");
  const member = String(call.action.member ?? "");
  switch (asking.confirmation.name) {
    case "teamAdvanceBaseConfirm": return { title: tr("Advance base?"), action: tr("Advance base"),
      text: tr("Advance the base of %1 to %2? Future hand-outs use it. Earlier hand-outs keep their assigned base. Branches are unchanged.", name, String(call.action.commit)) };
    case "teamSwapMemberConfirm": return { title: tr("Swap member?"), action: tr("Swap member"),
      text: tr("Replace the agent for %1 of %2? The old session stays outside the team. The member keeps its ID, task history, folder and branch. Only a person-stopped member with no open or unsettled work can be swapped.", member, name) };
    case "teamStopConfirm": return { title: tr("Stop team?"), action: tr("Stop team"),
      text: tr("Stop %1? Every open task is cancelled and no task is handed out again. Member sessions keep running; their current turns finish. A stopped team cannot be started again; use Pause to stop it for a while.", name) };
    case "teamArchiveConfirm": return { title: tr("Archive team?"), action: trc("team action", "Archive"),
      text: tr("Archive %1? Its history, tasks and reports stay. The list of teams shows it under Show archived. Archiving cannot be undone.", name) };
    case "teamStopMemberConfirm": return { title: tr("Stop member?"), action: tr("Stop member"),
      text: tr("Stop %1 of %2? Its current turn finishes and it takes no new task.", member, name) };
    case "teamArchiveMemberConfirm": return { title: tr("Remove member?"), action: tr("Remove member"),
      text: tr("Remove %1 from %2? Its history and reports stay. Removing a member cannot be undone; add a new member to take its place.", member, name) };
    case "teamCancelConfirm": return { title: tr("Cancel task?"), action: tr("Cancel task"),
      text: tr("Cancel task #%1 of %2? Its work stops and its result is not accepted.", String(call.action.node), name) };
    default: return asking.confirmation;
  }
}

// The person's confirmation of a destructive team action: the core refused to
// send it (`confirm`), so nothing ran yet. The dialog shows the core's own
// title and text; its action button sends the same intent again marked
// confirmed, and Cancel drops it. Escape and closing cancel.

export function TeamConfirmation(props: { name: string; title: string; text: string; action: string; onCancel: () => void; onConfirm: () => void }) {
  let dialog!: HTMLDialogElement;
  const origin = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
  onMount(() => {
    dialog.showModal();
    queueMicrotask(() => dialog.querySelector<HTMLElement>("[data-initial]")?.focus());
  });
  onCleanup(() => { if (dialog.open) dialog.close(); if (origin?.isConnected) origin.focus(); });
  return (
    <dialog ref={dialog} class="dlg team-confirm" data-name={props.name}
      aria-labelledby="team-confirm-title" aria-describedby="team-confirm-text"
      onCancel={(event) => { event.preventDefault(); props.onCancel(); }}>
      <div class="dlg-head"><Icon name="triangle-alert" size={18} class="warn" /><h2 id="team-confirm-title">{props.title}</h2></div>
      <div class="dlg-body"><p id="team-confirm-text" class="team-confirm-text">{props.text}</p></div>
      <div class="dlg-foot">
        <span class="spacer" />
        <button type="button" class="btn solid-danger" onClick={props.onConfirm}>{props.action}</button>
        <button type="button" class="btn primary" data-initial="" autofocus onClick={props.onCancel}>{tr("Cancel")}</button>
      </div>
    </dialog>
  );
}

export function TeamConfirmDialog(props: { source?: string } = {}) {
  return (
    <Show when={teams.confirmation()} keyed>
      {(asking) => {
        const words = confirmationWords(asking);
        return <TeamConfirmation name={asking.confirmation.name} title={words.title} text={words.text} action={words.action}
          onConfirm={() => void teams.confirm(props.source)} onCancel={() => teams.cancelConfirmation()} />;
      }}
    </Show>
  );
}
