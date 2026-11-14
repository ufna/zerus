import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { ActivitySource } from "../../store/activitySource";
import type { TranscriptItem } from "./types";

export type OwnMessageDirection = -1 | 1;
const own = (item: TranscriptItem) => !item.inherited && (item.kind === "user" || item.kind === "media");

/** One mounted Activity owns the cursor. Older reads use its existing page
 * protocol, one request at a time, stopping at the first matching row. */
export function createOwnMessageNavigation(port: {
  source(): ActivitySource;
  items(): readonly TranscriptItem[];
  pivot(): number;
  available(): boolean;
  begin(): void;
  reveal(id: string): void;
}) {
  const [cursor, setCursor] = createSignal<string>();
  const [searching, setSearching] = createSignal(false);
  const [highlight, setHighlight] = createSignal<string>();
  const [position, setPosition] = createSignal(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0, waiting = false;
  const reset = () => {
    generation++; waiting = false; clearTimeout(timer);
    setSearching(false); setCursor(undefined); setHighlight(undefined);
    setPosition(n => n + 1);
  };
  createEffect(() => { port.source().target()?.id; reset(); });
  onCleanup(reset);
  const candidate = (direction: OwnMessageDirection) => {
    position();
    const items = port.items(), id = cursor();
    const index = id === undefined ? -1 : items.findIndex(item => item.id === id);
    const pivot = index >= 0 ? index : port.pivot();
    for (let i = direction < 0 ? pivot - 1 : index >= 0 ? pivot + 1 : pivot; i >= 0 && i < items.length; i += direction) {
      if (own(items[i])) return items[i].id;
    }
  };
  const previous = createMemo(() => port.available() && !searching() &&
    (candidate(-1) !== undefined || (!port.source().olderDone() && port.source().events().length > 0)));
  const next = createMemo(() => port.available() && !searching() && candidate(1) !== undefined);
  const show = (id: string) => {
    setSearching(false); setCursor(id); setHighlight(id);
    clearTimeout(timer); timer = setTimeout(() => setHighlight(undefined), 1600);
    port.reveal(id);
  };
  createEffect(() => {
    if (!searching()) return;
    const source = port.source(), loading = source.loadingOlder();
    const id = candidate(-1), done = source.olderDone(), error = source.error();
    if (loading || waiting) return;
    if (!port.available() || error) { setSearching(false); return; }
    if (id !== undefined) { show(id); return; }
    if (done) { setSearching(false); return; }
    // Defer to avoid starting a second read inside the page's publication.
    // A source that refuses a read must not spin the UI in a microtask loop.
    waiting = true;
    const mine = generation;
    queueMicrotask(() => {
      if (mine !== generation || !searching()) return;
      waiting = false;
      const found = candidate(-1);
      if (found !== undefined) { show(found); return; }
      if (!port.available() || source.olderDone() || source.error()) { setSearching(false); return; }
      if (source.loadingOlder()) return;
      source.loadOlder();
      if (!source.loadingOlder()) setSearching(false);
    });
  });
  return {
    previous, next, loading: searching, highlight, reset,
    moved() { reset(); },
    navigate(direction: OwnMessageDirection) {
      if (!(direction < 0 ? previous() : next())) return;
      port.begin();
      const id = candidate(direction);
      if (id !== undefined) show(id);
      else setSearching(true);
    },
  };
}

export type OwnMessageNavigation = ReturnType<typeof createOwnMessageNavigation>;
export const [ownMessageNavigation, setOwnMessageNavigation] = createSignal<OwnMessageNavigation>();
