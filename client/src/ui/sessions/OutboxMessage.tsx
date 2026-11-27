import { createContext, createEffect, createSignal, Show, useContext } from "solid-js";
import { tr } from "../../i18n";
import { composer } from "../../store/composer";
import { setDetailTab } from "../../store/ui";
import { askedTimeOf, zonedTimeOf } from "../lib/dates";
import { Icon } from "../lib/icons";
import type { OutboxItem } from "./types";

export const OutboxScope = createContext<{ rows: () => readonly OutboxItem[]; done: () => void }>();

const queuedDate = (at?: number) =>
  at !== undefined && at > 0 && at < 253_402_300_800 ? new Date(at * 1000) : undefined;

export const outboxStamp = (item: OutboxItem) => {
  switch (item.state) {
    case "waiting": return tr("Queued in Zerus");
    case "reserved": return tr("Checking delivery");
    case "unknown": return tr("Delivery not confirmed");
    case "failed": return tr("Not sent");
  }
};

export function OutboxHeader(props: { item: OutboxItem }) {
  return <>
    <span class="act-stamp" classList={{ failed: props.item.state === "unknown" || props.item.state === "failed" }}
      title={props.item.state === "waiting" && !props.item.target_changed ? tr("Waiting for the agent to become ready") : undefined}>
      {outboxStamp(props.item)}
    </span>
    <Show when={queuedDate(props.item.queued_at)} fallback={<span class="act-time">{tr("Queue time unavailable")}</span>}>{(at) =>
      <time class="act-time" dateTime={at().toISOString()} title={tr("Queued in Zerus at %1", zonedTimeOf(at()))}>
        {askedTimeOf(at())}
      </time>
    }</Show>
  </>;
}

export function OutboxActions(props: { item: OutboxItem }) {
  const scope = useContext(OutboxScope);
  const [review, setReview] = createSignal<{ handle: number; requestId: string }>();
  let removeButton: HTMLButtonElement | undefined;
  const current = () => scope?.rows().find((row) =>
    row.handle === props.item.handle && row.request_id === props.item.request_id);
  const removable = () => !!current() && !composer.operationLocked() && composer.waitingMessages()[0]?.state !== "reserved";
  const confirming = () => {
    const action = review(), item = current();
    return action && item?.handle === action.handle && item.request_id === action.requestId ? action : undefined;
  };
  createEffect(() => { if (review() && !confirming()) setReview(undefined); });
  const cancel = () => {
    setReview(undefined);
    removeButton?.focus();
  };
  return <>
    <Show when={props.item.target_changed}>
      <p class="act-inbox-hint">{tr("The original session changed.")}</p>
    </Show>
    <Show when={props.item.state === "unknown" && current()?.can_check_terminal}>
      <p class="act-error">{tr("Delivery unknown. Check Terminal before removing this message.")}</p>
    </Show>
    <Show when={props.item.state === "failed"}>
      <p class="act-error">{tr("The agent did not accept this message.")}</p>
    </Show>
    <Show when={current()}>
      <div class="act-local-actions act-outbox-actions">
        <Show when={current()?.can_check_terminal}>
          <button type="button" class="btn small" onClick={() => {
            if (current()?.can_check_terminal) setDetailTab("terminal");
          }}>{tr("Check Terminal")}</button>
        </Show>
        <button type="button" class="icon-btn small" ref={removeButton} disabled={!removable()}
          title={tr("Remove from Zerus queue…")} aria-label={tr("Remove from Zerus queue…")} onClick={() => {
            const item = current();
            if (item && removable()) setReview({ handle: item.handle, requestId: item.request_id });
          }}><Icon name="trash" size={14} /></button>
      </div>
    </Show>
    <Show when={confirming()}>{(action) =>
      <div class="act-outbox-confirm" role="group" aria-label={tr("Remove queued message")} onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          cancel();
        }
      }}>
        <p>{tr("Remove this message from the Zerus queue? This cannot undo delivery to the agent.")}</p>
        <div class="act-local-actions">
          <button type="button" class="btn small" ref={(node) => queueMicrotask(() => {
            if (node.isConnected) node.focus();
          })} onClick={cancel}>{tr("Cancel")}</button>
          <button type="button" class="btn small" disabled={!removable()} onClick={() => {
            const bound = confirming();
            if (!bound || bound !== action() || !removable()) return;
            setReview(undefined);
            scope?.done();
            void composer.removeWaiting(bound.handle, bound.requestId);
          }}>{tr("Remove queued message")}</button>
        </div>
      </div>
    }</Show>
  </>;
}
