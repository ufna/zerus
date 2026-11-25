// @vitest-environment happy-dom
// zerus-uiy9p G13: Team settings in the person's words. Timers say "0 or more"
// instead of "From 0 to 4294967295"; Tests and Builds leads with the detected
// providers and keeps the JSON override in a closed Advanced section, open
// only while an override is in effect. The settings and the provider
// inspection are the core's product fixtures.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamSettingsDialog } from "./TeamSettingsDialog";
import { ProviderSettings } from "./ProviderSettings";
import { teamView } from "./fixtures";
import { TIMERS, completeSettings } from "./team-settings";
import status from "../../../core/src/teams/fixtures/cli-status.json";
import scope from "../../../core/src/teams/fixtures/provider-scope.json";
import payload from "../../../core/src/teams/fixtures/providers.json";
import type { ProviderInspection, ProviderRead } from "../../store/teamProviders";
import type { TeamView } from "../../store/teams";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const tick = async () => { await new Promise((resolve) => setTimeout(resolve, 0)); };
function mount(make: () => unknown) { const host = document.createElement("div"); document.body.append(host); dispose = render(make as never, host); return host; }

it("each timer says its range in words, not the 32-bit limit", () => {
  const settings = structuredClone(status.team.settings);
  if (!completeSettings(settings)) throw new Error("Incomplete product fixture");
  const host = mount(() => createComponent(TeamSettingsDialog, { view: { ...structuredClone(teamView), settings } as TeamView, busy: false, onSave: vi.fn(), onClose: vi.fn() }));
  expect(host.textContent).not.toContain("4294967295");
  for (const key of TIMERS) {
    const hint = host.querySelector(`input[data-setting="${key}"]`)!.closest("label")!.querySelector(".hint")!.textContent;
    // The label names the unit ("Report timeout (seconds)"); the hint names the range.
    expect(hint, key).toBe(key === "resume_limit_per_hour" ? "Fixed at 3 per hour" : "0 or more");
  }
});

function provider(inspection: ProviderInspection) {
  return mount(() => createComponent(ProviderSettings, { view: structuredClone(scope.team) as unknown as TeamView, home: inspection.home, locked: false, onSave: vi.fn(), port: vi.fn(async () => inspection) }));
}

it("the detected providers lead and the JSON override waits in a closed Advanced section", async () => {
  const inspection = structuredClone(payload.inspection) as ProviderInspection;
  const host = provider({ ...inspection, override_config: null });
  await tick(); await tick();
  const advanced = host.querySelector("[data-provider-override]")!.closest("details");
  expect(advanced, "the override is in a section of its own").not.toBeNull();
  expect(advanced!.open).toBe(false);
  expect(advanced!.querySelector("summary")?.textContent).toBe("Advanced: override as JSON");
  expect(host.querySelector("[data-provider-source]")?.closest("details")).toBeNull();
});

it("an override in effect keeps the Advanced section open", async () => {
  const inspection = structuredClone(payload.inspection) as ProviderInspection;
  const override = { schema: 1, provider: inspection.providers.slice(0, 1) };
  const host = provider({ ...inspection, override_config: override });
  await tick(); await tick();
  expect(host.querySelector("[data-provider-override]")!.closest("details")?.open).toBe(true);
});

// Review of #2535, adopted: a saved override stays visibly identified while an
// unsaved clear is inspected and Advanced is collapsed.
it("keeps a saved override visibly identified while an unsaved clear is being inspected", async () => {
  const base = structuredClone(payload.inspection) as ProviderInspection;
  const config = { schema: 1, provider: base.providers.slice(0, 1) };
  const saved = { digest: base.digest, source_digest: base.source_digest, override_config: config };
  const checkedDigest = "c".repeat(64);
  const port = vi.fn(async (read: ProviderRead): Promise<ProviderInspection> => {
    if (read.candidate) {
      expect(read.candidate.override_config).toBeNull();
      // The native inspection returns the candidate and persisted selection separately.
      return { ...base, saved, saved_current: false, override_config: null, digest: checkedDigest };
    }
    return { ...base, saved, saved_current: true, override_config: config };
  });
  const commit = vi.fn();
  const host = mount(() => createComponent(ProviderSettings, {
    view: scope.team as unknown as TeamView, home: base.home, locked: false, onSave: commit, port,
  }));
  const details = host.querySelector<HTMLDetailsElement>("details")!;
  await vi.waitFor(() => expect(host.querySelector("[data-provider-source]")?.textContent).toContain("with your override"));
  expect(details.open).toBe(true);

  const input = host.querySelector<HTMLTextAreaElement>("[data-provider-override]")!;
  input.value = "";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  host.querySelector<HTMLButtonElement>("[data-provider-check]")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-provider-digest]")?.textContent).toContain(checkedDigest));
  details.open = false;
  expect(commit).not.toHaveBeenCalled();
  const outsideEditor = [...host.querySelectorAll("[data-provider-source],summary")].map((node) => node.textContent).join(" ");
  expect(outsideEditor).toMatch(/with your override|active override|saved override/i);
});
