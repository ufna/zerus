import { expect, it, vi } from "vitest";
import { inboxText } from "./feed";
import notice from "./fixtures/unreadable-notice.json";
import stageNotice from "./fixtures/unreadable-stage-notice.json";

const locale = vi.hoisted(() => ({ language: "en" as "en" | "ru" }));
vi.mock("../../i18n", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../i18n")>();
  const catalog = (await import("../../i18n.ru.json")).default;
  return {
    ...original,
    tr: (source: string, ...args: (string | number)[]) =>
      original.translateWithCatalog(catalog, locale.language, source, ...args),
  };
});

// Produced by export_teams81_readability_store using the real actor,
// Teams inbox writer and client drawer decoder.
it("localizes the unreadable staffing notice with its original member IDs", () => {
  locale.language = "en";
  expect(inboxText(notice)).toBe(notice.text);
  locale.language = "ru";
  expect(inboxText(notice)).toBe(
    "Задача 1 готова к выдаче. Службе недоступны данные всех подходящих участников (m1, m2). Служба повторит проверку.",
  );
});

// The pipeline writer and the client drawer decoder produce this row.
it("labels an unreadable review stage in English and Russian", () => {
  locale.language = "en";
  expect(inboxText(stageNotice)).toBe(
    "Pipeline stage 2 is ready. The service cannot read any eligible member (m3). It will check again.",
  );
  locale.language = "ru";
  expect(inboxText(stageNotice)).toBe(
    "Этап конвейера 2 готов к выдаче. Службе недоступны данные всех подходящих участников (m3). Служба повторит проверку.",
  );
});

it("preserves raw text for a different kind or a mismatched task identity", () => {
  locale.language = "ru";
  expect(inboxText({ ...notice, kind: "question:1" })).toBe(notice.text);
  expect(inboxText({ ...notice, node: 2 })).toBe(notice.text);
  expect(inboxText({ ...notice, text_truncated: true })).toBe(notice.text);
});
