import { describe, expect, it, vi } from "vitest";
import { anyHidden, type PageVisibility } from "./visibility";

function source(initial = false) {
  let hidden = initial;
  const listeners = new Set<() => void>();
  const visibility: PageVisibility = {
    hidden: () => hidden,
    onChange(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
  return {
    visibility,
    listeners,
    setHidden(value: boolean) {
      hidden = value;
      for (const listener of [...listeners]) listener();
    },
  };
}

describe("anyHidden", () => {
  it("pauses motion on blur without hiding Activity, media or data subscriptions", async () => {
    let focused = true;
    const doc = Object.assign(new EventTarget(), {
      visibilityState: "visible",
      hasFocus: () => focused,
    });
    const win = new EventTarget();
    vi.stubGlobal("document", doc);
    vi.stubGlobal("window", win);
    vi.resetModules();
    try {
      const { pageHidden, motionPaused } = await import("./visibility");
      expect([pageHidden(), motionPaused()]).toEqual([false, false]);
      focused = false;
      win.dispatchEvent(new Event("blur"));
      expect([pageHidden(), motionPaused()]).toEqual([false, true]);
      doc.visibilityState = "hidden";
      doc.dispatchEvent(new Event("visibilitychange"));
      focused = true;
      win.dispatchEvent(new Event("focus"));
      expect([pageHidden(), motionPaused()]).toEqual([true, true]);
      doc.visibilityState = "visible";
      doc.dispatchEvent(new Event("visibilitychange"));
      expect([pageHidden(), motionPaused()]).toEqual([false, false]);
    } finally {
      vi.unstubAllGlobals();
      vi.resetModules();
    }
  });

  it("keeps motion paused until both document visibility and focus allow it", () => {
    const documentState = source();
    const focus = source();
    const combined = anyHidden(documentState.visibility, focus.visibility);
    expect(combined.hidden()).toBe(false);
    focus.setHidden(true);
    expect(combined.hidden()).toBe(true);
    documentState.setHidden(true);
    focus.setHidden(false);
    expect(combined.hidden()).toBe(true);
    documentState.setHidden(false);
    expect(combined.hidden()).toBe(false);
  });

  it("reports only combined transitions and unsubscribes from both", () => {
    const documentState = source();
    const focus = source();
    const combined = anyHidden(documentState.visibility, focus.visibility);
    let calls = 0;
    const stop = combined.onChange(() => { calls++; });
    documentState.setHidden(true);
    focus.setHidden(true);
    documentState.setHidden(false);
    expect(calls).toBe(1);
    focus.setHidden(false);
    expect(calls).toBe(2);
    expect(documentState.listeners.size).toBe(1);
    expect(focus.listeners.size).toBe(1);
    stop();
    expect(documentState.listeners.size).toBe(0);
    expect(focus.listeners.size).toBe(0);
    documentState.setHidden(true);
    focus.setHidden(true);
    expect(calls).toBe(2);
  });
});
