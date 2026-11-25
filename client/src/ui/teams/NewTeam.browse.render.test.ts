// @vitest-environment happy-dom
// #2533 review (review2533, Review2533.project.test.ts adopted): the folder a
// team is created from must be its project's one folder on this computer.
// Browse re-resolves the project from the chosen folder every time, and
// Create checks that the folder belongs to the chosen project, also after a
// typed edit. The projects are client core's own ProjectsView projection
// (views::projects::tests::the_new_team_projects_fixture_is_this_projection).
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam, samePath } from "./NewTeam";
import { catalog } from "./fixtures";
import projects from "./fixtures/new-team-projects.json";
import type { ProjectsView } from "../../types";

const view = projects as unknown as ProjectsView;
const HERE = "00000000-0000-4000-8000-0000000000aa";
const HEAD = "0123456789abcdef0123456789abcdef01234567";
const OTHER = "fedcba9876543210fedcba9876543210fedcba98";
// A second project whose one folder on this computer is another repository.
const orders = view.projects.find((p) => p.id === "orders")!;
const another = { ...orders, id: "other", name: "Second Project",
  folders: [{ ...orders.folders.find((entry) => entry.machine_id === HERE)!, id: "second", key: "second", path: "/workspace/second" }] };
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const flush = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };
type Created = { project: string; base_commit: string };

function setup(overrides: Partial<Parameters<typeof NewTeam>[0]> = {}) {
  const host = document.createElement("div"); document.body.append(host);
  const created: Created[] = [];
  dispose = render(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: HERE, defaultProject: "", defaultFolder: "", projects: [...view.projects, another],
    onCheckFolder: async (path) => ({ git: true, head: path === "/workspace/example" ? HEAD : OTHER, branch: "main", reason: "" }),
    onBrowseFolder: async () => "/workspace/second",
    onCreate: async (input) => { created.push(input); return { outcome: "done", receipt: {}, request_id: input.request_id }; },
    onSaveTemplate: async () => {}, onClose: () => {}, ...overrides,
  }), host);
  const input = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  input.value = "Fixes"; input.dispatchEvent(new Event("input", { bubbles: true }));
  return { host, created };
}
const button = (host: HTMLElement, text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text)!;
const chooser = (host: HTMLElement) => host.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="Project"]')!;
const folder = (host: HTMLElement) => host.querySelector<HTMLInputElement>('input[aria-label="Project folder"]')!;
const blocker = (host: HTMLElement) => host.querySelector("[data-create-blocker]")?.textContent ?? "";
async function choose(host: HTMLElement, id: string) {
  chooser(host).click(); await flush();
  document.querySelector<HTMLButtonElement>(`[role="option"][data-value="${id}"]`)!.click(); await flush();
}
async function browse(host: HTMLElement) {
  button(host, "Browse…").click(); await flush();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
}

it("Browse to another project's folder selects that project and creates with its own base", async () => {
  const { host, created } = setup();
  await choose(host, "orders");
  expect(folder(host).value).toBe("/workspace/example");
  await browse(host);
  expect(folder(host).value).toBe("/workspace/second");
  expect(chooser(host).textContent).toContain("Second Project");
  button(host, "Create team").click(); await flush();
  expect(created).toEqual([expect.objectContaining({ project: "other", base_commit: OTHER })]);
});

it("Browse to a Git folder of no project clears the chosen project and says why Create is blocked", async () => {
  const { host, created } = setup({ onBrowseFolder: async () => "/workspace/unlisted" });
  await choose(host, "orders");
  await browse(host);
  expect(chooser(host).textContent).toContain("Choose a project");
  expect(blocker(host)).toContain("This folder belongs to no project on this computer. Add it on the Projects screen.");
  expect(button(host, "Create team").disabled).toBe(true);
  button(host, "Create team").click(); await flush();
  expect(created).toEqual([]);
});

it("Browse to a Git folder of no project, with none chosen, blocks Create with the same reason", async () => {
  const { host, created } = setup({ onBrowseFolder: async () => "/workspace/unlisted" });
  await browse(host);
  expect(blocker(host)).toContain("This folder belongs to no project on this computer.");
  button(host, "Create team").click(); await flush();
  expect(created).toEqual([]);
});

it("a typed folder that is not the chosen project's folder blocks Create", async () => {
  const { host, created } = setup();
  await choose(host, "orders");
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  expect(button(host, "Create team").disabled).toBe(false);
  folder(host).value = "/workspace/second"; folder(host).dispatchEvent(new Event("input", { bubbles: true }));
  button(host, "Check folder").click(); await flush();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  expect(chooser(host).textContent, "a typed edit keeps the person's project").toContain("Orders API");
  expect(blocker(host)).toContain("The folder is not the chosen project's folder on this computer.");
  button(host, "Create team").click(); await flush();
  expect(created).toEqual([]);
  // The project's own folder, typed back with a trailing separator, is that folder.
  folder(host).value = "/workspace/example/"; folder(host).dispatchEvent(new Event("input", { bubbles: true }));
  button(host, "Check folder").click(); await flush();
  await vi.waitFor(() => expect(button(host, "Create team").disabled).toBe(false));
});

it.each(["\\", " "])("Browse keeps the literal POSIX suffix %j when checking a folder", async (suffix) => {
  const path = "/workspace/example" + suffix;
  const check = vi.fn(async (value: string) => ({
    git: true, head: value === path ? OTHER : HEAD, branch: "main", reason: "",
  }));
  const { host, created } = setup({ onCheckFolder: check, onBrowseFolder: async () => path });
  await choose(host, "orders");
  await browse(host);
  expect(folder(host).value).toBe(path);
  expect(check).toHaveBeenLastCalledWith(path);
  expect(chooser(host).textContent).toContain("Choose a project");
  expect(blocker(host)).toContain("This folder belongs to no project on this computer.");
  expect(button(host, "Create team").disabled).toBe(true);
  button(host, "Create team").click();
  expect(created).toEqual([]);
});

it.each(["\\", " "])("a typed POSIX folder with literal suffix %j cannot use another project's ID", async (suffix) => {
  const path = "/workspace/example" + suffix;
  const check = vi.fn(async (value: string) => ({
    git: true, head: value === path ? OTHER : HEAD, branch: "main", reason: "",
  }));
  const { host, created } = setup({ onCheckFolder: check });
  await choose(host, "orders");
  folder(host).value = path;
  folder(host).dispatchEvent(new Event("input", { bubbles: true }));
  button(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  expect(check).toHaveBeenLastCalledWith(path);
  expect(blocker(host)).toContain("The folder is not the chosen project's folder on this computer.");
  expect(button(host, "Create team").disabled).toBe(true);
  button(host, "Create team").click();
  expect(created).toEqual([]);
});

it("keeps Windows trailing-separator support without confusing an absolute root and a relative drive", () => {
  expect(samePath("C:\\work\\repo\\", "C:\\work\\repo", "windows")).toBe(true);
  expect(samePath("C:\\", "C:/", "windows")).toBe(true);
  expect(samePath("C:\\", "C:", "windows")).toBe(false);
});

it.each(["\\", " "])("Browse to a registered POSIX sibling with suffix %j uses its own project and base", async (suffix) => {
  const path = "/workspace/example" + suffix;
  const sibling = { ...another, folders: [{ ...another.folders[0], path }] };
  const { host, created } = setup({
    projects: [...view.projects, sibling],
    onBrowseFolder: async () => path,
  });
  await choose(host, "orders");
  await browse(host);
  expect(chooser(host).textContent).toContain("Second Project");
  button(host, "Create team").click();
  await flush();
  expect(created).toEqual([expect.objectContaining({ project: "other", base_commit: OTHER })]);
});

it("a late check for the previous project does not replace the current project's facts", async () => {
  const resolve: Record<string, (facts: { git: boolean; head: string; branch: string; reason: string }) => void> = {};
  const { host, created } = setup({ onCheckFolder: (path) => new Promise((done) => { resolve[path] = done; }) });
  await choose(host, "orders");
  await choose(host, "other");
  resolve["/workspace/second"]({ git: true, head: OTHER, branch: "main", reason: "" });
  await flush();
  expect(button(host, "Create team").disabled).toBe(false);
  resolve["/workspace/example"]({ git: true, head: HEAD, branch: "main", reason: "" });
  await flush();
  button(host, "Create team").click(); await flush();
  expect(created).toEqual([expect.objectContaining({ project: "other", base_commit: OTHER })]);
});
