import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js";

export interface TeamScope { key: string; team?: string }
/** One in-flight follow and one latest desired scope, never an unbounded queue.
 * A stale follow must settle before its replacement; it never publishes a view.
 * Unmount retires even an in-flight follow after it settles. */
export function followTeamScope(desired: Accessor<TeamScope>, port: { close: () => Promise<unknown>; open: (team: string) => Promise<unknown> }) {
  const [ready, setReady] = createSignal<TeamScope>();
  const [failure, setFailure] = createSignal("");
  let wanted: TeamScope, active = true, running = false;
  const flush = async () => {
    if (running) return;
    running = true;
    try {
      while (active) {
        const next = wanted;
        try {
          await port.close();
          if (!active) break;
          if (next !== wanted) continue;
          if (next.team) await port.open(next.team);
          if (active && next === wanted) { setReady(next); break; }
        } catch (error) {
          if (active && next === wanted) { setFailure(String(error)); break; }
        }
      }
    } finally {
      running = false;
      if (!active) void port.close().catch(() => {});
    }
  };
  createEffect(() => {
    wanted = desired();
    setReady(undefined); setFailure("");
    void flush();
  });
  onCleanup(() => { active = false; if (!running) void port.close().catch(() => {}); });
  return { ready, failure };
}
