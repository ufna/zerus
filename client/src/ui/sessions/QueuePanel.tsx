import { For, Show, createMemo } from "solid-js";
import { composer } from "../../store/composer";
import { activity } from "../../store/session";
import { selected, subagentOpen } from "../../store/ui";
import { chord } from "../../store/shortcuts";
import { tr, trNoop } from "../../i18n";
import { Icon } from "../lib/icons";
import { OutgoingMessage } from "./Activity";
import { queuePresentation } from "./queue-message";

const SEND_HINTS = [
  trNoop("Send the queued messages now"),
  trNoop("Steer the agent with the queued messages now"),
  trNoop("Interrupt the current turn and send pending messages now"),
];
const TERMINAL_HINTS = [
  trNoop("Use Terminal to edit or submit these queued follow-ups"),
  trNoop("Return Terminal to an empty input before sending the queue now"),
];

function queueHint(hint: string, canSendNow: boolean): string {
  const source = (canSendNow ? SEND_HINTS : TERMINAL_HINTS).find((known) => known === hint);
  if (source) return tr(source);
  return canSendNow ? hint : tr("Check Terminal for the queued messages.");
}

/** Native pending messages remain distinct from the sent Activity. */
export function QueuePanel() {
  const display = createMemo(() => {
    const queue = composer.queue();
    return queue ? queuePresentation(queue, selected(), composer.view(), activity.target(), activity.summary(), !!subagentOpen()) : undefined;
  }, undefined, { equals: (a, b) => JSON.stringify(a?.items) === JSON.stringify(b?.items)
    && JSON.stringify(a?.scope) === JSON.stringify(b?.scope) });
  const tooltip = (hint: string, canSendNow: boolean) => {
    const keys = canSendNow ? chord("queue.sendNow") : undefined;
    return keys ? hint ? `${hint} (${keys})` : keys : hint;
  };
  return (
    <Show when={composer.queue()}>{(q) => (
      <div class="act-queue" role="region" aria-label={tr("Queued in agent")}>
        <div class="act-queue-head">
          <Icon name="clock" size={15} /><b>{tr("Queued in agent")}</b><span class="grow" />
          <button type="button" class="btn small" disabled={!q().enabled} title={tooltip(queueHint(q().hint, q().can_send_now), q().can_send_now)}
            onClick={() => { void composer.sendNow(q().id); }}>{q().label === "Sending…" ? tr("Sending…") : tr("Send queued messages now")}</button>
        </div>
        <Show when={!q().can_send_now}>
          <div class="act-queue-source">{queueHint(q().hint, false)}</div>
        </Show>
        <div class="act-queue-text">
          <For each={display()?.items}>{(item) => <div class="act-queue-message">
            <Show when={item.sender}>{(sender) => <div class="act-queue-source">{tr("Sender shown by agent: %1", sender())}</div>}</Show>
            <OutgoingMessage item={item} scope={display()?.scope} session={display()?.scope ? selected() : undefined} />
          </div>}</For>
        </div>
      </div>
    )}</Show>
  );
}
