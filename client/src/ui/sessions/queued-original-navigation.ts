import { createEffect, createSignal, onCleanup } from "solid-js";
import type { WaitingMessage } from "../../types";

/** Same bound as core drafts/outbox.rs; a navigation intent is never a queue. */
export const QUEUED_ORIGINAL_LIMIT = 16;
export interface QueueSelection {
  sessionKey: string;
  draftKey: string;
  handle: number;
  runId: string;
  conversationId: string;
}
export interface QueuedOriginal extends QueueSelection { requestId: string }
export const sameQueueSelection = (a: QueueSelection | undefined, b: QueueSelection) => !!a
  && a.sessionKey === b.sessionKey && a.draftKey === b.draftKey && a.handle === b.handle
  && a.runId === b.runId && a.conversationId === b.conversationId;
export interface QueuedOriginalJump {
  readonly original: QueuedOriginal;
  current(): boolean;
  cancel(): void;
}
const [pending, setPending] = createSignal<QueuedOriginalJump>();

/** One transient view intent. No delivery state, native operation or older read. */
export function requestQueuedOriginalJump(original: QueuedOriginal, current: () => boolean): QueuedOriginalJump | undefined {
  if (!current()) return;
  pending()?.cancel();
  const [cancelled, setCancelled] = createSignal(false);
  const request: QueuedOriginalJump = {
    original: { ...original }, current: () => !cancelled() && current(),
    cancel: () => { setCancelled(true); if (pending() === request) setPending(undefined); },
  };
  setPending(request);
  return request;
}

/** Activity supplies this small callback boundary beside its existing named-row reveal. */
export function createQueuedOriginalNavigation(port: {
  selection(): QueueSelection | undefined;
  originals(): readonly WaitingMessage[];
  ready(): boolean;
  reveal(id: string, current: () => boolean): () => void;
}) {
  let active: { request: QueuedOriginalJump; cancel: () => void } | undefined;
  const contains = (request: QueuedOriginalJump, rows: readonly WaitingMessage[]) => {
    const matches = rows.slice(0, QUEUED_ORIGINAL_LIMIT).filter(row => row.request_id === request.original.requestId);
    return matches.length === 1 && !matches[0].target_changed
      && matches[0].run_id === request.original.runId && matches[0].conversation_id === request.original.conversationId;
  };
  createEffect(() => {
    const request = pending(), selection = port.selection(), rows = port.originals();
    if (active && (!active.request.current() || !sameQueueSelection(selection, active.request.original) || !contains(active.request, rows))) {
      active.cancel(); active = undefined;
    }
    if (!request) return;
    if (!request.current()) { request.cancel(); return; }
    if (!sameQueueSelection(selection, request.original) || !port.ready()) return;
    if (!contains(request, rows)) { request.cancel(); return; }
    setPending(undefined);
    active?.cancel();
    active = { request, cancel: port.reveal(`local-${request.original.requestId}`, request.current) };
  });
  onCleanup(() => {
    active?.cancel();
    const request = pending();
    if (request && sameQueueSelection(port.selection(), request.original)) request.cancel();
  });
}
