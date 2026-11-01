import { createSignal } from "solid-js";

// The webview derives the page's visibility from its window. The batch pump
// (bridge.ts) and Working clocks (store/ui.ts) use only that document state;
// motion also pauses on focus loss because a Wayland window does not report
// minimisation and the GL path otherwise keeps painting it.

export interface PageVisibility {
  hidden(): boolean;
  /** Calls `listener` after every change; returns the unsubscribe. */
  onChange(listener: () => void): () => void;
}

export const documentVisibility: PageVisibility = {
  hidden: () => typeof document !== "undefined" && document.visibilityState === "hidden",
  onChange(listener) {
    if (typeof document === "undefined") return () => {};
    document.addEventListener("visibilitychange", listener);
    return () => document.removeEventListener("visibilitychange", listener);
  },
};

/** Hidden when any source is hidden; listeners fire only when the combined state changes. */
export function anyHidden(...sources: PageVisibility[]): PageVisibility {
  const hidden = () => sources.some((source) => source.hidden());
  return {
    hidden,
    onChange(listener) {
      let last = hidden();
      const changed = () => {
        const current = hidden();
        if (current === last) return;
        last = current;
        listener();
      };
      const stops = sources.map((source) => source.onChange(changed));
      return () => { for (const stop of stops) stop(); };
    },
  };
}

const windowFocus: PageVisibility = {
  hidden: () => typeof document !== "undefined" && typeof document.hasFocus === "function" && !document.hasFocus(),
  onChange(listener) {
    if (typeof window === "undefined" || typeof window.addEventListener !== "function" || typeof window.removeEventListener !== "function") return () => {};
    window.addEventListener("focus", listener);
    window.addEventListener("blur", listener);
    return () => {
      window.removeEventListener("focus", listener);
      window.removeEventListener("blur", listener);
    };
  },
};

export const motionVisibility = anyHidden(documentVisibility, windowFocus);
const [paused, setPaused] = createSignal(motionVisibility.hidden());
motionVisibility.onChange(() => setPaused(motionVisibility.hidden()));

/** Reactive: pause motion while the window is hidden, minimised or unfocused. */
export const motionPaused = paused;

// Data subscriptions, selection and media still use document visibility only.
const [hidden, setHidden] = createSignal(documentVisibility.hidden());
documentVisibility.onChange(() => setHidden(documentVisibility.hidden()));

/** Reactive: true while the document is hidden. */
export const pageHidden = hidden;

/**
 * Whether the window is the active one (Qt's isActiveWindow): the webview's
 * document has the focus. Not reactive; window `focus` and `blur` say when it
 * changes.
 */
export const documentFocused = () => typeof document !== "undefined" && document.hasFocus();
