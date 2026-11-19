// @vitest-environment happy-dom
// zerus-uiy9p G10: the red banner after a refused team action says which
// action was refused and can be dismissed; New team says a refusal once.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import kinds from "../../../core/src/teams/fixtures/action-kinds.json";
import { ActionFailureNotice, actionName } from "./TeamActionFailure";
import { NewTeam } from "./NewTeam";
import { catalog } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
function mount(make: () => unknown) {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(make as never, host);
  return host;
}

it("names the refused action, shows its reason and dismisses it", () => {
  const onDismiss = vi.fn();
  const host = mount(() => createComponent(ActionFailureNotice, { failure: { key: "action", code: "invalid_state", message: "This member cannot be paused now.", source: "team-tab:a", action: "pause_member" }, onDismiss }));
  expect(host.querySelector("[data-team-action-failure-action]")?.textContent).toBe("Pause member");
  expect(host.querySelector("[data-team-action-failure-message]")?.textContent).toBe("This member cannot be paused now.");
  host.querySelector<HTMLButtonElement>("[data-team-action-failure-dismiss]")!.click();
  expect(onDismiss).toHaveBeenCalledOnce();
});

it("a reason without a known action still shows, without an invented name", () => {
  const host = mount(() => createComponent(ActionFailureNotice, { failure: { key: "action", code: "", message: "Too many actions wait for an answer.", source: "team-tab:a" }, onDismiss: () => {} }));
  expect(host.querySelector("[data-team-action-failure-action]")).toBeNull();
  expect(host.querySelector("[data-team-action-failure-message]")?.textContent).toBe("Too many actions wait for an answer.");
});

it("every action kind the core knows has a person's name, not its identifier", () => {
  for (const kind of kinds as string[]) {
    const name = actionName(kind);
    expect(name, kind).toBeTruthy();
    expect(name, kind).not.toContain("_");
  }
  expect(actionName("not_a_kind")).toBeUndefined();
});

it("New team says a refusal once, beside Create team", async () => {
  const host = mount(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: async (input) => ({ outcome: "done", receipt: {}, request_id: input.request_id }),
    onSaveTemplate: async () => { throw "invalid_state: The built-in template is read-only."; }, onClose: () => {},
  }));
  [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save as template")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-create-error]")?.textContent).toContain("The built-in template is read-only."));
  const said = [...host.querySelectorAll("[role=alert]")].filter((node) => node.textContent?.includes("The built-in template is read-only."));
  expect(said).toHaveLength(1);
});
