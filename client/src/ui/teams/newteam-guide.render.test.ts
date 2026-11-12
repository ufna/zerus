// @vitest-environment happy-dom
// New team's M1 guide: how a standing team works and what Create team does.
import { afterEach, expect, it } from "vitest";
import { createComponent, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { NewTeamHowTo, NewTeamIntroSteps } from "./NewTeamGuide";
import { translateWithCatalog } from "../../i18n";
import russian from "../../i18n.ru.json";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function setup() {
  const host = document.createElement("div");
  document.body.append(host);
  const both = () => [createComponent(NewTeamIntroSteps, {}), createComponent(NewTeamHowTo, { open: true, onToggle: () => {} })];
  dispose = render(() => both() as unknown as JSX.Element, host);
  return host;
}

it("says that accepted work is merged into the integration branch, in both languages without the stumble", () => {
  const host = setup();
  const steps = [...host.querySelectorAll("[data-intro-steps] li")].map((step) => step.textContent);
  expect(steps[2]).toBe("Accepted work is merged into one integration branch; you follow it in the Team tab.");
  expect(host.textContent).not.toContain("accepts accepted");
  expect(translateWithCatalog(russian, "ru", steps[2]!)).toBe("Принятая работа сливается в одну ветку интеграции; вы следите за ней на вкладке «Команда».");
});

it("tells how a standing team works", () => {
  const host = setup();
  expect(host.textContent).toContain("How a team works");
  expect(host.textContent).toContain("Nothing runs before you press Create team.");
  expect(host.textContent).toContain("The template brings the roles, the pipelines and the team's settings.");
});
