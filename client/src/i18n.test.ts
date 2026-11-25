import { describe, expect, it } from "vitest";
import { preferredLanguage, translateWithCatalog, type TranslationArgument, type UiLanguage } from "./i18n";
import catalog from "./i18n.ru.json";

const translate = (language: UiLanguage, source: string, ...args: TranslationArgument[]) =>
  translateWithCatalog(catalog, language, source, ...args);

describe("frontend interface language", () => {
  it("defaults to English except for the explicit Russian choice", () => {
    expect(preferredLanguage(undefined)).toBe("en");
    expect(preferredLanguage("system")).toBe("en");
    expect(preferredLanguage("de")).toBe("en");
    expect(preferredLanguage("ru")).toBe("ru");
  });

  it("uses reviewed Russian entries and keeps English for missing entries", () => {
    expect(translate("ru", "Interface language")).toBe("Язык интерфейса");
    expect(translate("ru", "Russian (preview)")).toBe("Русский (предпросмотр)");
    expect(translate("ru", "A Next-only label")).toBe("A Next-only label");
    expect(translate("en", "Interface language")).toBe("Interface language");
  });

  it("substitutes Qt placeholders without translating the arguments", () => {
    expect(translate("ru", "Label color for %1", "workstation")).toBe("Цвет метки для workstation");
    expect(translate("en", "Label color for %1", "workstation")).toBe("Label color for workstation");
  });

  it("uses the archive action wording in Russian and keeps the Archive destination a noun", () => {
    expect(translate("ru", "Stop and archive…")).toBe("Остановить и архивировать…");
    expect(translate("ru", "session action\u0004Archive")).toBe("В архив");
    expect(translate("ru", "Archive")).toBe("Архив");
    expect(translate("en", "Stop and archive…")).toBe("Stop and archive…");
  });

  it("uses all three Russian plural forms", () => {
    const source = "%n connection(s) unavailable. Changes are saved here and will sync when reachable.";
    expect(translate("ru", source, 1)).toContain("1 подключение ");
    expect(translate("ru", source, 2)).toContain("2 подключения ");
    expect(translate("ru", source, 5)).toContain("5 подключений ");
    expect(translate("ru", source, 11)).toContain("11 подключений ");
    expect(translate("ru", source, 21)).toContain("21 подключение ");
  });
});
