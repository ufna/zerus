// @vitest-environment happy-dom
// New team shows what it grants (the guide review, number 4): every delegation
// of the template with its value, in plain words, as a line the person can
// change before creating; for each of the template's pipelines who ends it;
// nothing about phases. The create carries exactly what the form shows: the
// shown template's digest and the five permissions as the checkboxes show
// them (the create binding); Save as template is an action of its own.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam, pipelineLines, settingLines, type NewTeamInput } from "./NewTeam";
import { catalog as fixture } from "./fixtures";
import type { TeamCatalog, TeamCatalogTemplate } from "../../store/teams";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup(start: TeamCatalog = fixture) {
  const [catalog, setCatalog] = createSignal(start);
  const created: NewTeamInput[] = [];
  const saved: TeamCatalogTemplate[] = [];
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(NewTeam, {
    get catalog() { return catalog(); }, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: async (input) => { created.push(input); return { outcome: "done", receipt: {}, request_id: input.request_id }; },
    // As the store does: the save, then the command line's catalog read, which gives the saved template its digest.
    onSaveTemplate: async (template) => { saved.push(template); setCatalog({ ...catalog(), templates: [...catalog().templates, { ...template, digest: "saved-digest" }] }); },
    onClose: () => {},
  }), host);
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  return { host, created, saved };
}
const button = (host: HTMLElement, label: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === label)!;
const delegations = (host: HTMLElement) => [...host.querySelectorAll<HTMLLabelElement>("[data-delegation]")]
  .map((line) => [line.dataset.delegation, line.querySelector<HTMLInputElement>("input")!.checked]);

it("lists every delegation of the template, in its order, with its value and what it allows", () => {
  const { host } = setup();
  expect(delegations(host)).toEqual([["accept_results", true], ["answer_members", true], ["confirm_trust", true], ["resume_exited", true], ["merge_accepted", true]]);
  const text = host.querySelector("[data-permissions]")!.textContent!;
  expect(text).toContain("Accept results");
  expect(text).toContain("The lead may accept a team task's result without you.");
  expect(text).toContain("An accepted result is merged into the integration branch without your approval.");
});

it("takes the delegations from the template: a value off and a name this version does not know", () => {
  const template = { ...fixture.templates[0], settings: { ...fixture.templates[0].settings, delegations: { accept_results: false, future_power: true } } };
  const { host } = setup({ ...fixture, templates: [template] });
  expect(delegations(host)).toEqual([["accept_results", false], ["future_power", true]]);
  expect(host.querySelector('[data-delegation="future_power"]')!.textContent).toContain("future_power");
  expect(host.querySelector('[data-delegation="future_power"]')!.textContent).toContain("A permission this version does not describe.");
});

it("creates from the template with its digest and the permissions as shown", async () => {
  const { host, created } = setup();
  button(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  button(host, "Create team").click();
  await vi.waitFor(() => expect(created).toHaveLength(1));
  expect(created[0].template).toBe("bugfix");
  expect(created[0].expected_template).toBe(fixture.templates[0].digest);
  expect(created[0].delegations).toEqual({ accept_results: true, answer_members: true, confirm_trust: true, resume_exited: true, merge_accepted: true });
});

it("changed permissions go with the create, without saving a template first", async () => {
  const { host, created, saved } = setup();
  button(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  const accept = host.querySelector<HTMLInputElement>('[data-delegation="accept_results"] input')!;
  accept.checked = false; accept.dispatchEvent(new Event("change", { bubbles: true }));
  expect(button(host, "Create team").disabled).toBe(false);
  expect(host.querySelector("[data-create-blocker]")).toBeNull();
  button(host, "Create team").click();
  await vi.waitFor(() => expect(created).toHaveLength(1));
  expect(saved).toHaveLength(0);
  expect(created[0].template).toBe("bugfix");
  expect(created[0].expected_template).toBe(fixture.templates[0].digest);
  expect(created[0].delegations).toEqual({ accept_results: false, answer_members: true, confirm_trust: true, resume_exited: true, merge_accepted: true });
});

it("Save as template stays an action of its own: the saved row has no digest, and the form takes the saved one", async () => {
  const { host, created, saved } = setup();
  button(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  const merge = host.querySelector<HTMLInputElement>('[data-delegation="merge_accepted"] input')!;
  merge.checked = false; merge.dispatchEvent(new Event("change", { bubbles: true }));
  button(host, "Save as template").click();
  await vi.waitFor(() => expect(saved).toHaveLength(1));
  expect("digest" in saved[0], "a shown row's digest is never sent to a template save").toBe(false);
  expect(Object.keys(saved[0]).sort(), "only the template's own fields, never the row's facts").toEqual(["default_pipeline", "description", "id", "lead", "name", "roster", "settings"]);
  expect(saved[0].settings.delegations).toEqual({ accept_results: true, answer_members: true, confirm_trust: true, resume_exited: true, merge_accepted: false });
  await vi.waitFor(() => expect(host.querySelector<HTMLInputElement>(`[data-template="${saved[0].id}"] input`)?.checked).toBe(true));
  expect(delegations(host).at(-1)).toEqual(["merge_accepted", false]);
  button(host, "Create team").click();
  await vi.waitFor(() => expect(created).toHaveLength(1));
  expect(created[0].template).toBe(saved[0].id);
  expect(created[0].expected_template).toBe("saved-digest");
});

it("names who ends each of the template's pipelines and says nothing about phases", () => {
  const docs = { id: "docs", name: "Docs", implementer: "developer", stages: [], max_rounds: 0, accept: "pipeline" };
  const roles = fixture.roles.map((role) => role.id === "reviewer" ? { ...role, pipeline: "docs" } : role);
  const lines = pipelineLines({ ...fixture, roles, pipelines: [...fixture.pipelines, docs] }, fixture.templates[0]);
  expect(lines.find((line) => line.startsWith("Reviewed"))).toMatch(/— the lead accepts$/u);
  expect(lines.find((line) => line.startsWith("Docs"))).toMatch(/— it accepts by itself$/u);
  expect(pipelineLines({ ...fixture, pipelines: [{ ...fixture.pipelines[0], accept: "person" }] }, fixture.templates[0])[0]).toMatch(/— you accept$/u);
  const labels = settingLines(fixture.templates[0]).map(([label]) => label);
  expect(labels).not.toContain("Plan approval");
  expect(labels).not.toContain("Acceptance");
  expect(settingLines(fixture.templates[0]).flat().join(" ")).not.toMatch(/phase/iu);
});

it("a catalog read's or a template save's refusal is shown without its code", async () => {
  const failing: TeamCatalogTemplate[] = [];
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(NewTeam, {
    catalog: fixture, catalogError: "store_busy: The team store is busy. Nothing was written; try again.", busy: false,
    machineId: "00000000-0000-4000-8000-0000000000aa", defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: async (input) => ({ outcome: "done", receipt: {}, request_id: input.request_id }),
    onSaveTemplate: async (template) => { failing.push(template); throw "invalid_state: The built-in template is read-only."; },
    onClose: () => {},
  }), host);
  expect(host.querySelector(".nt-error")?.textContent).toBe("The team store is busy. Nothing was written; try again.");
  button(host, "Save as template").click();
  await vi.waitFor(() => expect(failing).toHaveLength(1));
  await vi.waitFor(() => expect(host.querySelector("[data-new-team-error]")?.textContent).toBe("The built-in template is read-only."));
  expect(host.querySelector("[data-create-error]")?.textContent).not.toContain("invalid_state");
});

it("a template this version cannot show whole cannot be saved either, so nothing it holds is dropped", () => {
  const template = { ...fixture.templates[0], unknown_permissions: ["future_power"], unknown_fields: [] };
  const { host } = setup({ ...fixture, templates: [template] });
  expect(button(host, "Save as template").disabled).toBe(true);
  const other = { ...fixture.templates[0], unknown_permissions: [], unknown_fields: ["settings/auto_land"] };
  dispose?.(); dispose = undefined; document.body.replaceChildren();
  expect(button(setup({ ...fixture, templates: [other] }).host, "Save as template").disabled).toBe(true);
});
