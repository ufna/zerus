// @vitest-environment happy-dom
// zerus-63rru.197 (C1, A3, C2): every confirmation the core asks for reads,
// in English, exactly as the core wrote it, and in Russian from the catalog.
// `confirmations.json` is the core's own output for each destructive action
// (client/core teams/action/tests.rs `the_confirmations_are_the_shared_fixture`).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import confirmations from "../../../core/src/teams/fixtures/confirmations.json";

type Asked = { call: { team_name: string; action: Record<string, unknown> }; confirmation: { name: string; title: string; text: string; action: string } };
const f = vi.hoisted(() => ({ setAsking: (_: unknown) => {} }));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [asking, setAsking] = createSignal<unknown>(null);
  f.setAsking = setAsking as (value: unknown) => void;
  return { teams: { confirmation: asking, confirm: vi.fn(), cancelConfirmation: vi.fn(), run: vi.fn(), check: vi.fn(), dismiss: vi.fn(), uncertain: () => null } };
});
vi.mock("../lib/icons", () => ({ Icon: () => null }));
const { TeamConfirmDialog } = await import("./TeamConfirmDialog");

const rows = confirmations as unknown as Asked[];
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; f.setAsking(null); document.body.replaceChildren(); });

function shown(asked: Asked) {
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  f.setAsking(asked);
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TeamConfirmDialog, {}), host);
  const dialog = host.querySelector("dialog.team-confirm")!;
  const result = {
    name: dialog.getAttribute("data-name"),
    title: dialog.querySelector("#team-confirm-title")!.textContent!,
    text: dialog.querySelector("#team-confirm-text")!.textContent!,
    action: dialog.querySelector(".btn.solid-danger")!.textContent!,
  };
  dispose(); dispose = undefined; document.body.replaceChildren(); f.setAsking(null);
  return result;
}

it("shows each of the core's confirmations in its own English words", () => {
  expect(rows.map((row) => row.confirmation.name)).toEqual(expect.arrayContaining(["teamStopConfirm", "teamArchiveConfirm", "teamArchiveMemberConfirm"]));
  for (const row of rows) {
    const { name, title, text, action } = row.confirmation;
    expect(shown(row)).toEqual({ name, title, text, action });
  }
});

it("shows each of the core's confirmations in Russian from the catalog", async () => {
  const prefs = await import("../../store/settings");
  await prefs.settingsReady;
  await prefs.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n");
  language.installUiLanguage();
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
  try {
    for (const row of rows) {
      const russian = shown(row);
      expect(russian.name).toBe(row.confirmation.name);
      for (const key of ["title", "text", "action"] as const) {
        expect(russian[key], `${row.confirmation.name} ${key}`).not.toBe(row.confirmation[key]);
        expect(russian[key], `${row.confirmation.name} ${key}`).toMatch(/[а-яё]/i);
      }
      if (row.call.team_name) expect(russian.text).toContain(row.call.team_name);
    }
    const stop = shown(rows.find((row) => row.confirmation.name === "teamStopConfirm" && row.call.team_name)!);
    expect(stop.text).toContain("Остановленную команду нельзя запустить снова");
  } finally {
    await prefs.setSetting("workspace/language", "en", { persist: false });
  }
});
