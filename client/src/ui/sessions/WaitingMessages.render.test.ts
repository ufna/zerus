// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { WaitingMessage } from "../../types";

const f = vi.hoisted(() => ({ language: "en" as "en" | "ru", remove: vi.fn(), set: (_handle: number, _rows: WaitingMessage[]) => {} }));
vi.mock("../../i18n", async (original) => {
  const module = await original<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return { ...module, uiLanguage: () => f.language,
    tr: (source: string, ...args: import("../../i18n").TranslationArgument[]) => module.translateWithCatalog(catalog, f.language, source, ...args) };
});
vi.mock("../../store/composer", async () => {
  const { createSignal } = await import("solid-js");
  const [handle, setHandle] = createSignal(0);
  const [rows, setRows] = createSignal<WaitingMessage[]>([]);
  f.set = (next, items) => { setHandle(next); setRows(items); };
  return { composer: { view: () => ({ handle: handle() }), waitingMessages: rows,
    operationLocked: () => false, removeWaiting: f.remove } };
});
import { WaitingMessages } from "./WaitingMessages";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); f.remove.mockReset(); f.language = "en"; document.body.replaceChildren(); });

it("shows local delivery doubt separately and confirms removal of the exact selected waiting item", () => {
  const original: WaitingMessage = { request_id: "a", preview: "one", state: "unknown" };
  const successor: WaitingMessage = { request_id: "b", preview: "two", state: "waiting" };
  f.set(1, [original, successor]);
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(WaitingMessages, host);
  expect(host.querySelector("summary")?.textContent).toBe("Zerus queue paused (2)");
  expect(host.textContent).toContain("Delivery unknown. Check Terminal before removing this message.");
  expect(host.textContent).not.toContain("Send now");
  const ask = () => host.querySelector<HTMLButtonElement>("li button")!;
  const confirm = () => host.querySelector<HTMLButtonElement>("[role=group] button:last-child");
  ask().click();
  expect(confirm()).not.toBeNull();
  f.set(2, [successor]);
  expect(confirm()).toBeNull();
  expect(f.remove).not.toHaveBeenCalled();
  expect(host.querySelector("summary")?.textContent).toBe("Waiting in Zerus (1)");
  ask().click();
  confirm()!.click();
  expect(f.remove).toHaveBeenCalledExactlyOnceWith(2, "b");
});

it.each([
  ["en", "Queued in Zerus at", "26 Nov"],
  ["ru", "Поставлено в очередь Zerus", "26 ноября"],
] as const)("shows the recorded Zerus enqueue time in %s and omits it for legacy messages", (language, label, date) => {
  const at = new Date(2026, 10, 26, 14, 8, 9);
  const known = { request_id: "new", preview: "same", state: "unknown", queued_at: at.getTime() / 1000 } as const;
  const legacy = { request_id: "legacy", preview: "same", state: "waiting" } as const;
  f.language = language;
  f.set(1, [known, legacy]);
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(WaitingMessages, host);
  const first = host.querySelector("li time");
  expect(first?.getAttribute("datetime")).toBe(at.toISOString());
  expect(first?.textContent).toContain(label);
  expect(first?.textContent).toContain(date);
  expect(first?.textContent).toContain("14:08:09");
  expect(host.querySelector("li:nth-child(2) time")).toBeNull();
});
