// @vitest-environment happy-dom
// Delta review of #2535 (codex/zerus-review2525/review2525-cx), adopted as guards.
import { expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import generatedBoard from "../../../core/src/teams/fixtures/board-pending.json";
import generatedProviders from "../../../core/src/teams/fixtures/providers.json";
import providerScope from "../../../core/src/teams/fixtures/provider-scope.json";
import type { ProviderInspection } from "../../store/teamProviders";
import type { TeamView } from "./types";

it("renders the first-round fix strings in Russian from the installed UI catalog", async () => {
  const prefs = await import("../../store/settings");
  await prefs.settingsReady;
  await prefs.setSetting("workspace/language", "ru", { persist: false });
  const language = await import("../../i18n");
  language.installUiLanguage();
  await vi.waitFor(() => expect(language.uiLanguage()).toBe("ru"));
  const { swapBlocked } = await import("./member-edit");
  const { ProviderSettings } = await import("./ProviderSettings");
  const member = (generatedBoard as unknown as TeamView).members.find((row) => row.id === "m1")!;
  expect(swapBlocked({ ...member, state: "paused", person_stopped: true, open_work: 1 }))
    .toBe("Сначала передайте незавершённую работу участника другому участнику; незавершённая работа: 1");

  const product = structuredClone(generatedProviders.inspection) as ProviderInspection;
  product.saved = {
    digest: product.digest, source_digest: product.source_digest,
    override_config: { schema: 1, provider: product.providers.slice(0, 1) },
  };
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => createComponent(ProviderSettings, {
    view: providerScope.team as unknown as TeamView, home: product.home, locked: false,
    onSave: vi.fn(), port: async () => product,
  }), host);
  try {
    await vi.waitFor(() => expect(host.querySelector("summary [data-provider-saved-override]")?.textContent)
      .toBe("Действует сохранённое переопределение."));
  } finally { dispose(); host.remove(); }
});
