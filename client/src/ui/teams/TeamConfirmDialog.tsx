import { Show, onCleanup, onMount } from "solid-js";
import { Icon } from "../lib/icons";
import { tr, trc } from "../../i18n";
import { trText } from "../../text";
import { teams } from "../../store/teams";

type Asking = NonNullable<ReturnType<typeof teams.confirmation>>;
type Words = { title: string; text: string; action: string };

/** The core's confirmation in the window's language (review of #2558). The
 * sentence is the core's `message`: the native dialog's own first line for the
 * same call (client/core teams/action/input.rs `confirmation_text`), rendered
 * here from the same catalog key, so no sentence is defined twice. The title
 * and button are the core's catalog sources; "Archive" is the team action's
 * word. An older core without a message keeps its English text. */
export function confirmationWords(asking: Asking): Words {
  const asked = asking.confirmation;
  return {
    title: tr(asked.title),
    text: asked.message ? trText(asked.message) : asked.text,
    action: asked.name === "teamArchiveConfirm" ? trc("team action", "Archive") : tr(asked.action),
  };
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
