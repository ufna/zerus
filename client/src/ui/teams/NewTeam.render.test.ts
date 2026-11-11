// @vitest-environment happy-dom
// New team (the fork's form adapted to M1): the template, its roster,
// pipelines and settings, a Git folder checked before Create can run, and Save
// as template.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam } from "./NewTeam";
import { catalog } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup(overrides: Partial<Parameters<typeof NewTeam>[0]> = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const created: unknown[] = [];
  dispose = render(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: (input) => created.push(input), onSaveTemplate: async () => {}, onClose: () => {}, ...overrides,
  }), host);
  return { host, created };
}

const find = (host: HTMLElement, label: string) =>
  [...host.querySelectorAll("button")].find((button) => button.textContent === label) as HTMLButtonElement;

it("shows the template's roster, pipelines and the settings a person confirms", () => {
  const { host } = setup();
  expect(host.querySelector("[data-template='bugfix']")).not.toBeNull();
  expect(host.textContent).toContain("Developer × 2");
  expect(host.textContent).toContain("Reviewer × 1");
  expect(host.textContent).toContain("Reviewed: review, lead");
  expect(host.textContent).toContain("an integration branch");
  expect(host.textContent).toContain("the person approves the plan");
  expect(host.textContent).toContain("not allowed");
});

it("cannot create before the folder is a verified Git folder", async () => {
  const { host, created } = setup();
  expect(find(host, "Create team").disabled).toBe(true);
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  expect(find(host, "Create team").disabled).toBe(true);
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  expect(host.textContent).toContain("A Git folder at main");
  expect(find(host, "Create team").disabled).toBe(false);
  find(host, "Create team").click();
  expect(created).toHaveLength(1);
  expect(created[0]).toMatchObject({
    team: expect.any(String), template: "bugfix", project: "example-project", name: "Fixes",
    home: "00000000-0000-4000-8000-0000000000aa",
    base_commit: "0123456789abcdef0123456789abcdef01234567", integration: "branch",
  });
});

it("refuses a folder that is not a Git repository", async () => {
  const { host, created } = setup({
    onCheckFolder: async () => ({ git: false, head: "", branch: "", reason: "not a Git repository" }),
  });
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-bad]")).not.toBeNull());
  expect(host.textContent).toContain("Not a Git folder");
  expect(find(host, "Create team").disabled).toBe(true);
  find(host, "Create team").click();
  expect(created).toHaveLength(0);
});

it("saves the changed template", async () => {
  const saved: unknown[] = [];
  const { host } = setup({ onSaveTemplate: async (template) => { saved.push(template); } });
  find(host, "Save as template").click();
  await vi.waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0]).toMatchObject({ id: "bugfix-bugfix", name: "Bugfix" });
});
