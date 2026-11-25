// @vitest-environment happy-dom
// G12 (zerus-efeo3): the catalog page through the real teams store and the
// real catalog editor store; only the desktop invoke is replaced. The teams
// list row is pinned by the Rust product writer (teams_screen.rs), the editor
// schema and built-in catalog by theirs (catalog-editor-schema.json,
// catalog-builtin.json), and the read error text is catalog_editor_load's.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "../screens/fixtures/teams-screen.json";
import schema from "../../../core/src/teams/fixtures/catalog-editor-schema.json";
import builtin from "../../../../crates/team-plan/fixtures/catalog-builtin.json";
import type { TeamList } from "../../store/teams";
import type { CatalogSnapshot } from "../../store/catalogEditor";

const f = vi.hoisted(() => ({ loads: [] as ("fail" | "ok")[], calls: [] as string[], snapshot: undefined as unknown }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (command: string) => {
  f.calls.push(command);
  if (command === "catalog_editor_load") {
    // catalog_editor_load's own refusal for a failed `hgs team catalog snapshot`.
    if (f.loads.shift() === "fail") throw "The catalog could not be read.";
    return f.snapshot;
  }
  return undefined;
}, isTauri: () => false }));

const snapshot: CatalogSnapshot = { revision: "r1", person: {}, catalog: builtin, builtin, schema_fields: schema as CatalogSnapshot["schema_fields"] };
const disposers: (() => void)[] = [];
const tick = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };
let CatalogPage: typeof import("./CatalogPage").CatalogPage;
async function page() {
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => createComponent(CatalogPage, {}), host)); await tick(); return host;
}
beforeEach(async () => {
  vi.resetModules(); f.loads = []; f.calls = []; f.snapshot = snapshot;
  const { teams } = await import("../../store/teams");
  teams.apply([{ computer: "", key: "list", kind: "list", payload: (fixture as unknown as { pages: TeamList[] }).pages[0] }]);
  ({ CatalogPage } = await import("./CatalogPage"));
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); });

it("a successful read clears the earlier read error", async () => {
  f.loads = ["fail", "ok"];
  const first = await page();
  expect(first.querySelector('[role="status"]')?.textContent).toBe("The catalog could not be read.");
  disposers.pop()!(); first.remove();
  // Opening the page again reads the catalog again, through the same store.
  const again = await page();
  expect(f.calls.filter((c) => c === "catalog_editor_load")).toHaveLength(2);
  expect(again.querySelector(".cat-row")).not.toBeNull();
  expect(again.textContent).not.toContain("The catalog could not be read.");
  expect(again.querySelector('[role="status"]')).toBeNull();
});

it("names the catalog once", async () => {
  f.loads = ["ok"];
  const host = await page();
  expect(host.querySelector(".cat-row")).not.toBeNull();
  const headings = [...host.querySelectorAll("h1, h2, h3")].filter((h) => h.textContent === "Team catalog");
  expect(headings).toHaveLength(1);
  // The page keeps its Back and the editor its actions.
  expect([...host.querySelectorAll("button")].map((b) => b.textContent)).toEqual(expect.arrayContaining(["Back", "Save catalog", "Revert"]));
});
