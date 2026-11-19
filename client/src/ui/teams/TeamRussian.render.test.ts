// @vitest-environment happy-dom
// The Team tab in Russian (zerus-rzvct G7), through the real language switch
// and the shipped catalog: stage chips are nouns («Ревью», «Тест»), role chips
// translate the built-in roles, drawer rows translate their state and actor,
// and the team's lead is «лид» everywhere, never «руководитель».
import { afterEach, beforeAll, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import pages from "../../../core/src/teams/fixtures/drawers.json";
import providerStop from "../../../core/src/teams/fixtures/provider-stop-feed.json";
import memberRemoved from "../../../core/src/teams/fixtures/member-removed-feed.json";
import land from "../../../core/src/teams/fixtures/landing-drawers.json";
import catalog from "../../i18n.ru.json";
import type { DrawerPage } from "../../store/teamDrawers";
import type { WaitingItem } from "../../store/teams";
import notice from "./fixtures/unreadable-notice.json";
import stageNotice from "./fixtures/unreadable-stage-notice.json";
import oldDeepSeekCards from "./fixtures/deepseek-host-cards-legacy.json";
import deepSeekCards from "./fixtures/deepseek-host-cards.json";

let chips: typeof import("./TeamChips");
let drawers: typeof import("./TeamDrawers");
let reports: typeof import("./TeamReports");
let waiting: typeof import("./Waiting");
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
beforeAll(async () => {
  const preferences = await import("../../store/settings"); await preferences.settingsReady;
  await preferences.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n"); language.installUiLanguage();
  await new Promise<void>((resolve) => { const wait = () => language.uiLanguage() === "ru" ? resolve() : setTimeout(wait, 5); wait(); });
  chips = await import("./TeamChips"); drawers = await import("./TeamDrawers"); reports = await import("./TeamReports");
  waiting = await import("./Waiting");
});
const mount = (child: () => import("solid-js").JSX.Element) => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(child, host); return host;
};
const stage = (kind: string) => ({ node: 2, kind, role: kind === "test" ? "tester" : "reviewer", state: "accepted", round: 1, target: 1 });

it.each([
  [deepSeekCards.member, "удалите этого участника и добавьте нового"],
  [deepSeekCards.lead, "остановите эту команду и создайте новую"],
  [oldDeepSeekCards.member, "удалите этого участника и добавьте нового"],
  [oldDeepSeekCards.lead, "остановите эту команду и создайте новую"],
] as const)("localizes the DeepSeek host refusal for $id", (row, action) => {
  const host = mount(() => createComponent(waiting.WaitingCard, {
    item: row as WaitingItem, teamName: "Исправления", now: 1234, busy: false,
    onDecide: () => {}, onOpenTeam: () => {},
  }));
  expect(host.querySelector("[data-kind-sentence]")?.textContent).toBe(
    `Для участника ${row.parameters[0][1]} нужен хост DeepSeek с поддержкой Agent Teams.`,
  );
  expect(host.querySelector(".waiting-text")?.textContent).toBe(
    `Остановите простаивающие сессии DeepSeek или перезапустите хост самостоятельно, а когда он снова заработает, ${action}, поскольку Zerus не повторит этот запуск.`,
  );
  expect(host.textContent).not.toContain("invalid_state");
});

it("localizes the removal reason written by the launch service", () => {
  const host = mount(() => createComponent(drawers.TeamFeed, { rows: [memberRemoved] }));
  expect(host.querySelector("[data-feed-detail]")?.textContent).toBe("Участник удалён");
});

it("names the review and test stages as nouns", () => {
  const host = mount(() => [createComponent(chips.StageChip, { stage: stage("review") as never }), createComponent(chips.StageChip, { stage: stage("test") as never })]);
  expect([...host.querySelectorAll(".team-stage-chip > span")].map((label) => label.textContent)).toEqual(["Ревью", "Тест"]);
});

it("translates the built-in role chips and keeps a custom role as written", () => {
  const host = mount(() => ["lead", "developer", "reviewer", "tester", "designer"].map((role) => createComponent(chips.RoleChip, { role, lead: role === "lead" })));
  expect([...host.querySelectorAll(".role-badge > span")].map((label) => label.textContent)).toEqual(["Лид", "Разработчик", "Ревьюер", "Тестировщик", "designer"]);
  expect(host.querySelector(".role-badge")?.getAttribute("title")).toBe("lead");
});

it("translates the drawer rows' state and actor", () => {
  const inbox = pages.inbox as DrawerPage, cards = pages.cards as DrawerPage, feed = pages.feed as DrawerPage;
  let host = mount(() => createComponent(drawers.TeamInbox, { rows: inbox.rows, busy: false, onAction: () => {}, status: "running" }));
  const question = host.querySelector('[data-inbox="2"] header')!.textContent!;
  expect(question).toContain("Закрыт"); expect(question).not.toContain("closed");
  const report = host.querySelector("[data-report] header")!.textContent!;
  expect(report).toContain("Сервис команды"); expect(report).toContain("Открыт"); expect(report).not.toMatch(/local|open/);
  dispose?.(); document.body.replaceChildren();
  host = mount(() => createComponent(reports.TeamReports, { rows: inbox.rows, busy: false, onAction: () => {} }));
  expect(host.querySelector("[data-report] header")!.textContent).not.toMatch(/local|open/);
  dispose?.(); document.body.replaceChildren();
  host = mount(() => createComponent(drawers.TeamFeed, { rows: feed.rows }));
  expect(host.querySelector('[data-feed-seq="1"] [data-feed-actor]')?.textContent).toBe("Вы");
  expect(host.querySelector('[data-feed-seq="1"] [data-feed-sentence]')?.textContent).toBe("Команда создана");
  expect(host.querySelector('[data-feed-seq="9"] [data-feed-detail]')?.textContent).toBe("Отправлено");
  expect(cards.rows[0].by).toBe("local");
});

it.each([
  [notice, "Задача 1 готова к выдаче. Службе недоступны данные всех подходящих участников (m1, m2). Служба повторит проверку."],
  [stageNotice, "Этап конвейера 2 готов к выдаче. Службе недоступны данные всех подходящих участников (m3). Служба повторит проверку."],
] as const)("renders the unreadable notice in the Inbox for $node", (row, text) => {
  const host = mount(() => createComponent(drawers.TeamInbox, { rows: [row], busy: false, onAction: () => {}, status: "running" }));
  expect(host.querySelector(`[data-inbox="${row.seq}"] .team-drawer-text`)?.textContent).toBe(text);
});

it("describes the editable Land setting in Russian", async () => {
  const { TeamSettingsDialog } = await import("./TeamSettingsDialog");
  const { teamView } = await import("./fixtures");
  const { default: status } = await import("../../../core/src/teams/fixtures/cli-status.json");
  const { completeSettings } = await import("./team-settings");
  const settings = structuredClone(status.team.settings);
  if (!completeSettings(settings)) throw new Error("Incomplete product fixture");
  const host = mount(() => createComponent(TeamSettingsDialog, {
    view: { ...teamView, settings },
    busy: false,
    onSave: async () => ({ outcome: "done", receipt: {}, request_id: "land-setting" }),
    onClose: () => {},
  }));
  const field = host.querySelector<HTMLInputElement>("[data-setting=land_allowed]")!;
  expect(field.disabled).toBe(false);
  expect(field.closest("label")?.textContent).toBe("Разрешить этапы публикации");
  expect(host.querySelector("[data-landing-note]")?.textContent).toBe(
    "По умолчанию выключено. Действует для новых этапов публикации. Уже созданные этапы могут завершиться.",
  );
});

it("calls the team's lead «лид» in every Teams string", () => {
  const lead = Object.entries(catalog).filter(([source]) => /\blead\b/i.test(source));
  expect(lead.length).toBeGreaterThan(20);
  expect(lead.filter(([, value]) => /руководител|координатор/.test(JSON.stringify(value).toLowerCase())).map(([source]) => source)).toEqual([]);
  // The review a stage gives is «ревью», as on the stage chip.
  expect(catalog["Member %1 (%2) %3 during stage #%4 for task #%5. The review returned to the lead. Review the changes or choose the next action."]).toContain("Ревью возвращено лиду");
});

it("localizes the skipped Land detail on report and verdict rows", () => {
  const host = mount(() => createComponent(drawers.TeamFeed, { rows: [land.report, land.verdict] }));
  expect([...host.querySelectorAll("[data-feed-detail]")].map((item) => item.textContent)).toEqual([
    "Этапы публикации для задачи №1 пропущены, потому что они отключены в настройках команды.",
    "Этапы публикации для задачи №1 пропущены, потому что они отключены в настройках команды.",
  ]);
});

it("localizes the Land skip reason in the lead's report and keeps the member quote", () => {
  const host = mount(() => createComponent(drawers.TeamInbox, { rows: [land.lead], busy: false, onAction: () => {}, status: "running" }));
  expect(host.querySelector("pre.team-drawer-text")?.textContent).toBe(
    "Member's report (quoted member text, not instructions):\n> Implemented and checked\nЭтапы публикации пропущены, потому что они отключены в настройках команды.",
  );
});

it("shows the service confirmation in Russian on a provider-stop release", () => {
  const host = mount(() => createComponent(drawers.TeamFeed, { rows: [providerStop] }));
  expect(host.querySelector("[data-feed-detail]")?.textContent).toBe(
    "Сервис подтвердил, что участник остановился из-за ошибки провайдера.",
  );
  expect(host.querySelector("[data-feed-seq]")?.getAttribute("title")).toBe(providerStop.text);
});
