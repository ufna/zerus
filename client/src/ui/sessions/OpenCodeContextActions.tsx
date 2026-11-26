import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { tr } from "../../i18n";
import { perform } from "../../store/actions";
import type { Details } from "../../types";
import { snapshotOf, type ActionRow } from "./actions-ui";

export function OpenCodeContextActions(props: { row: ActionRow; details?: Details; offline?: boolean; busy?: boolean }) {
  const [target, setTarget] = createSignal<ActionRow>();
  const enabled = () => !props.offline && !props.busy && !!props.row.runId && !!props.row.conversationId
    && props.details?.run_id === props.row.runId && props.details?.conversation_id === props.row.conversationId
    && props.details?.compact_context_supported === true;
  const allowed = (row: ActionRow) => enabled() && props.row.key === row.key
    && props.row.runId === row.runId && props.row.conversationId === row.conversationId;
  const reason = () => props.details?.compact_context_reason || "";
  return <>
    <button type="button" class="btn small" title={reason() ? tr(reason()) : undefined}
      disabled={!enabled()} onClick={() => { if (enabled()) setTarget(snapshotOf(props.row)); }}>{tr("Compact context…")}</button>
    <button type="button" class="btn small" disabled title={tr("OpenCode cannot reset this conversation. Start a new session to begin with empty context.")}>{tr("Clear session…")}</button>
    <Show when={target()} keyed>{(row) => <CompactContextDialog row={row} allowed={() => allowed(row)} close={() => setTarget(undefined)} />}</Show>
  </>;
}

function CompactContextDialog(props: { row: ActionRow; allowed: () => boolean; close: () => void }) {
  let dialog!: HTMLDialogElement;
  let used = false;
  onMount(() => dialog.showModal());
  onCleanup(() => { if (dialog.open) dialog.close(); });
  const confirm = () => {
    if (used || !props.allowed()) return;
    used = true;
    props.close();
    void perform(props.row, { kind: "compact_context" });
  };
  return <dialog ref={dialog} class="dlg" aria-labelledby="compact-context-title" aria-describedby="compact-context-detail"
    onCancel={(event) => { event.preventDefault(); props.close(); }}>
    <div class="dlg-head"><h2 id="compact-context-title">{tr("Compact context")}</h2></div>
    <div class="dlg-body"><p class="mono">{props.row.name}</p><p id="compact-context-detail">{tr("OpenCode will summarize older messages for the same conversation. The summary may omit details.")}</p></div>
    <div class="dlg-foot"><span class="grow" /><button type="button" class="btn" autofocus onClick={props.close}>{tr("Cancel")}</button>
      <button type="button" class="btn primary" disabled={!props.allowed()} onClick={confirm}>{tr("Compact context")}</button></div>
  </dialog>;
}
