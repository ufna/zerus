import { Show } from "solid-js";
import { composer } from "../../store/composer";
import { chord } from "../../store/shortcuts";
import { tr } from "../../i18n";

/** The native queue, plain text, separate from local sends (ActivityView.cpp:1280-1292,1890-1896). */
export function QueuePanel() {
  const tooltip = () => {
    const hint = composer.queue()?.hint ?? "", keys = chord("queue.sendNow");
    // Qt's queueSendTooltip (1477-1486), including customized shortcuts.
    return keys ? hint ? `${hint} (${keys})` : keys : hint;
  };
  return (
    <Show when={composer.queue()}>{(q) => (
      <div class="act-queue" role="region" aria-label={tr("Queued in agent")}>
        <div class="act-queue-head">
          <b>{tr("Queued in agent")}</b><span class="grow" />
          <button type="button" class="btn small" disabled={!q().enabled} title={tooltip()}
            onClick={() => { void composer.sendNow(q().id); }}>{q().label}</button>
        </div>
        <div class="act-queue-text wrap-pre" title={q().text}>{q().text}</div>
      </div>
    )}</Show>
  );
}
