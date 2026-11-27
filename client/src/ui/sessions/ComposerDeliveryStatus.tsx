import { createMemo, For, Show } from "solid-js";
import { tr } from "../../i18n";
import { composer } from "../../store/composer";
import { setDetailTab } from "../../store/ui";
import type { ComposerView } from "../../types";
import { OutboxActions, OutboxScope, outboxStamp } from "./OutboxMessage";
import { localItem } from "./transcript";
import type { OutboxItem } from "./types";

interface DeliveryStatus {
  id: string;
  at: number;
  stamp: string;
  preview: string;
  files: string[];
  outbox?: OutboxItem;
  failed?: boolean;
  uncertain?: boolean;
  error?: string;
}

export function ComposerDeliveryStatus(props: {
  view?: ComposerView; run?: string; conversation?: string; focus: () => void;
}) {
  const waiting = createMemo((): readonly OutboxItem[] => {
    const view = props.view;
    if (!view) return [];
    return composer.waitingMessages().filter(row => row.delivery_in_composer).map(row => ({
      ...row, handle: view.handle,
      can_check_terminal: row.can_check_terminal === true && row.run_id === props.run && row.conversation_id === props.conversation,
    }));
  });
  const rows = createMemo(() => {
    const result = new Map<string, DeliveryStatus>();
    for (const receipt of props.view?.terminal_only_outbox ?? []) {
      if (!receipt.delivery_in_composer || receipt.run_id !== props.run || receipt.conversation_id !== props.conversation) continue;
      result.set(receipt.request_id, { id: receipt.request_id, at: receipt.queued_at ?? 0, stamp: tr("Submitted to terminal"),
        preview: receipt.preview, files: receipt.attachments.flatMap(file => file.name ? [file.name] : []) });
    }
    for (const row of props.view?.local ?? []) {
      if (row.source !== "zerus_terminal_status" || row.run_id !== props.run || row.conversation_id !== (props.conversation ?? "")) continue;
      const info = localItem(row);
      result.set(row.id, { id: row.id, at: row.submitted_at, stamp: info.stamp, preview: row.text,
        files: row.attachments.flatMap(file => typeof file.name === "string" ? [file.name] : []),
        failed: row.status === "error", uncertain: row.uncertain, error: info.error });
    }
    for (const row of waiting()) {
      result.set(row.request_id, { id: row.request_id, at: row.queued_at ?? 0, stamp: outboxStamp(row),
        preview: row.preview, files: row.attachments?.map(file => file.name) ?? [], outbox: row });
    }
    return [...result.values()].sort((a, b) => a.at - b.at);
  });
  const current = () => rows().find(row => row.outbox) ?? rows().at(-1);

  return <Show when={current()}>
    <details class="cmp-delivery">
      <summary><span role="status" data-request-id={current()?.id}>{current()?.stamp}</span></summary>
      <OutboxScope.Provider value={{ rows: waiting, done: props.focus }}>
        <ul class="cmp-delivery-list">
          <For each={rows().map(row => row.id)}>{id => {
            const row = () => rows().find(item => item.id === id)!;
            return <li data-request-id={id}>
              <strong>{row().stamp}</strong>
              <p class="wrap-pre">{row().preview || tr("Message with files")}</p>
              <For each={row().files}>{name => <div class="dim">{name}</div>}</For>
              <Show when={row().outbox}>{item => <OutboxActions item={item()} />}</Show>
              <Show when={row().error}><p class="act-error">{row().error}</p></Show>
              <Show when={row().failed && !row().uncertain}>
                <button type="button" class="btn small" disabled={composer.operationLocked()}
                  onClick={() => void composer.localRetry(id)}>{tr("Retry")}</button>
              </Show>
              <Show when={row().uncertain}>
                <button type="button" class="btn small" onClick={() => setDetailTab("terminal")}>{tr("Check Terminal")}</button>
              </Show>
            </li>;
          }}</For>
        </ul>
      </OutboxScope.Provider>
    </details>
  </Show>;
}
