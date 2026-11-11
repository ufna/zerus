// @vitest-environment happy-dom
// The team bar (the fork's strip, adapted to M1): the name, the status, the
// progress track and count from the core's counts, the chips and the team's
// own operations.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamBar } from "./team-bar";
import { teamView } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

const setup = (view = teamView, onAction = vi.fn(), width = 1300) => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TeamBar, { view, busy: false, onAction, width }), host);
  return { host, onAction };
};

it("shows the team's name, status and progress", () => {
  const { host } = setup();
  expect(host.querySelector(".team-name")?.textContent).toBe("Fixes");
  expect(host.querySelector(".tm-state-badge")?.textContent).toBe("running");
  expect(host.querySelector(".tm-state-badge")?.getAttribute("data-state")).toBe("running");
  expect(host.querySelector(".team-count")?.textContent).toBe("0/2");
  // One segment for each non-zero count: one reported and one waiting.
  expect(host.querySelectorAll(".team-progress .seg").length).toBe(2);
});

it("shows the counts the core gave", () => {
  const { host } = setup();
  const chips = host.querySelector("[data-strip-chips]")?.textContent ?? "";
  expect(chips).toContain("2 active");
  expect(chips).toContain("1 inbox");
  expect(chips).toContain("1 cards");
});

it("offers the team's own operations by its status", () => {
  const { host, onAction } = setup();
  const buttons = [...host.querySelectorAll("[data-strip-actions] button")].map((button) => button.getAttribute("data-strip-action"));
  expect(buttons).toEqual(["pause", "stop"]);
  (host.querySelector('[data-strip-action="stop"]') as HTMLButtonElement).click();
  expect(onAction).toHaveBeenCalledWith("stop");
});

it("folds the actions by the strip's own width", () => {
  expect(setup(teamView, vi.fn(), 1300).host.querySelector("[data-strip-actions]")).not.toBeNull();
  expect(setup(teamView, vi.fn(), 1000).host.querySelector("[data-strip-actions]")).toBeNull();
  expect(setup(teamView, vi.fn(), 1000).host.querySelector(".team-bar")?.getAttribute("data-compact")).toBe("low");
});

it("offers resume to a paused team and archive to a stopped one", () => {
  const paused = { ...teamView, bar: { ...teamView.bar, status: "paused" } };
  expect([...setup(paused).host.querySelectorAll("[data-strip-actions] button")].map((button) => button.getAttribute("data-strip-action"))).toEqual(["resume", "stop"]);
  const stopped = { ...teamView, bar: { ...teamView.bar, status: "stopped" } };
  expect([...setup(stopped).host.querySelectorAll("[data-strip-actions] button")].map((button) => button.getAttribute("data-strip-action"))).toEqual(["archive"]);
});
