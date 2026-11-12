// @vitest-environment happy-dom
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamTaskCard } from "./TeamTaskCard";
import { fidelityView } from "./fixtures";
import type { TaskView } from "./types";
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); });
function setup(task: TaskView) {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TeamTaskCard, { task, members: fidelityView.members, selected: false, onSelect: () => {} }), host);
  return host;
}
const card = () => fidelityView.groups.flatMap((group) => group.cards).find((card) => card.node === 4)!;
it("keeps the implementation round separate from its current stage round and pinned limit", () => {
  const host = setup(card());
  expect(host.querySelector(".team-task-round")?.textContent).toBe("Attempt 2");
  const chips = host.querySelectorAll(".team-stage-chip");
  expect(chips[0].textContent).toBe("Review"); expect(chips[0].querySelectorAll("svg")).toHaveLength(2);
  expect(chips[1].textContent).toBe("Testr1/3");
  expect(host.querySelector(".team-card-progress")?.children).toHaveLength(3);
  expect(host.querySelector(".team-task-card")?.classList.contains("is-working")).toBe(true);
});
it("shows the role's literal name and no invented round when optional metadata is null", () => {
  const host = setup({ ...card(), rounds: null, stages: card().stages.map((stage) => ({ ...stage, kind: null, round: null })) });
  expect(host.querySelectorAll(".team-stage-chip")[1].textContent).toBe("quality");
  expect(host.textContent).not.toContain("r1/"); expect(host.textContent).not.toContain("Test");
});
it("marks a submitted decision and a held card separately from active work", () => {
  const host = setup({ ...card(), held: true, stages: card().stages.map((stage) => ({ ...stage, state: "accepted" })) });
  expect(host.querySelector(".team-task-card")?.classList.contains("needs-decision")).toBe(true);
  expect(host.querySelector(".team-task-card")?.classList.contains("is-working")).toBe(false);
  expect(host.querySelector(".team-decision-chip")?.textContent).toBe("Decision needed");
});
