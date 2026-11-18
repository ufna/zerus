import { expect, it, vi } from "vitest";
import { waitingNote } from "./decisions";

vi.mock("../../i18n", async (importOriginal) => {
  const { translateWithCatalog } = await importOriginal<typeof import("../../i18n")>();
  const { default: catalog } = await import("../../i18n.ru.json");
  return { tr: (source: string, ...args: string[]) => translateWithCatalog(catalog, "ru", source, ...args) };
});

it("localizes the provider stop for stage 21 without displaying the English fallback", () => {
  expect(waitingNote({
    code: "provider_stop_quota",
    parameters: ["m3", "Kimi", "20", "21"],
    text: "English fallback",
  })).toBe("На этапе №21 задачи №20 участник m3 (Kimi) достиг лимита использования. Ревью возвращено лиду. Просмотрите изменения или выберите следующее действие.");
});

it("localizes a lead's informational staffing item", () => {
  expect(waitingNote({
    code: "lead_member_added",
    parameters: ["m4", "Codex", "reviewer"],
    text: "English fallback",
  })).toBe("Лид добавил участника m4 (Codex) в роль reviewer.");
});

it("names the stop and task for a standalone hand-back", () => {
  expect(waitingNote({
    code: "provider_stop_authentication",
    parameters: ["m2", "Claude", "19"],
  })).toBe("При работе над задачей №19 участник m2 (Claude) не смог войти в аккаунт провайдера. Задача возвращена лиду.");
});
