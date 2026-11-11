// @vitest-environment happy-dom
// New team's M1 guide: how a standing team works and what Create team does.
import { afterEach, expect, it } from "vitest";
import { createComponent, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { NewTeamHowTo, NewTeamIntroSteps } from "./NewTeamGuide";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup() {
  const host = document.createElement("div");
  document.body.append(host);
  const both = () => [createComponent(NewTeamIntroSteps, {}), createComponent(NewTeamHowTo, { open: true, onToggle: () => {} })];
  dispose = render(() => both() as unknown as JSX.Element, host);
  return host;
}

it("tells how a standing team works", () => {
  const host = setup();
  expect(host.textContent).toContain("How a team works");
  expect(host.textContent).toContain("Nothing runs before you press Create team.");
  expect(host.textContent).toContain("The template brings the roles, the pipelines and the team's settings.");
});
