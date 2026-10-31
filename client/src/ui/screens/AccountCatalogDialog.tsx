import { Show, createSignal, onMount, onCleanup } from "solid-js";
import { Icon } from "../lib/icons";
import type { AccountPrompt } from "../../store/accountManagement";

/** Mockup SettingsDialogs RenameDialog and accounts-remove Confirm. */
export default function AccountCatalogDialog(props: { prompt: AccountPrompt; close(handle: string): void; confirm(label: string, handle: string): void }) {
  const view = props.prompt, rename = view.kind === "rename";
  const [label, setLabel] = createSignal(view.label);
  let dialog!: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    queueMicrotask(() => { const first=dialog.querySelector<HTMLElement>("[data-initial]");first?.focus();if(first instanceof HTMLInputElement)first.select(); });
  });
  onCleanup(() => { dialog.close(); if(view.origin.isConnected)view.origin.focus(); });
  const submit=(event: Event) => { event.preventDefault(); if(!rename || label().trim())props.confirm(label().trim(),view.handle); };
  return <dialog ref={dialog} class="dlg" aria-labelledby="account-catalog-title" style={{width:rename ? "420px" : "460px"}}
    onCancel={(event)=>{event.preventDefault();props.close(view.handle);}} onClose={()=>props.close(view.handle)}>
    <form onSubmit={submit}>
      <div class="dlg-head"><Icon name={rename ? "pencil" : "triangle-alert"} size={18} class="accent" />
        <h2 id="account-catalog-title">{rename ? "Rename account" : "Remove account from Zerus"}</h2><span class="grow" />
        <button type="button" class="icon-btn" aria-label="Close" onClick={()=>props.close(view.handle)}><Icon name="x" size={16}/></button>
      </div>
      <div class="dlg-body">
        <Show when={rename} fallback={<div class="confirm-body">
          <p><b>Remove “{view.label}” from {view.machine}?</b></p>
          <p class="muted" style={{"white-space":"pre-line"}}>{view.provider === "opencode"
            ? `OpenCode will disappear from Accounts and New session for ${view.machine} in every Zerus connected to ${view.machine}, including Zerus running on ${view.machine}.\n\nIts native configuration, provider credentials and existing sessions on ${view.machine} are kept.\n\nTo use it here again, choose Add account.`
            : `This account will disappear from Accounts and New session for ${view.machine} in every Zerus connected to ${view.machine}, including Zerus running on ${view.machine}.\n\nSign-in credentials and existing sessions on ${view.machine} are kept. The account stays available on its other machines.\n\nTo use it here again, choose Add account or Add to ${view.machine}. Zerus can reuse the saved sign-in.`}</p>
        </div>}>
          <div class="form-row"><label for="rn-name">Account name</label><input id="rn-name" class="field" data-initial maxlength={160} value={label()} onInput={event=>setLabel(event.currentTarget.value)}/></div>
        </Show>
      </div>
      <div class="dlg-foot"><span class="spacer" />
        <button type="button" class="btn" data-initial={!rename ? "" : undefined} onClick={()=>props.close(view.handle)}>Cancel</button>
        <button type="submit" class={rename ? "btn primary" : "btn solid-danger"} disabled={rename && !label().trim()}>{rename ? "Save" : `Remove from ${view.machine}`}</button>
      </div>
    </form>
  </dialog>;
}
