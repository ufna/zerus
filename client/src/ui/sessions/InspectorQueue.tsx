import { For, Show, batch, createEffect, createMemo, onCleanup } from "solid-js";
import type { ComposerView, WaitingMessage } from "../../types";
import { composer } from "../../store/composer";
import { draftKey } from "../../store/draftKeys";
import { selected, subagentOpen, setSubagentOpen, setDetailTab } from "../../store/ui";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { shownSearchPreview } from "./Activity";
import type { Session } from "./types";
import { QUEUED_ORIGINAL_LIMIT, requestQueuedOriginalJump, sameQueueSelection, type QueuedOriginal, type QueuedOriginalJump } from "./queued-original-navigation";

type IndexRow = { original: QueuedOriginal; preview: string; state: WaitingMessage["state"] };
/** A bounded projection of the current native view, never retained delivery state. */
export function inspectorQueueRows(session: Session | undefined, view: ComposerView | null | undefined,
  waiting: readonly WaitingMessage[], opening: boolean, child: string | undefined): IndexRow[] {
  if (!session || session.state === "archived" || !session.runId || !session.conversationId
    || !view?.loaded || opening || child || view.subagent) return [];
  const key = draftKey({ computer: session.computer, key: session.catalogKey, name: session.name,
    session_id: session.sessionId ?? null, archive_id: null });
  if (view.key !== key) return [];
  const bounded = waiting.slice(0, QUEUED_ORIGINAL_LIMIT);
  return bounded.filter(row => !!row.request_id && !row.target_changed && row.run_id === session.runId
    && row.conversation_id === session.conversationId && bounded.filter(other => other.request_id === row.request_id).length === 1)
    .map(row => ({ original: { sessionKey: session.key, draftKey: key, handle: view.handle,
      runId: session.runId!, conversationId: session.conversationId!, requestId: row.request_id },
      preview: row.preview.slice(0, 240), state: row.state }));
}
const stateLabel = (state: WaitingMessage["state"]) => state === "unknown" ? tr("Delivery not confirmed")
  : state === "reserved" ? tr("Sending…") : state === "failed" ? tr("Not sent") : tr("Queued in Zerus");

export function InspectorQueue() {
  const rows = createMemo(() => inspectorQueueRows(selected(), composer.view(), composer.waitingMessages(), composer.opening(), subagentOpen()));
  let jump: QueuedOriginalJump | undefined;
  const current = (original: QueuedOriginal) => !shownSearchPreview() && rows().some(row => row.original.requestId === original.requestId
    && sameQueueSelection(row.original, original));
  createEffect(() => {
    rows(); const preview = shownSearchPreview();
    if (jump && (preview || !current(jump.original))) jump.cancel();
  });
  onCleanup(() => jump?.cancel());
  const open = (original: QueuedOriginal) => {
    if (!current(original)) return;
    batch(() => {
      jump?.cancel();
      jump = requestQueuedOriginalJump(original, () => current(original));
      if (jump) { setSubagentOpen(undefined); setDetailTab("activity"); }
    });
  };
  return <Show when={rows().length}>
    <section class="icard" data-queue-index aria-label={tr("Queued in Zerus")}>
      <header class="icard-head"><span class="icard-toggle"><span class="icard-title">{tr("Queued in Zerus")}</span></span><span class="icard-aside">{rows().length}</span></header>
      <div class="icard-body"><ol style={{ margin: "0", padding: "0", "list-style": "none" }}>
        <For each={rows()}>{(row, index) => <li data-queue-request={row.original.requestId}>
          <button type="button" class="irow" disabled={!!shownSearchPreview()} title={tr("Activity")} aria-label={`${tr("Activity")}: ${index() + 1}: ${row.preview || tr("Message with files")}`}
            onClick={() => open(row.original)}>
            <span class="irow-ico">{index() + 1}</span><span class="irow-val ell">{row.preview || tr("Message with files")}</span>
            <span class="irow-label">{stateLabel(row.state)}</span><Icon name="arrow-up-right" size={13} />
          </button>
        </li>}</For>
      </ol></div>
    </section>
  </Show>;
}
