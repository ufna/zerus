// @vitest-environment happy-dom
// G8 (zerus-efeo3): New team's project is a chooser of the Projects catalog
// that fills the folder from that project's one folder on this computer; the
// folder has Browse…; the roster says where its seats are edited. The
// projects are client core's own ProjectsView projection (written by
// views::projects::tests::the_new_team_projects_fixture_is_this_projection).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam } from "./NewTeam";
import { catalog } from "./fixtures";
import projects from "./fixtures/new-team-projects.json";
import type { ProjectsView } from "../../types";

const view = projects as unknown as ProjectsView;
const HERE = "00000000-0000-4000-8000-0000000000aa";
const HEAD = "0123456789abcdef0123456789abcdef01234567";
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const flush = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

function setup(overrides: Partial<Parameters<typeof NewTeam>[0]> = {}) {
  const host = document.createElement("div"); document.body.append(host);
  const checks: string[] = [], created: { project: string }[] = [], browsed: string[] = [], opened = vi.fn();
  dispose = render(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: HERE, defaultProject: "", defaultFolder: "", projects: view.projects,
    onCheckFolder: async (path) => { checks.push(path); return { git: true, head: HEAD, branch: "main", reason: "" }; },
    onBrowseFolder: async (start) => { browsed.push(start); return "/workspace/example"; },
    onCreate: async (input) => { created.push(input); return { outcome: "done", receipt: {}, request_id: input.request_id }; },
    onSaveTemplate: async () => {}, onClose: () => {}, onOpenCatalog: opened, ...overrides,
  }), host);
  return { host, checks, created, browsed, opened };
}
const button = (host: HTMLElement, text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text)!;
const chooser = (host: HTMLElement) => host.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="Project"]')!;
const folder = (host: HTMLElement) => host.querySelector<HTMLInputElement>('input[aria-label="Project folder"]')!;
async function choose(host: HTMLElement, id: string) {
  chooser(host).click(); await flush();
  document.querySelector<HTMLButtonElement>(`[role="option"][data-value="${id}"]`)!.click(); await flush();
}
function name(host: HTMLElement, text: string) {
  const input = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  input.value = text; input.dispatchEvent(new Event("input", { bubbles: true }));
}

it("chooses the project from the catalog and fills its one folder on this computer", async () => {
  const { host, checks, created } = setup();
  expect(chooser(host)).not.toBeNull();
  expect(host.querySelector('input[aria-label="Project"]'), "no free-text project").toBeNull();
  chooser(host).click(); await flush();
  const offered = [...document.querySelectorAll('[role="option"]')].map((o) => o.getAttribute("data-value"));
  // Catalog order; a project this computer cannot open is not offered.
  expect(offered).toEqual(["", "orders", "two", "elsewhere", "ungrouped"]);
  document.querySelector<HTMLButtonElement>('[role="option"][data-value="orders"]')!.click(); await flush();
  expect(chooser(host).textContent).toContain("Orders API");
  // Of /workspace/example here and /srv/orders on studio, this computer's.
  expect(folder(host).value).toBe("/workspace/example");
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  expect(checks).toEqual(["/workspace/example"]);
  name(host, "Fixes");
  expect(button(host, "Create team").disabled).toBe(false);
  button(host, "Create team").click();
  expect(created).toHaveLength(1);
  expect(created[0].project).toBe("orders");
});

it("says why a project without exactly one folder here cannot make a team", async () => {
  const { host, checks } = setup();
  name(host, "Fixes");
  expect(host.querySelector("[data-create-blocker]")?.textContent).toContain("Choose the project.");
  await choose(host, "two");
  expect(host.querySelector("[data-project-folders]")?.textContent).toBe("This project has several folders on this computer; a team needs exactly one.");
  expect(host.querySelector("[data-create-blocker]")?.textContent).toContain("This project has several folders on this computer");
  await choose(host, "elsewhere");
  expect(host.querySelector("[data-project-folders]")?.textContent).toBe("This project has no folder on this computer. Add one on the Projects screen.");
  expect(button(host, "Create team").disabled).toBe(true);
  expect(folder(host).value, "nothing is filled from another computer").toBe("");
  expect(checks).toEqual([]);
});

it("starts from the project of the selected session's folder", async () => {
  const { host } = setup({ defaultFolder: "/workspace/example" });
  await flush();
  expect(chooser(host).textContent).toContain("Orders API");
  expect(folder(host).value).toBe("/workspace/example");
});

it("browses for the folder with the folder picker and checks the chosen one", async () => {
  const { host, browsed, checks } = setup({ defaultFolder: "/workspace/start" });
  button(host, "Browse…").click(); await flush();
  expect(browsed).toEqual(["/workspace/start"]);
  expect(folder(host).value).toBe("/workspace/example");
  await vi.waitFor(() => expect(checks).toEqual(["/workspace/example"]));
  // The chosen folder is a catalog project's only folder here: that project.
  expect(chooser(host).textContent).toContain("Orders API");
});

it("offers Edit roster in Catalog beside the roster", () => {
  const { host, opened, created } = setup();
  const roster = host.querySelector('section[aria-label="Roster"]')!;
  const edit = [...roster.querySelectorAll("button")].find((b) => b.textContent === "Edit roster in Catalog")!;
  expect(edit).toBeDefined();
  edit.click();
  expect(opened).toHaveBeenCalledOnce();
  expect(created).toHaveLength(0);
});
