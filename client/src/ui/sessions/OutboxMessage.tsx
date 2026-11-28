import { createContext, createEffect, createSignal, onCleanup, Show, useContext } from "solid-js";
import { composerCheckDelivery } from "../../bridge";
import { tr } from "../../i18n";
import type { DeliveryObservation, SkipWaiting } from "../../types";
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
      title={props.item.state === "waiting" && !props.item.target_changed && !props.item.blockedBy ? tr("Waiting for the agent to become ready") : undefined}>
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
  const [review, setReview] = createSignal<{ handle: number; requestId: string; skipScope?: string; skipBinding?: SkipWaiting }>();
  const [check, setCheck] = createSignal<{ scope: string; observation?: DeliveryObservation }>();
  let removeButton: HTMLButtonElement | undefined;
  const current = () => scope?.rows().find((row) =>
    row.handle === props.item.handle && row.request_id === props.item.request_id);
  const checkable = () => {
    const row = current();
    return row?.can_check_delivery && row.state === "unknown" ? row : undefined;
  };
  const deliveryScope = (row: OutboxItem) => JSON.stringify([row.handle, row.request_id, row.run_id, row.conversation_id]);
  const activeCheck = () => {
    const row = checkable(), attempt = check();
    return row && attempt?.scope === deliveryScope(row) ? attempt : undefined;
  };
  createEffect(() => { if (check() && !activeCheck()) setCheck(undefined); });
  onCleanup(() => setCheck(undefined));
  const checking = () => {
    const attempt = activeCheck();
    return !!attempt && attempt.observation === undefined;
  };
  const checkDelivery = async () => {
    const row = checkable();
    if (!row || checking()) return;
    const attempt = { scope: deliveryScope(row) };
    setCheck(attempt);
    const observed = await composerCheckDelivery(row.handle, row.request_id).catch(() => "unresolved" as const);
    if (activeCheck() === attempt) setCheck({ ...attempt, observation: observed });
  };
  const removable = () => !!current() && !composer.operationLocked() && composer.waitingMessages()[0]?.state !== "reserved";
  const skippable = () => {
    const rows = scope?.rows(), item = current();
    const binding = item && composer.skipWaitingBinding(item.handle, item.request_id);
    return item && rows && rows.length > 1 && rows[0] === item && item.state === "unknown"
      && !item.target_changed && item.run_id && item.conversation_id && binding
      && binding.target.run_id === item.run_id && binding.target.conversation_id === item.conversation_id
      && JSON.stringify(binding.queue) === JSON.stringify(rows.map((row) => [row.request_id, row.state])) ? item : undefined;
  };
  const removalLabel = () => skippable() ? tr("Skip and continue…") : tr("Remove from Zerus queue…");
  const confirming = () => {
    const action = review(), item = current();
    return action && item?.handle === action.handle && item.request_id === action.requestId
      && (!action.skipScope || (action.skipBinding && skippable() === item && deliveryScope(item) === action.skipScope
        && JSON.stringify(composer.skipWaitingBinding(item.handle, item.request_id)) === JSON.stringify(action.skipBinding))) ? action : undefined;
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
    <Show when={props.item.blockedBy}>{(head) => (
      <p class="act-inbox-hint">{tr("Waiting on the first queued message (delivery not confirmed): %1", head().preview || tr("Message with files"))}</p>
    )}</Show>
    <Show when={props.item.state === "unknown" && current()?.can_check_terminal}>
      <p class="act-error">{tr("Delivery unknown. Check Terminal before removing this message.")}</p>
    </Show>
    <Show when={props.item.state === "failed"}>
      <p class="act-error">{tr("The agent did not accept this message.")}</p>
    </Show>
    <Show when={checkable()}>
      <div role="status" class="act-inbox-hint">{(() => {
        switch (activeCheck()?.observation) {
          case "accepted": return tr("Native acceptance observed. This does not confirm the model read the message.");
          case "no_input": return tr("Confirmed: no input was offered for this message.");
          case "unresolved": return tr("Delivery remains unconfirmed.");
          default: return "";
        }
      })()}</div>
    </Show>
    <Show when={current()}>
      <div class="act-local-actions act-outbox-actions">
        <Show when={checkable()}>
          <button type="button" class="btn small" disabled={checking()}
            onClick={() => void checkDelivery()}>{tr("Check delivery")}</button>
        </Show>
        <Show when={current()?.can_check_terminal}>
          <button type="button" class="btn small" onClick={() => {
            if (current()?.can_check_terminal) setDetailTab("terminal");
          }}>{tr("Check Terminal")}</button>
        </Show>
        <button type="button" class={skippable() ? "btn small" : "icon-btn small"} ref={removeButton} disabled={!removable()}
          title={removalLabel()} aria-label={removalLabel()} onClick={() => {
            const item = current();
            if (item && removable()) setReview({ handle: item.handle, requestId: item.request_id,
              skipScope: skippable() === item ? deliveryScope(item) : undefined,
              skipBinding: skippable() === item ? composer.skipWaitingBinding(item.handle, item.request_id) : undefined });
          }}><Show when={skippable()} fallback={<Icon name="trash" size={14} />}>{tr("Skip and continue…")}</Show></button>
      </div>
    </Show>
    <Show when={confirming()}>{(action) =>
      <div class="act-outbox-confirm" role="group" aria-label={action().skipScope ? tr("Skip and continue") : tr("Remove queued message")} onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          cancel();
        }
      }}>
        <p>{action().skipScope
          ? tr("This original will leave the Zerus queue without being sent again. It may already have reached the agent. Later queued messages can then proceed.")
          : tr("Remove this message from the Zerus queue? This cannot undo delivery to the agent.")}</p>
        <div class="act-local-actions">
          <button type="button" class="btn small" ref={(node) => queueMicrotask(() => {
            if (node.isConnected) node.focus();
          })} onClick={cancel}>{tr("Cancel")}</button>
          <button type="button" class="btn small" disabled={!removable()} onClick={() => {
            const bound = confirming();
            if (!bound || bound !== action() || !removable()) return;
            setReview(undefined);
            scope?.done();
            if (bound.skipScope) {
              if (!bound.skipBinding) return;
              void composer.removeWaiting(bound.handle, bound.requestId, { kind: "skip_unknown", binding: bound.skipBinding });
            } else {
              void composer.removeWaiting(bound.handle, bound.requestId);
            }
          }}>{action().skipScope ? tr("Skip and continue") : tr("Remove queued message")}</button>
        </div>
      </div>
    }</Show>
  </>;
}
