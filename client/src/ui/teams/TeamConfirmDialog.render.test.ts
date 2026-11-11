// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

// The core asks for the person's confirmation before a destructive action:
// the dialog shows the core's own words, and only its action sends it again.
const f = vi.hoisted(() => ({
  confirm: vi.fn(), cancel: vi.fn(),
  asking: (() => null) as () => null,
  setAsking: (_: unknown) => {},
}));
vi.mock("../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [asking, setAsking] = createSignal<{
    call: { request_id: string; action: { kind: string } };
    confirmation: { name: string; title: string; text: string; action: string };
  } | null>(null);
  f.asking = asking as unknown as () => null;
  f.setAsking = setAsking as (value: unknown) => void;
  return {
    teams: {
      confirmation: asking, confirm: f.confirm, cancelConfirmation: f.cancel,
      run: vi.fn(), check: vi.fn(), dismiss: vi.fn(), uncertain: () => null,
    },
  };
});
vi.mock("../lib/icons", () => ({ Icon: () => null }));
import { TeamConfirmDialog } from "./TeamConfirmDialog";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; f.setAsking(null); document.body.replaceChildren(); vi.clearAllMocks(); });

const show = () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TeamConfirmDialog, {}), host);
  return host;
};

it("shows the core's confirmation and sends nothing until its action is pressed", () => {
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  f.setAsking({
    call: { request_id: "11111111-2222-4333-8444-555555555555", action: { kind: "stop" } },
    confirmation: { name: "teamStopConfirm", title: "Stop team?", text: "Stop Fixes? Every open task is cancelled.", action: "Stop team" },
  });
  const host = show();
  const dialog = host.querySelector("dialog.team-confirm")!;
  expect(dialog.getAttribute("data-name")).toBe("teamStopConfirm");
  expect(host.textContent).toContain("Stop team?");
  expect(host.textContent).toContain("Every open task is cancelled.");
  expect(f.confirm).not.toHaveBeenCalled();
  const action = [...host.querySelectorAll("button")].find((button) => button.textContent === "Stop team") as HTMLButtonElement;
  action.click();
  expect(f.confirm).toHaveBeenCalledTimes(1);
});

it("cancels without sending", () => {
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  f.setAsking({
    call: { request_id: "11111111-2222-4333-8444-555555555555", action: { kind: "archive" } },
    confirmation: { name: "teamArchiveConfirm", title: "Archive team?", text: "Archive it?", action: "Archive" },
  });
  const host = show();
  const cancel = [...host.querySelectorAll("button")].find((button) => button.textContent === "Cancel") as HTMLButtonElement;
  cancel.click();
  expect(f.cancel).toHaveBeenCalledTimes(1);
  expect(f.confirm).not.toHaveBeenCalled();
});

it("focuses rightmost Cancel and leaves Enter activation to the focused button (review 8)", async () => {
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  f.setAsking({ call: { request_id: "11111111-2222-4333-8444-555555555555", action: { kind: "stop" } }, confirmation: { name: "stop", title: "Stop team?", text: "Every unfinished task is cancelled.", action: "Stop team" } });
  const host = show(); await Promise.resolve();
  const buttons = [...host.querySelectorAll<HTMLButtonElement>("button")];
  expect(buttons.map((button) => button.textContent)).toEqual(["Stop team", "Cancel"]);
  expect(document.activeElement).toBe(buttons[1]);
  for (const button of buttons) {
    button.focus();
    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    button.dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(false);
  }
  expect(f.confirm).not.toHaveBeenCalled(); expect(f.cancel).not.toHaveBeenCalled();
  buttons[1].click(); expect(f.cancel).toHaveBeenCalledTimes(1); expect(f.confirm).not.toHaveBeenCalled();
  buttons[0].click(); expect(f.confirm).toHaveBeenCalledTimes(1);
});
