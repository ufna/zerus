// @vitest-environment happy-dom
// Review of #2558 (P3, inbox closers): an unstaffed item closes when staffing
// reconciliation finds its task held or covered again (ops unstaffed.rs
// reconcile), and a paused member's start notice closes when that member is
// active again (ops members.rs close_paused_started, actor schedule.rs). The
// first test is the reviewer's probe (an open unstaffed:8 item); the kinds are
// the writers' own (unstaffed:<revision>, member_started:<node>:<member>:<n>).
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import pages from "../../../core/src/teams/fixtures/drawers.json";
import { TeamInbox } from "./TeamDrawers";
import { TEAM_ID } from "./fixtures";
import type { DrawerPage, DrawerRow } from "../../store/teamDrawers";

const question = (pages.inbox as DrawerPage).rows.find((row) => row.kind.startsWith("question:"))!;
const row = (seq: number, kind: string, node: number): DrawerRow => ({ ...question, seq, kind, node, generation: 1, state: "open", reply: null, notes: [], card: null, text: "Notice." });
let dispose: (() => void) | undefined;
afterEach(async () => {
  dispose?.(); dispose = undefined; document.body.replaceChildren();
  const prefs = await import("../../store/settings"); await prefs.setSetting("workspace/language", "en", { persist: false });
});
const lines = (rows: DrawerRow[]) => {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TeamInbox, { rows, busy: false, onAction: () => {}, status: "running", team: TEAM_ID }), host);
  return (seq: number) => host.querySelector(`[data-inbox="${seq}"] [data-inbox-closes]`)?.textContent;
};

it("an open unstaffed item names its own closer", () => {
  const line = lines([row(8, "unstaffed:8", 3)]);
  expect(line(8)).toBe("This item closes by itself when task #3 is held or a member can take it again.");
});

it("a paused member's start notice closes when that member is active again", () => {
  const line = lines([row(9, "member_started:3:m2:1", 3)]);
  expect(line(9)).toBe("This item closes by itself when member m2 is active again.");
});

it("both closers read from the item kind in Russian", async () => {
  const prefs = await import("../../store/settings");
  await prefs.settingsReady;
  await prefs.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n");
  language.installUiLanguage();
  const { vi } = await import("vitest");
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
  const line = lines([row(8, "unstaffed:8", 3), row(9, "member_started:3:m2:1", 3)]);
  expect(line(8)).toBe("Этот элемент закроется сам, когда задачу #3 удержат или её снова сможет взять участник.");
  expect(line(9)).toBe("Этот элемент закроется сам, когда участник m2 снова станет активным.");
});
