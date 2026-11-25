// @vitest-environment happy-dom
// zerus-63rru.197 (C2): an archived team leaves the visible list until the
// person asks for archived teams. The page is the product's: one live team
// and one the person stopped and archived (teams_screen.rs, "archived").
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "../screens/fixtures/teams-screen.json";
import type { TeamList } from "../../store/teams";

vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => undefined, isTauri: () => false }));

const { teams } = await import("../../store/teams");
const { TeamsScreen } = await import("../screens/Teams");
const page = (fixture as unknown as { archived: TeamList }).archived;

let dispose: (() => void) | undefined;
beforeEach(async () => { await teams.recheck(); teams.apply([{ computer: "", key: "list", kind: "list", payload: page }]); });
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const mount = () => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(() => createComponent(TeamsScreen, {}), host); return host;
};
const names = (host: HTMLElement) => [...host.querySelectorAll(".teams-row-name")].map((name) => name.textContent);

it("the product's archived team is hidden until Show archived, and Hide archived hides it again", () => {
  expect(page.teams.map((team) => [team.name, team.status])).toEqual([["Live team", "running"], ["Archived team", "archived"]]);
  const host = mount();
  expect(names(host)).toEqual(["Live team"]);
  const toggle = host.querySelector<HTMLButtonElement>("[data-teams-archived]")!;
  expect(toggle.textContent).toBe("Show archived (1)");
  toggle.click();
  expect(names(host)).toEqual(["Live team", "Archived team"]);
  expect(toggle.textContent).toBe("Hide archived");
  toggle.click();
  expect(names(host)).toEqual(["Live team"]);
});

it("handwritten a page of archived teams only says so instead of No teams yet", () => {
  teams.apply([{ computer: "", key: "list", kind: "list", payload: { ...page, teams: page.teams.filter((team) => team.status === "archived") } }]);
  const host = mount();
  expect(names(host)).toEqual([]);
  expect(host.textContent).not.toContain("No teams yet.");
  expect(host.textContent).toContain("Only archived teams are on this page.");
  host.querySelector<HTMLButtonElement>("[data-teams-archived]")!.click();
  expect(names(host)).toEqual(["Archived team"]);
});
