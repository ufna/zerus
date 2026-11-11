// @vitest-environment happy-dom
// The New team footer (the fork's NewTeamFooter, adapted to M1): what blocks
// Create team is written out, Save as template and Cancel sit beside it, and
// Create is disabled until the form is ready.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeamFooter } from "./NewTeamFooter";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup(over: Partial<Parameters<typeof NewTeamFooter>[0]> = {}) {
  const host = document.createElement("div");
  document.body.append(host);
  const onCreate = vi.fn();
  dispose = render(() => createComponent(NewTeamFooter, {
    busy: false, createWhy: "", canSave: true, saving: false, onSaveTemplate: vi.fn(), onCancel: vi.fn(), onCreate, ...over,
  }), host);
  return { host, onCreate };
}

it("says nothing is created until Create team and enables it when ready", () => {
  const { host, onCreate } = setup();
  expect([...host.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Save as template", "Create team", "Cancel"]);
  expect(host.textContent).toContain("Nothing is created until you press Create team.");
  (host.querySelector(".nt-create") as HTMLButtonElement).click();
  expect(onCreate).toHaveBeenCalledTimes(1);
});

it("writes out what blocks Create team", () => {
  const { host, onCreate } = setup({ createWhy: "Check the project folder: it must be a Git folder." });
  expect(host.querySelector("[data-create-blocker]")?.textContent).toContain("must be a Git folder");
  expect((host.querySelector(".nt-create") as HTMLButtonElement).disabled).toBe(true);
  (host.querySelector(".nt-create") as HTMLButtonElement).click();
  expect(onCreate).not.toHaveBeenCalled();
});

it("shows the last refusal", () => {
  const { host } = setup({ error: "refused: no such template" });
  expect(host.querySelector("[data-create-error]")?.textContent).toContain("no such template");
});
