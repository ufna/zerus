// @vitest-environment happy-dom
import { createRoot, createSignal } from "solid-js";
import { expect, it, vi } from "vitest";
import { createQueuedOriginalNavigation, requestQueuedOriginalJump, type QueueSelection } from "./queued-original-navigation";

it("Plan queue navigation refuses stale selection/identity and cancels an unpainted reveal", () => {
  let dispose!: () => void;
  const selection: QueueSelection = { sessionKey: "local\nrun:first", draftKey: "local\nid:fixture", handle: 5, runId: "first", conversationId: "conversation" };
  const [current, setCurrent] = createSignal(true), [ready, setReady] = createSignal(false);
  const [bound, setBound] = createSignal<QueueSelection | undefined>(selection);
  const [rows, setRows] = createSignal([{ request_id: "original", preview: "same", state: "unknown" as const, run_id: "first", conversation_id: "conversation" }]);
  const reveal = vi.fn(() => vi.fn());
  createRoot(stop => { dispose = stop; createQueuedOriginalNavigation({ selection: bound, originals: rows, ready, reveal }); });
  try {
    const stale = requestQueuedOriginalJump({ ...selection, requestId: "original" }, current)!;
    setCurrent(false); setReady(true);
    expect(reveal).not.toHaveBeenCalled();
    expect(stale.current()).toBe(false);
    expect(requestQueuedOriginalJump({ ...selection, requestId: "original" }, current)).toBeUndefined();
    setCurrent(true);
    requestQueuedOriginalJump({ ...selection, requestId: "original" }, current);
    expect(reveal).toHaveBeenCalledExactlyOnceWith("local-original", expect.any(Function));
    setBound({ ...selection, conversationId: "successor" });
    expect(reveal.mock.results[0].value).toHaveBeenCalledTimes(1);
    setRows([{ ...rows()[0], conversation_id: "foreign" }]);
    requestQueuedOriginalJump({ ...selection, conversationId: "successor", requestId: "original" }, current);
    expect(reveal).toHaveBeenCalledTimes(1);
  } finally { dispose(); }
});
