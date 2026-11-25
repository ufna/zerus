// @vitest-environment happy-dom
// Review of #2558 (P2, native confirmations): the live dialog is the native
// one, so the window's fallback dialog must say what it says, and each
// confirmation sentence is defined once. `native-confirmations.json` is the
// native formatter's own Stop, Archive and Remove sentences, rendered in
// English and Russian by the app's native renderer beside the window's
// confirmation for the same call (client/src-tauri native_text.rs
// `native_confirmations_are_the_window_fixture`).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import natives from "./fixtures/native-confirmations.json";
import confirmations from "../../../core/src/teams/fixtures/confirmations.json";

type Message = { source: string; args: unknown[] };
type Row = { from: string; call: { team_name: string; action: Record<string, unknown> }; confirmation: { name: string; title: string; text: string; action: string; message: Message }; rendered: { en: string; ru: string } };
const f = vi.hoisted(() => ({ setAsking: (_: unknown) => {} }));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [asking, setAsking] = createSignal<unknown>(null);
  f.setAsking = setAsking as (value: unknown) => void;
  return { teams: { confirmation: asking, confirm: vi.fn(), cancelConfirmation: vi.fn(), run: vi.fn(), check: vi.fn(), dismiss: vi.fn(), uncertain: () => null } };
});
vi.mock("../lib/icons", () => ({ Icon: () => null }));
const { TeamConfirmDialog } = await import("./TeamConfirmDialog");

const rows = natives as unknown as Row[];
let dispose: (() => void) | undefined;
afterEach(async () => {
  dispose?.(); dispose = undefined; f.setAsking(null); document.body.replaceChildren();
  const prefs = await import("../../store/settings"); await prefs.setSetting("workspace/language", "en", { persist: false });
});

function sentence(row: Row) {
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  f.setAsking({ call: { ...row.call, confirmed: false }, confirmation: row.confirmation });
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TeamConfirmDialog, {}), host);
  const text = host.querySelector("dialog.team-confirm #team-confirm-text")!.textContent!;
  dispose(); dispose = undefined; document.body.replaceChildren(); f.setAsking(null);
  return text;
}

it("shows the native formatter's Stop, Archive and Remove sentences in English", () => {
  expect(rows.filter((row) => row.from === "native prompt").map((row) => row.confirmation.name)).toEqual(["teamStopConfirm", "teamArchiveConfirm", "teamArchiveMemberConfirm"]);
  for (const row of rows) expect(sentence(row), row.confirmation.name).toBe(row.rendered.en);
});

it("shows the native formatter's Stop, Archive and Remove sentences in Russian", async () => {
  const prefs = await import("../../store/settings");
  await prefs.settingsReady;
  await prefs.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n");
  language.installUiLanguage();
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
  for (const row of rows) {
    expect(row.rendered.ru, row.confirmation.name).toMatch(/[а-яё]/i);
    expect(sentence(row), row.confirmation.name).toBe(row.rendered.ru);
  }
});

// Every window module except tests: a confirmation sentence written there
// would be a second definition of a core sentence.
const WINDOW = import.meta.glob(["../../**/*.ts", "../../**/*.tsx", "!../../**/*.test.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

it("defines no confirmation sentence of its own in the window", () => {
  const sources = [...new Set([...rows, ...(confirmations as unknown as Row[])].map((row) => row.confirmation.message?.source).filter((source): source is string => !!source))];
  expect(sources).toHaveLength(7);
  const copies = Object.entries(WINDOW).flatMap(([file, text]) => sources.filter((source) => text.includes(source)).map((source) => `${file}: ${source.slice(0, 40)}`));
  expect(copies).toEqual([]);
});
