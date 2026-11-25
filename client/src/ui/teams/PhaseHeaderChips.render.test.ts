// @vitest-environment happy-dom
// zerus-uiy9p G14: the Layers header's state, mode and metric tags read as
// separate chips ("Draft", "Standard", "Path unavailable", "Parallelism
// unavailable" ran together as one line of words). The product stylesheets
// the pane imports are applied to the real PhaseMetrics markup inside the
// header PhasePane draws.
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { readFileSync } from "node:fs";
// Vitest serves CSS imports empty, also with ?raw: the files are read as they ship.
// Paths from the client folder vitest runs in (happy-dom gives no file URL).
const phases = readFileSync(`${process.cwd()}/src/ui/styles/team-phases.css`, "utf8");
const metrics = readFileSync(`${process.cwd()}/src/ui/styles/phase-metrics.css`, "utf8");
import { PhaseMetrics } from "./PhaseMetrics";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); document.head.replaceChildren(); });

it("each header tag is a chip of its own: padding and a border apart from its neighbours", () => {
  for (const css of [phases, metrics]) { const style = document.createElement("style"); style.textContent = css; document.head.append(style); }
  const host = document.createElement("section"); host.className = "phase-pane"; document.body.append(host);
  dispose = render(() => {
    const header = document.createElement("header"); header.className = "phase-header";
    for (const word of ["Draft", "Standard"]) { const tag = document.createElement("span"); tag.className = "tag"; tag.textContent = word; header.append(tag); }
    const metricsHost = document.createElement("div"); header.append(metricsHost);
    render(() => createComponent(PhaseMetrics, { metrics: null, readFailed: true }), metricsHost);
    return header;
  }, host);
  const tags = [...host.querySelectorAll<HTMLElement>(".tag")];
  expect(tags.map((tag) => tag.textContent?.trim())).toEqual(["Draft", "Standard", "Path unavailable", "Parallelism unavailable"]);
  for (const tag of tags) {
    const style = getComputedStyle(tag);
    expect(style.paddingLeft, tag.textContent!).not.toBe("0px");
    expect(style.paddingLeft, tag.textContent!).not.toBe("");
    expect(style.borderLeftStyle, tag.textContent!).toBe("solid");
  }
});
