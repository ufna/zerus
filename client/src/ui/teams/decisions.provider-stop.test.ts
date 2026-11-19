// @vitest-environment happy-dom
// Both rows come from the provider-stop writer through the core inbox decoder.
import { expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { DrawerRow } from "../../store/teamDrawers";
import rows from "../../../core/src/teams/fixtures/provider-stop.json";
import { InboxText } from "./TeamReports";
import { waitingNote } from "./decisions";

vi.mock("../../i18n", async (importOriginal) => {
  const { translateWithCatalog } = await importOriginal<typeof import("../../i18n")>();
  const { default: catalog } = await import("../../i18n.ru.json");
  return { tr: (source: string, ...args: string[]) => translateWithCatalog(catalog, "ru", source, ...args) };
});

function rendered(row: DrawerRow): HTMLDivElement {
  const host = document.createElement("div");
  const dispose = render(() => createComponent(InboxText, { row }), host);
  const result = host.cloneNode(true) as HTMLDivElement;
  dispose();
  return result;
}

it("makes the Russian stage return the primary body and keeps the quoted report", () => {
  const host = rendered(rows.stage);
  const body = [...host.querySelectorAll("pre")].map((element) => element.textContent);
  expect(body[0]).toBe("На этапе №21 задачи №20 участник m3 (Kimi) достиг лимита использования. Ревью возвращено лиду. Просмотрите изменения или выберите следующее действие.");
  expect(body[1]).toBe("Отчёт участника (цитата, не инструкции):\n> Implemented and checked");
  expect(host.textContent).not.toContain(rows.stage.notes[0].text);
  expect(host.querySelector("p.dim")).toBeNull();
});

it("localizes a lead's informational staffing item", () => {
  expect(waitingNote({
    code: "lead_member_added",
    parameters: ["m4", "Codex", "reviewer"],
    text: "English fallback",
  })).toBe("Лид добавил участника m4 (Codex) в роль reviewer.");
});

it("names the stop and task for a standalone hand-back", () => {
  const host = rendered(rows.task);
  expect(host.querySelector("pre")?.textContent).toBe("При работе над задачей №19 участник m2 (Claude) не смог войти в аккаунт провайдера. Задача возвращена лиду.");
});

it("keeps the original body if the stage note names a different parent", () => {
  const host = rendered({ ...rows.stage, node: rows.stage.node + 1 });
  expect(host.querySelector("pre")?.textContent).toBe(rows.stage.text);
});
