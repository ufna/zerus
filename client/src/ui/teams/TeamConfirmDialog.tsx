import { Show, onCleanup, onMount } from "solid-js";
import { Icon } from "../lib/icons";
import { tr } from "../../i18n";
import { teams } from "../../store/teams";

// The person's confirmation of a destructive team action: the core refused to
// send it (`confirm`), so nothing ran yet. The dialog shows the core's own
// title and text; its action button sends the same intent again marked
// confirmed, and Cancel drops it. Escape and closing cancel.

function Dialog(props: { name: string; title: string; text: string; action: string }) {
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
      onCancel={(event) => { event.preventDefault(); teams.cancelConfirmation(); }}
      onKeyDown={(event) => {
        if (event.key === "Escape") { event.preventDefault(); teams.cancelConfirmation(); }
        if (event.key === "Enter" && (document.activeElement as HTMLElement | null)?.dataset.initial !== undefined) {
          event.preventDefault();
          void teams.confirm();
        }
      }}>
      <div class="dlg-head"><Icon name="triangle-alert" size={18} class="warn" /><h2 id="team-confirm-title">{props.title}</h2></div>
      <div class="dlg-body"><p id="team-confirm-text" class="team-confirm-text">{props.text}</p></div>
      <div class="dlg-foot">
        <span class="spacer" />
        <button type="button" class="btn" onClick={() => teams.cancelConfirmation()}>{tr("Cancel")}</button>
        <button type="button" class="btn solid-danger primary" data-initial="" autofocus onClick={() => void teams.confirm()}>{props.action}</button>
      </div>
    </dialog>
  );
}

export function TeamConfirmDialog() {
  return (
    <Show when={teams.confirmation()} keyed>
      {(asking) => (
        <Dialog name={asking.confirmation.name} title={asking.confirmation.title}
          text={asking.confirmation.text} action={asking.confirmation.action} />
      )}
    </Show>
  );
}
