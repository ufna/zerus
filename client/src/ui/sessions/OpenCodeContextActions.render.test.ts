// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import type { Details } from "../../types";
import { detailsFromInspection, type ActionRow } from "./actions-ui";

const fixture = vi.hoisted(() => ({ language: "en" as "en" | "ru", perform: vi.fn() }));
vi.mock("../../i18n", async (original) => {
  const real = await original<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return { ...real, tr: (source: string, ...args: import("../../i18n").TranslationArgument[]) =>
    real.translateWithCatalog(catalog, fixture.language, source, ...args) };
});
vi.mock("../../store/actions", () => ({ perform: fixture.perform }));
import { OpenCodeContextActions } from "./OpenCodeContextActions";
import { usageRows } from "./inspector-data";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); fixture.perform.mockReset(); fixture.language = "en"; document.body.replaceChildren(); });

const row = { key: "fixture", name: "opencode/project/fixture", computer: "", runId: "original", conversationId: "ses_original" } as ActionRow;
const observed = (fields: Record<string, unknown> = {}) => detailsFromInspection({
  run_id: "original", conversation_id: "ses_original", compact_context_supported: true, ...fields,
})!;

function mount() {
  const [details, setDetails] = createSignal<Details>(observed());
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(OpenCodeContextActions, {
    row, get details() { return details(); },
  }), host);
  return { setDetails, host, buttons: () => Array.from(host.querySelectorAll<HTMLButtonElement>("button")) };
}

it.each([
  ["en", "Compact context…", "Clear session…", "summary may omit details", "Model context window", "Maximum output tokens"],
  ["ru", "Сжать контекст…", "Очистить сессию…", "детали могут быть опущены", "Контекстное окно модели", "Максимум токенов в ответе"],
] as const)("offers a scoped summary with separate read-only model limits in %s", (language, compact, clear, notice, window, output) => {
  fixture.language = language;
  const view = mount();
  expect(view.buttons().map(button => button.textContent)).toEqual([compact, clear]);
  expect(view.buttons()[1].disabled).toBe(true);
  expect(usageRows({ session_usage: { status: "unavailable", source: "opencode", context: { limit: 32768, output_limit: 2048 } } }))
    .toEqual([[window, (32768).toLocaleString()], [output, (2048).toLocaleString()]]);
  view.buttons()[0].click();
  expect(view.host.querySelector("dialog")?.textContent).toContain(notice);
  const confirm = view.host.querySelector<HTMLButtonElement>("dialog button:last-child")!;
  confirm.click();
  confirm.click();
  expect(fixture.perform).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    name: row.name, runId: "original", conversationId: "ses_original",
  }), { kind: "compact_context" });
});

it("refuses a stale or unsupported inspection before invoking Compact", () => {
  const view = mount();
  view.buttons()[0].click();
  view.setDetails(observed({ conversation_id: "ses_replaced" }));
  expect(view.host.querySelector<HTMLButtonElement>("dialog button:last-child")!.disabled).toBe(true);
  view.host.querySelector<HTMLButtonElement>("dialog button:last-child")!.click();
  expect(fixture.perform).not.toHaveBeenCalled();
  view.setDetails(observed({ compact_context_supported: false, compact_context_reason: "Manual compaction is unavailable on this OpenCode service." }));
  expect(view.buttons()[0].disabled).toBe(true);
  expect(view.buttons()[0].title).toContain("unavailable");
});
