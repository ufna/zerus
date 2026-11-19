// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamLeadContact } from "./TeamLeadContact";
import { TeamDrawerButtons, TeamFeed, TeamInbox } from "./TeamDrawers";
import { ProposalCards } from "./ProposalCards";
import { WaitingCard } from "./Waiting";
import fixture from "../../../core/src/teams/fixtures/person-lead-contact.json";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); });
function mount(component: () => import("solid-js").JSX.Element) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(component, host);
  return host;
}

it.each([["question", "ask_lead"], ["note", "leave_lead_note"]])("submits the reviewed %s mode and revision", (mode, kind) => {
  const onAction = vi.fn().mockResolvedValue({ outcome: "failed" });
  const host = mount(() => createComponent(TeamLeadContact, {
    team: fixture.reply.team, revision: 7, status: "running", busy: false, onAction,
  }));
  host.querySelector<HTMLButtonElement>("[data-team-contact]")!.click();
  const select = host.querySelector("select")!;
  select.value = mode;
  select.dispatchEvent(new Event("change"));
  const text = host.querySelector("textarea")!;
  text.value = "Line one\nLine two";
  text.dispatchEvent(new Event("input", { bubbles: true }));
  host.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
  expect(onAction).toHaveBeenCalledWith(kind, { text: "Line one\nLine two" }, 7, fixture.reply.team);
});

it("opens the lead reply with no decision buttons", () => {
  const onOpenNotice = vi.fn(), onDecide = vi.fn();
  const host = mount(() => createComponent(WaitingCard, {
    item: fixture.reply, teamName: "Example", now: 1234, busy: false,
    onDecide, onOpenTeam: vi.fn(), onOpenNotice,
  }));
  expect(host.querySelector("[data-decision]")).toBeNull();
  expect(host.textContent).not.toContain(fixture.reply.text);
  host.querySelector<HTMLButtonElement>("[data-open-lead-reply]")!.click();
  expect(onOpenNotice).toHaveBeenCalledWith(fixture.reply);
  expect(onDecide).not.toHaveBeenCalled();
});

it("shows the contact badge and the persisted reply under the question", () => {
  const onOpen = vi.fn();
  const host = mount(() => [
    createComponent(TeamDrawerButtons, { value: null, leadNotices: 1, onOpen }),
    createComponent(TeamInbox, { rows: [fixture.inbox], busy: false, onAction: vi.fn() }),
  ]);
  expect(host.querySelector("[data-team-lead-notices]")?.textContent?.trim()).toBe("1");
  expect(host.textContent).toContain(fixture.reply.text);
  expect(host.textContent).toContain(fixture.inbox.text);
  expect(host.querySelector("[data-inbox-answer]")).toBeNull();
});


it("shows the lead's acknowledgement as a passive card and a Feed sentence", () => {
  const onAction = vi.fn();
  const host = mount(() => [
    createComponent(ProposalCards, { rows: [fixture.ack], team: fixture.reply.team, busy: false, onAction }),
    createComponent(TeamFeed, { rows: [fixture.notice] }),
  ]);
  expect(host.querySelector("[data-feed-sentence]")?.textContent).toBe("The lead acknowledged your note");
  expect(host.querySelector("[data-decision]")).toBeNull();
  host.querySelector<HTMLButtonElement>("[data-open-lead-notice]")!.click();
  expect(onAction).toHaveBeenCalledWith("read_lead_notice", { card: fixture.ack.card.card });
});

it("renders the form and reply in Russian through the shared catalog", async () => {
  const preferences = await import("../../store/settings");
  const language = await import("../../i18n");
  await preferences.settingsReady;
  await preferences.setSetting("workspace/language", "ru", { persist: false });
  language.installUiLanguage();
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
  try {
    const host = mount(() => [
      createComponent(TeamLeadContact, { team: fixture.reply.team, revision: 7, status: "running", busy: false, onAction: vi.fn() }),
      createComponent(WaitingCard, { item: fixture.reply, teamName: "Пример", now: 1234, busy: false, onDecide: vi.fn(), onOpenTeam: vi.fn(), onOpenNotice: vi.fn() }),
      createComponent(TeamFeed, { rows: [fixture.notice] }),
    ]);
    const button = host.querySelector<HTMLButtonElement>("[data-team-contact]")!;
    expect(button.textContent).toBe("Спросить лида…");
    button.click();
    expect(host.querySelector('option[value="note"]')?.textContent).toBe("Оставить заметку");
    expect(host.querySelector("[data-open-lead-reply]")?.textContent).toBe("Открыть ответ");
    expect(host.querySelector("[data-feed-sentence]")?.textContent).toBe("Лид отметил вашу заметку прочитанной");
  } finally {
    await preferences.setSetting("workspace/language", "en", { persist: false });
  }
});
