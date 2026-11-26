import { createSignal, For, Show } from "solid-js";
import { tr } from "../../i18n";
import { composer } from "../../store/composer";
import { setDetailTab } from "../../store/ui";
import type { WaitingMessage } from "../../types";
import { askedTimeOf, zonedTimeOf } from "../lib/dates";

const queuedDate = (at?: number) => at !== undefined && at > 0 && at < 253_402_300_800 ? new Date(at * 1000) : undefined;

const stateText = (message: WaitingMessage) => {
  switch (message.state) {
    case "waiting": return message.target_changed ? tr("The original session changed.") : tr("Waiting for the agent to become ready");
    case "reserved": return tr("Checking delivery");
    case "unknown": return tr("Delivery unknown. Check Terminal before removing this message.");
    case "failed": return tr("The agent did not accept this message.");
  }
};

export function WaitingMessages() {
  const [review, setReview] = createSignal<{ handle: number; requestId: string }>();
  const rows = composer.waitingMessages;
  const confirming = () => {
    const action = review();
    return action && action.handle === composer.view()?.handle
      && rows().some(row => row.request_id === action.requestId) ? action : undefined;
  };
  const paused = () => {
    const head = rows()[0];
    return head?.target_changed || head?.state === "failed" || head?.state === "unknown";
  };
  return <Show when={rows().length}>
    <details class="cmp-draft-status">
      <summary>{paused() ? tr("Zerus queue paused (%1)", rows().length) : tr("Waiting in Zerus (%1)", rows().length)}</summary>
      <ol>
        <For each={rows()}>{row =>
          <li>
            <span>{row.preview || tr("Message with files")}</span>
            <Show when={queuedDate(row.queued_at)}>{(at) =>
              <div><time dateTime={at().toISOString()} title={zonedTimeOf(at())}>{tr("Queued in Zerus at %1", askedTimeOf(at()))}</time></div>
            }</Show>
            <div role="status">{stateText(row)}</div>
            <Show when={row.can_check_terminal}>
              <button type="button" class="btn ghost" onClick={() => {
                if (rows().some(current => current.request_id === row.request_id && current.can_check_terminal)) {
                  setDetailTab("terminal");
                }
              }}>{tr("Check Terminal")}</button>
            </Show>
            <button type="button" class="btn ghost" disabled={composer.operationLocked() || rows()[0]?.state === "reserved"}
              onClick={() => setReview({ handle: composer.view()!.handle, requestId: row.request_id })}>
              {tr("Remove from Zerus queue…")}
            </button>
          </li>
        }</For>
      </ol>
      <Show when={confirming()}>{(action) =>
        <div role="group" aria-label={tr("Remove queued message")}>
          <p>{tr("Remove this message from the Zerus queue? This cannot undo delivery to the agent.")}</p>
          <button type="button" class="btn ghost" onClick={() => setReview(undefined)}>{tr("Cancel")}</button>
          <button type="button" class="btn" disabled={composer.operationLocked() || rows()[0]?.state === "reserved"}
            onClick={() => {
              const bound = action();
              setReview(undefined);
              void composer.removeWaiting(bound.handle, bound.requestId);
            }}>{tr("Remove queued message")}</button>
        </div>
      }</Show>
    </details>
  </Show>;
}
