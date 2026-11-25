// @vitest-environment happy-dom
// The Feed reads as sentences (zerus-rzvct G6): each journal event names who
// acted (you, the team service or a member), what happened, and the task and
// member it is about, on separate elements; the exact journal text stays as
// the row's title. Rows are the core's projection of the product-written
// journal (crates/team-plan/fixtures/cli-journal.json).
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { TeamFeed } from "./TeamDrawers";
import pages from "../../../core/src/teams/fixtures/drawers.json";
import providerStop from "../../../core/src/teams/fixtures/provider-stop-feed.json";
import land from "../../../core/src/teams/fixtures/landing-drawers.json";
import type { DrawerPage, DrawerRow } from "../../store/teamDrawers";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const feed = pages.feed as DrawerPage;
function show(rows: DrawerRow[]) {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(TeamFeed, { rows }), host);
  return host;
}
const read = (host: HTMLElement, seq: number) => {
  const row = host.querySelector(`[data-feed-seq="${seq}"]`)!;
  const text = (selector: string) => row.querySelector(selector)?.textContent ?? null;
  return { actor: text("[data-feed-actor]"), sentence: text("[data-feed-sentence]"), task: text("[data-feed-task]"), member: text("[data-feed-member]"), detail: text("[data-feed-detail]"), title: row.getAttribute("title") };
};

it("says who did what to which task and member, in separate elements", () => {
  const host = show(feed.rows);
  expect(read(host, 1)).toEqual({ actor: "You", sentence: "Team created", task: null, member: null, detail: null, title: "create at revision 1" });
  expect(read(host, 2)).toEqual({ actor: "Team service", sentence: "Member session linked", task: null, member: "lead", detail: null, title: "set_member_session at revision 2" });
  expect(read(host, 5)).toMatchObject({ actor: "Team service", sentence: "Task state observed", task: "#1", member: null });
  expect(read(host, 8)).toMatchObject({ actor: "Team service", sentence: "Hand-out prepared", task: "#2", member: "m1" });
  expect(read(host, 13)).toMatchObject({ sentence: "Report recorded", task: "#2", member: "m1" });
  expect(read(host, 15)).toMatchObject({ actor: "m1", sentence: "Question asked", task: "#2" });
  expect(read(host, 16)).toMatchObject({ actor: "You", sentence: "Question answered" });
  const header = host.querySelector('[data-feed-seq="1"] header');
  expect(header?.querySelector("time")).not.toBeNull();
  expect(header?.querySelector("[data-feed-actor]")).not.toBeNull();
});

it("never shows an op code: every product op has a sentence and an unknown one reads as words", () => {
  const host = show(feed.rows);
  for (const row of feed.rows) expect(read(host, row.seq).sentence, row.kind).not.toContain("_");
  dispose?.(); document.body.replaceChildren();
  const later = show([{ ...feed.rows[0], seq: 99, kind: "some_future_op", text: "some_future_op at revision 99" }]);
  expect(read(later, 99).sentence).toBe("some future op");
});

it("keeps what the journal's text adds in visible text, not only in a hover title", () => {
  // Product-written: the hand-out result says it was submitted.
  const host = show(feed.rows);
  expect(read(host, 9)).toMatchObject({ sentence: "Hand-out result recorded", detail: "Submitted", title: "handout_outcome at revision 9: submitted" });
  dispose?.(); document.body.replaceChildren();
  // The writer's other shapes (src/teams/ops/journal.rs): a refusal with its
  // reason, a superseded decision, and a service sentence of its own.
  const base = feed.rows.find((row) => row.seq === 9)!;
  const rows: DrawerRow[] = [
    { ...base, seq: 91, text: "handout_outcome at revision 9: refused: The member's session asks a question." },
    { ...base, seq: 92, text: "handout_outcome at revision 9: uncertain: no receipt" },
    { ...base, seq: 93, kind: "return", text: "return at revision 12; decision 3 (reassign) superseded by decision 4 (return)" },
    { ...base, seq: 94, kind: "person_override", text: "Task 2 left the team." },
    { ...base, seq: 95, kind: "close_card", text: "The member's session is ready again." },
  ];
  const shapes = show(rows);
  expect(read(shapes, 91).detail).toBe("Refused: The member's session asks a question.");
  expect(read(shapes, 92).detail).toBe("Outcome unknown: no receipt");
  expect(read(shapes, 93).detail).toBe("decision 3 (reassign) superseded by decision 4 (return)");
  expect(read(shapes, 94)).toMatchObject({ sentence: "You changed the task outside the team", detail: "Task 2 left the team." });
  expect(read(shapes, 95).detail).toBe("The member's session is ready again.");
});

it("keeps the English skip detail for the task named by the writer", () => {
  const host = show([land.report, land.verdict]);
  for (const row of [land.report, land.verdict]) {
    expect(read(host, row.seq)).toMatchObject({
      detail: "Land stages skipped for task #1 because landing is disabled in Team settings.",
      title: row.text,
    });
  }
});

it("shows the confirmation sentence on a provider-stop release", () => {
  const host = show([providerStop]);
  expect(read(host, providerStop.seq)).toMatchObject({
    sentence: "Task of a member stopped by its provider returned",
    detail: "The service confirmed that the member stopped after a provider error.",
    task: "#21",
    member: "m3",
    title: providerStop.text,
  });
});
