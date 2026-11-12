// @vitest-environment happy-dom
// Waiting for you's Open task: the team's Team tab takes the node it names and
// selects that card, so the decision on reported work is made on the board.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
vi.mock("./TeamConfirmDialog", () => ({ TeamConfirmDialog: () => null }));
import { TeamWorkspace } from "./TeamTab";
import { openTeamNode, setOpenTeamNode } from "./openNode";
import { teamView } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; setOpenTeamNode(null); document.body.replaceChildren(); });

function setup() {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TeamWorkspace, {
    view: teamView, busy: false, failure: null, onAction: () => {}, onPage: () => {}, onAddTask: () => {},
  }), host);
  return host;
}

it("opens the named node's card on the board", () => {
  setOpenTeamNode({ team: teamView.bar.id, node: 2 });
  const host = setup();
  expect(host.querySelector("[data-team-detail]")?.textContent).toContain("Task 2");
  expect(host.querySelector('[data-node="2"]')?.getAttribute("aria-pressed")).toBe("true");
  expect(openTeamNode()).toBeNull();
});

it("ignores a node meant for another team", () => {
  setOpenTeamNode({ team: "00000000-0000-4000-8000-000000000000", node: 2 });
  const host = setup();
  expect(host.querySelector("[data-team-detail]")).toBeNull();
  expect(openTeamNode()).not.toBeNull();
});
