// @vitest-environment happy-dom
// The selected approval journal entry's detail in Russian (zerus-f67qp
// review), through the real language switch and the shipped catalog: the
// core writes the detail in English (approvals-hook-trust.fixture.json is its
// output for the record hgs writes), and the journal shows the core's own
// sentences translated. The agent's question and answers stay as recorded.
import { beforeAll, expect, it } from "vitest";
import catalog from "../../../i18n.ru.json";
import hookTrust from "./approvals-hook-trust.fixture.json";
import page from "./approvals-page.r6.fixture.json";

let journal: typeof import("./approvals-journal");
beforeAll(async () => {
  const preferences = await import("../../../store/settings"); await preferences.settingsReady;
  await preferences.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../../i18n"); language.installUiLanguage();
  await new Promise<void>((resolve) => { const wait = () => language.uiLanguage() === "ru" ? resolve() : setTimeout(wait, 5); wait(); });
  journal = await import("./approvals-journal");
});

const ru = catalog as Record<string, string>;

it("a team's Codex hook-trust record reads in Russian, every line of it", () => {
  expect(journal.localizedDetail(hookTrust.approved).split("\n\n")).toEqual([
    "Одобрено один раз",
    "Доверие к хукам Codex записано до запуска для собственных хуков Zerus и хуков из коммита команды",
    "Доверять собственным хукам Zerus и хукам Codex из коммита команды",
    "Хуков для доверия: 8",
    "Codex сохранил доверие ровно к этим хукам",
  ]);
  expect(journal.localizedDetail(hookTrust.unknown).split("\n\n")).toEqual([
    ru["Unconfirmed"],
    "Доверие к хукам Codex запрошено, но не подтверждено",
    "Доверять собственным хукам Zerus и хукам Codex из коммита команды",
    "Хуков для доверия: 8",
    "Доверие, сохранённое в Codex, не совпадает с этими хуками",
  ]);
});

it("a folder-trust record's own sentences translate and its native question stays as recorded", () => {
  const entries = page.entries as { detail: string }[];
  expect(journal.localizedDetail(entries[0].detail).split("\n\n")).toEqual([
    "Одобрено один раз",
    ru["Trusted for %1 and its subfolders on this account"].replace("%1", "/fixture/main"),
    "Trust this folder?",
    "/fixture/main",
  ]);
  expect(journal.localizedDetail(entries[1].detail).split("\n\n")[0]).toBe("Сбой");
});

it("text the core did not write is never translated", () => {
  const recorded = "Approved once\n\nMay I run this command?\n\nprintf done\n\nSelected answer: Yes\n\nGranted once.";
  expect(journal.localizedDetail(recorded).split("\n\n")).toEqual([
    "Одобрено один раз", "May I run this command?", "printf done", "Выбранный ответ: Yes", "Granted once.",
  ]);
});

it("the journal shows a selected entry's detail through that translation", async () => {
  let shown: import("./approvals-journal").JournalState | undefined;
  const io = { list: async () => ({ entries: [], next: 0, previous: 0 }), detail: async () => hookTrust.approved };
  journal.createJournal(io, () => ({ host: "", session: "" }), (state) => { shown = state; }).select("1");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(shown?.detail?.split("\n\n")).toEqual(journal.localizedDetail(hookTrust.approved).split("\n\n"));
  expect(shown?.detail?.startsWith("Одобрено один раз")).toBe(true);
});
