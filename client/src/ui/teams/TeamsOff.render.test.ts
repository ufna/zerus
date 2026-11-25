// @vitest-environment happy-dom
// zerus-63rru.197 (C9): a Teams page that is off says where to turn it on.
// The store reaches the off state the way the product does: the list read
// fails with `teams_inactive` (preview off) or `unavailable`.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => undefined, isTauri: () => false }));
vi.mock("./TeamCatalogEditor", () => ({ TeamCatalogEditor: () => null }));

const { teams } = await import("../../store/teams");
const { page, setPage } = await import("../../store/ui");
const { CatalogPage } = await import("./CatalogPage");
const { TeamsScreen } = await import("../screens/Teams");
const { WaitingScreen } = await import("../screens/Waiting");

let dispose: (() => void) | undefined;
// An off answer stops the store until the application's recheck (store/teams.ts stopFor).
beforeEach(async () => { await teams.recheck(); });
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); setPage("sessions"); });
const mount = (component: () => import("solid-js").JSX.Element) => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(() => createComponent(component, {}), host); return host;
};
const off = (code: string) => teams.apply([{ computer: "", key: "list", kind: "failed", payload: { code, message: "off" } }]);

for (const [name, component] of [["Teams", TeamsScreen], ["Waiting for you", WaitingScreen], ["the catalog", CatalogPage]] as const) {
  it(`${name} with the preview off links to Settings > Agent Teams`, async () => {
    off("teams_inactive");
    expect(teams.available()).toBe(false);
    const host = mount(component);
    const link = host.querySelector<HTMLButtonElement>("[data-teams-turn-on]");
    expect(link?.textContent).toBe("Turn on in Settings");
    link!.click();
    // The Settings sections load on demand; their first import can exceed a second.
    await vi.waitFor(() => expect(page()).toBe("settings"), { timeout: 10_000 });
    const { section } = await import("../screens/settings/sections");
    expect(section()).toBe("teams");
  });
}

it("Teams that are unavailable on this computer offer no link", () => {
  off("unavailable");
  const host = mount(TeamsScreen);
  expect(host.textContent).toContain("Teams are unavailable on this computer.");
  expect(host.querySelector("[data-teams-turn-on]")).toBeNull();
});
