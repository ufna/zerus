// @vitest-environment happy-dom
// The board's facts after the second review: Open task on a folded stage
// selects its parent and shows that stage, a node off the page is looked for
// through a bounded number of pages and otherwise named in a notice (finding
// 14); a stage card's worker is its assignee and the checked implementer is
// labelled apart (finding 16); contradictory stage facts are marked and never
// make a definite assignment (finding 17); an orphan stage keeps its own kind,
// target and round (finding 15, as the core now sends it).
import { afterEach, describe, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { OPEN_NODE_PAGES, TeamWorkspace } from "./TeamTab";
import { MemberCard } from "./MemberCard";
import { StageChip } from "./TeamChips";
import { TeamTaskCard } from "./TeamTaskCard";
import { boardColumns, cardWorker, memberWork, orphanStage } from "./pipeline-view";
import { teamView } from "./fixtures";
import { openTeamNode, setOpenTeamNode } from "./openNode";
import type { TaskView, TeamView } from "./types";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; setOpenTeamNode(null); document.body.replaceChildren(); });
const host = () => { const element = document.createElement("div"); document.body.append(element); return element; };
const cards = (view: TeamView) => view.groups.flatMap((group) => group.cards);
const base = cards(teamView)[0];
const member = (id: string) => teamView.members.find((candidate) => candidate.id === id)!;

function workspace(view: TeamView, pages: (string | null)[] = []) {
  const element = host();
  dispose = render(() => createComponent(TeamWorkspace, { view, busy: false, failure: null, onAction: () => {}, onPage: (cursor) => pages.push(cursor), onAddTask: () => {} }), element);
  return element;
}

describe("Open task (finding 14)", () => {
  it("selects the parent of a folded stage and shows that stage", () => {
    setOpenTeamNode({ team: teamView.bar.id, node: 5 });
    const element = workspace(teamView);
    expect(element.querySelector("[data-team-detail] h3")?.textContent).toBe("Task 2");
    expect(element.querySelector("[data-stage-focus]")?.getAttribute("aria-current")).toBe("true");
    expect(openTeamNode()).toBeNull();
  });

  it("looks for an off-page node on the next pages, at most a bounded number", async () => {
    const { createSignal } = await import("solid-js");
    const [view, setView] = createSignal<TeamView>({ ...teamView, next: "q1" });
    const pages: (string | null)[] = [];
    const element = host();
    dispose = render(() => createComponent(TeamWorkspace, { get view() { return view(); }, busy: false, failure: null, onAction: () => {}, onPage: (cursor) => pages.push(cursor), onAddTask: () => {} }), element);
    setOpenTeamNode({ team: teamView.bar.id, node: 42 });
    expect(pages).toEqual(["q1"]);
    expect(element.querySelector("[data-team-open-notice]")?.textContent).toContain("#42");
    for (let n = 1; n < OPEN_NODE_PAGES + 2; n++) setView({ ...teamView, after: `q${n}`, next: `q${n + 1}` });
    expect(pages).toHaveLength(OPEN_NODE_PAGES);
    expect(element.querySelector("[data-team-open-notice]")?.textContent).toContain("is not on the board's pages");
    expect(openTeamNode()).toBeNull();
  });

  it("starts the lookup from the first page and selects the node when a page has it", async () => {
    const { createSignal } = await import("solid-js");
    const [view, setView] = createSignal<TeamView>({ ...teamView, after: "q3", next: null });
    const pages: (string | null)[] = [];
    const element = host();
    dispose = render(() => createComponent(TeamWorkspace, { get view() { return view(); }, busy: false, failure: null, onAction: () => {}, onPage: (cursor) => pages.push(cursor), onAddTask: () => {} }), element);
    setOpenTeamNode({ team: teamView.bar.id, node: 1 });
    // Task 1 is on this fixture page, so it is selected at once.
    expect(element.querySelector("[data-team-detail] h3")?.textContent).toBe("Task 1");
    setOpenTeamNode({ team: teamView.bar.id, node: 77 });
    expect(pages).toEqual([null]);
    const found = { ...base, node: 77, title: "Task 77" };
    setView({ ...teamView, after: null, groups: [...teamView.groups, { state: "cancelled", cards: [found] }] });
    expect(element.querySelector("[data-team-detail] h3")?.textContent).toBe("Task 77");
  });

  it("a page with no next names the missing node in a notice instead of dropping it", () => {
    setOpenTeamNode({ team: teamView.bar.id, node: 99 });
    const element = workspace(teamView);
    expect(element.querySelector("[data-team-open-notice]")?.textContent).toContain("#99");
    expect(openTeamNode()).toBeNull();
  });

  it("offers the board's first page from a later page", () => {
    const pages: (string | null)[] = [];
    const element = workspace({ ...teamView, after: "q2" }, pages);
    (element.querySelector("[data-team-first-page]") as HTMLButtonElement).click();
    expect(pages).toEqual([null]);
  });
});

describe("a stage card's worker (finding 16)", () => {
  const stageCard: TaskView = { ...base, kind: "stage", state: "in_progress", role: "reviewer", assignee: "m3", implementer: "m1" };
  it("is the stage's assignee, never the implementer it checks", () => {
    expect(cardWorker(stageCard, teamView.members)?.id).toBe("m3");
    expect(cardWorker({ ...base, implementer: "m1", assignee: "m2" }, teamView.members)?.id, "a task's worker stays its implementer").toBe("m1");
  });

  it("labels the checked implementer apart on the card", () => {
    const element = host();
    dispose = render(() => createComponent(TeamTaskCard, { task: stageCard, members: teamView.members, selected: false, onSelect: () => {} }), element);
    expect(element.querySelector(".team-task-worker:not([data-checks])")?.textContent).toBe("m3");
    expect(element.querySelector("[data-checks]")?.textContent).toBe("checks the work of m1");
  });
});

describe("contradictory stage facts (finding 17)", () => {
  const nested: TaskView = { ...base, state: "in_progress", assignee: "m1", implementer: "m1", stages: [
    { node: 9, index: 0, role: "reviewer", kind: "review", target: 999, round: 2, state: "in_progress", assignee: "m3", implementer: "m1" },
  ] };
  it("a stage naming another target makes no assignment and does not place the card", () => {
    expect(memberWork(member("m3"), [nested])).toEqual([]);
    expect(boardColumns({ ...teamView, groups: [{ state: "in_progress", cards: [nested] }] }).find((column) => column.key === "work")?.cards.map((card) => card.node)).toEqual([nested.node]);
  });

  it("is marked on the chip and in the member card it is not asserted", () => {
    const element = host();
    dispose = render(() => createComponent(TeamTaskCard, { task: nested, members: teamView.members, selected: false, onSelect: () => {} }), element);
    expect(element.querySelector('[data-stage="9"]')?.getAttribute("data-inconsistent")).toBe("true");
    expect(element.querySelector('[data-stage="9"]')?.getAttribute("title")).toContain("names task #999");
    dispose(); element.replaceChildren();
    dispose = render(() => createComponent(MemberCard, { member: { ...member("m3"), state: "active" }, cards: [nested], busy: false, onAction: () => {} }), element);
    expect(element.textContent).not.toContain("Reviewing #1");
  });

  it("a round above the pipeline's rounds shows as unknown, never r4/2", () => {
    const element = host();
    dispose = render(() => createComponent(StageChip, { stage: { node: 5, index: 0, role: "quality", kind: "review", target: 1, round: 4, state: "in_progress", assignee: "m3", implementer: "m1" }, rounds: 2 }), element);
    expect(element.textContent).not.toContain("r4/2");
    expect(element.textContent).toContain("r?/2");
    expect(element.querySelector("[data-inconsistent]")).not.toBeNull();
  });
});

describe("an orphan stage (finding 15)", () => {
  it("keeps the kind, target and round the core sent for it", () => {
    const orphan: TaskView = { ...base, node: 7, kind: "stage", state: "in_progress", role: "quality", assignee: "m3", implementer: "m1", rounds: 3,
      stages: [{ node: 7, index: 0, role: "quality", kind: "review", target: 999, round: 2, state: "in_progress", assignee: "m3", implementer: "m1" }] };
    expect(orphanStage(orphan)).toMatchObject({ kind: "review", target: 999, round: 2 });
    expect(boardColumns({ ...teamView, groups: [{ state: "in_progress", cards: [orphan] }] }).find((column) => column.key === "review")?.cards).toHaveLength(1);
    const element = host();
    dispose = render(() => createComponent(MemberCard, { member: { ...member("m3"), state: "active" }, cards: [orphan], busy: false, onAction: () => {} }), element);
    expect(element.textContent).toContain("Reviewing #999");
    expect(element.textContent).toContain("r2/3");
  });
});

describe("a refreshed view of the same team (found next to finding 14)", () => {
  it("keeps the open card; another team clears it", async () => {
    const { createSignal } = await import("solid-js");
    const [view, setView] = createSignal<TeamView>(teamView);
    const element = host();
    dispose = render(() => createComponent(TeamWorkspace, { get view() { return view(); }, busy: false, failure: null, onAction: () => {}, onPage: () => {}, onAddTask: () => {} }), element);
    (element.querySelector('[data-node="2"]') as HTMLButtonElement).click();
    expect(element.querySelector("[data-team-detail] h3")?.textContent).toBe("Task 2");
    setView({ ...teamView, revision: 17, bar: { ...teamView.bar, revision: 17 } });
    expect(element.querySelector("[data-team-detail] h3")?.textContent, "a poll of the same team").toBe("Task 2");
    setView({ ...teamView, bar: { ...teamView.bar, id: "00000000-0000-4000-8000-00000000cafe" } });
    expect(element.querySelector("[data-team-detail]")).toBeNull();
  });
});

describe("Open task through a page that names itself next (second confirmation, finding 4)", () => {
  it("ends the lookup with a notice instead of waiting on the same page", async () => {
    const { createSignal } = await import("solid-js");
    const [view, setView] = createSignal<TeamView>({ ...teamView, next: "q1" });
    const pages: (string | null)[] = [];
    const element = host();
    dispose = render(() => createComponent(TeamWorkspace, { get view() { return view(); }, busy: false, failure: null, onAction: () => {}, onPage: (cursor) => pages.push(cursor), onAddTask: () => {} }), element);
    setOpenTeamNode({ team: teamView.bar.id, node: 42 });
    expect(pages).toEqual(["q1"]);
    setView({ ...teamView, after: "q1", next: "q1" });
    expect(pages).toEqual(["q1"]);
    expect(element.querySelector("[data-team-open-notice]")?.textContent).toContain("is not on the board's pages");
    expect(openTeamNode()).toBeNull();
  });
});
