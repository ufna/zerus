// @vitest-environment happy-dom
// zerus-63rru.197 (Teams UI wave 3a): the dead ends of the audit of
// 2026-11-19. Rows come from the product fixtures where a product writer
// pins them (drawers.json, provider-stop.json, the teamView projection);
// handwritten_ tests build the kinds no fixture writes yet, with the
// writer's own text.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { MenuLayer } from "../components/Overlays";
import { setMenu } from "../../store/ui";
import { TeamInbox } from "./TeamDrawers";
import { TeamReports } from "./TeamReports";
import { ProposalCards } from "./ProposalCards";
import { WaitingCard } from "./Waiting";
import { MemberCard } from "./MemberCard";
import { TaskPlacement } from "./TaskPlacement";
import { TeamWorkspace } from "./TeamTab";
import { memberActions } from "./actions";
import { waitingKindSentence } from "./decisions";
import { openTeamNode, setOpenTeamNode } from "./openNode";
import { phaseFocus, takePhaseFocus } from "../../store/team-phases";
import { teamView, TEAM_ID } from "./fixtures";
import pages from "../../../core/src/teams/fixtures/drawers.json";
import providerStop from "../../../core/src/teams/fixtures/provider-stop.json";
import type { DrawerPage, DrawerRow } from "../../store/teamDrawers";
import type { WaitingItem } from "../../store/teams";
import type { TaskView } from "./types";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); setMenu(undefined); setOpenTeamNode(null); takePhaseFocus(TEAM_ID); });
const mount = (child: () => import("solid-js").JSX.Element) => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(child, host); return host;
};
const inbox = pages.inbox as DrawerPage;
const report = () => inbox.rows.find((row) => row.kind === "report")!;
const question = () => inbox.rows.find((row) => row.kind.startsWith("question:"))!;
const buttons = (host: HTMLElement) => [...host.querySelectorAll<HTMLButtonElement>("button")];
const byText = (host: HTMLElement, text: string) => buttons(host).find((button) => button.textContent === text);

// A4: a report's decisions are made beside its evidence, in the inspector.
it("a product report row offers Open task instead of an ungated Accept or Reject", () => {
  const action = vi.fn(), opened = vi.fn();
  const host = mount(() => createComponent(TeamReports, { rows: inbox.rows, busy: false, onAction: action, team: TEAM_ID, onOpenNode: opened }));
  expect(host.querySelector("[data-report-accept]")).toBeNull();
  expect(byText(host, "Reject…")).toBeUndefined();
  const open = host.querySelector<HTMLButtonElement>("[data-report-open]")!;
  expect(open.textContent).toBe("Open task");
  open.click();
  expect(openTeamNode()).toEqual({ team: TEAM_ID, node: report().node });
  expect(opened).toHaveBeenCalledTimes(1);
  expect(action).not.toHaveBeenCalled();
  expect(host.textContent).toContain("Accept or return this report from the task, beside its verdicts.");
});

it("the inbox shows a product report row the same way", () => {
  const host = mount(() => createComponent(TeamInbox, { rows: inbox.rows, busy: false, onAction: vi.fn(), team: TEAM_ID, status: "running" }));
  expect(host.querySelector("[data-report-accept]")).toBeNull();
  expect(host.querySelector("[data-report-open]")).not.toBeNull();
});

// C8: no Answer box on a team that cannot take the reply.
it("handwritten an open question on a stopped team has no Answer box and says why", () => {
  const open: DrawerRow = { ...question(), state: "open", reply: null };
  const running = mount(() => createComponent(TeamInbox, { rows: [open], busy: false, onAction: vi.fn(), team: TEAM_ID, status: "running" }));
  expect(running.querySelector("[data-inbox-answer]")).not.toBeNull();
  dispose?.(); document.body.replaceChildren();
  const stopped = mount(() => createComponent(TeamInbox, { rows: [open], busy: false, onAction: vi.fn(), team: TEAM_ID, status: "stopped" }));
  expect(stopped.querySelector("[data-inbox-answer]")).toBeNull();
  expect(stopped.querySelector("input")).toBeNull();
  expect(stopped.textContent).toContain("Questions can be answered only while the team is running or paused.");
});

// C6: every open item that is not a question or a report says what closes it.
it("a product provider-stop item says it closes with its task", () => {
  const row = providerStop.task as DrawerRow;
  expect(row.kind.startsWith("provider_stop:")).toBe(true);
  expect(row.state).toBe("open");
  const host = mount(() => createComponent(TeamInbox, { rows: [row], busy: false, onAction: vi.fn(), team: TEAM_ID, status: "running" }));
  expect(host.querySelector("[data-inbox-closes]")?.textContent)
    .toBe(`This item closes by itself when task #${row.node} is reported again, reopened or settled.`);
});

it("handwritten team-wide items name what closes them, and a sent-back phase says nothing does", () => {
  const base = { ...question(), node: null, generation: null, state: "open", reply: null, notes: [], card: null };
  const rows: DrawerRow[] = [
    { ...base, seq: 101, kind: "planning_goal", text: "Plan the work." },
    { ...base, seq: 102, kind: "phase_rejected:00000000-0000-4000-8000-000000000102", text: "Phase 2 was rejected: split it." },
    { ...base, seq: 103, kind: "phase_sent_back:00000000-0000-4000-8000-000000000103", text: "Phase 2 needs explicit corrections: add tests" },
    { ...base, seq: 104, kind: "lead_admission:7", text: "Tasks wait for admission." },
    { ...base, seq: 105, kind: "lead_routing:7", text: "Tasks wait for routing." },
    { ...base, seq: 106, kind: "something_new", text: "A new kind." },
  ];
  const host = mount(() => createComponent(TeamInbox, { rows, busy: false, onAction: vi.fn(), team: TEAM_ID, status: "running" }));
  const line = (seq: number) => host.querySelector(`[data-inbox="${seq}"] [data-inbox-closes]`)?.textContent;
  expect(line(101)).toBe("This item closes when you or the lead submit a phase draft.");
  expect(line(102)).toBe("This item closes when the phase's draft or corrections are submitted again, or its rejection is undone.");
  expect(line(103)).toBe("This item records that the phase was sent back; nothing closes it. Make the corrections in Layers.");
  expect(line(104)).toBe("This item closes by itself when no task waits for admission.");
  expect(line(105)).toBe("This item closes by itself when no task waits for routing.");
  expect(line(106)).toBe("Nothing in this window closes this item; the team's service closes it when its cause is settled.");
  expect(host.querySelectorAll("button")).toHaveLength(0);
  dispose?.(); document.body.replaceChildren();
  const ended = mount(() => createComponent(TeamInbox, { rows, busy: false, onAction: vi.fn(), team: TEAM_ID, status: "stopped" }));
  expect(ended.querySelector('[data-inbox="104"] [data-inbox-closes]')?.textContent)
    .toBe("The team has ended, so this item stays open as a record.");
});

// C5: a phase card's way out is its phase; hiding the card decides nothing.
const PHASE_CARD = (over: Partial<WaitingItem> = {}): WaitingItem => ({
  id: `${TEAM_ID}:9`, team: TEAM_ID, card: 9, kind: "phase_review", node: null, state: "open", by: "service", created_ms: 1,
  text: "Phase 2 (Release) has a draft ready for your review. Open the planned phase to approve or reject it.",
  decisions: ["acknowledged"], phase_reference: { phase: 2, generation: 3, purpose: "review" }, ...over,
});

it("handwritten a phase card in Waiting puts Open phase first and hides the card without deciding", () => {
  const decide = vi.fn(), openTeam = vi.fn();
  const host = mount(() => createComponent(WaitingCard, { item: PHASE_CARD(), teamName: "Fixes", now: 2, busy: false, onDecide: decide, onOpenTeam: openTeam }));
  const actions = [...host.querySelectorAll<HTMLButtonElement>(".waiting-decisions button")];
  expect(actions.map((button) => button.textContent)).toEqual(["Open phase", "Hide card"]);
  expect(actions[0].classList.contains("primary")).toBe(true);
  expect(actions[1].classList.contains("primary")).toBe(false);
  expect(host.textContent).toContain("Hiding the card does not decide the phase; it keeps waiting in Layers.");
  actions[1].click();
  expect(decide).toHaveBeenCalledWith(expect.objectContaining({ card: 9 }), "acknowledged");
  actions[0].click();
  expect(phaseFocus()).toMatchObject({ team: TEAM_ID, phase: 2 });
  expect(openTeam).toHaveBeenCalledTimes(1);
});

it("handwritten the Cards drawer offers the same Open phase and Hide card", () => {
  const action = vi.fn(), opened = vi.fn();
  const row = { ...question(), seq: 9, kind: "phase_review", card: PHASE_CARD({ kind: "phase_acceptance", phase_reference: { phase: 2, generation: 3, purpose: "accept" } }) } as DrawerRow;
  const host = mount(() => createComponent(ProposalCards, { rows: [row], busy: false, onAction: action, team: TEAM_ID, onOpenNode: opened }));
  const actions = [...host.querySelectorAll<HTMLButtonElement>(".team-drawer-actions button")];
  expect(actions.map((button) => button.textContent)).toEqual(["Open phase", "Hide card"]);
  actions[0].click();
  expect(phaseFocus()).toMatchObject({ team: TEAM_ID, phase: 2 });
  expect(opened).toHaveBeenCalledTimes(1);
  actions[1].click();
  expect(action).toHaveBeenCalledWith("decide", { card: 9, decision: "acknowledged" });
});

it("handwritten other cards keep their decision words", () => {
  const host = mount(() => createComponent(WaitingCard, { item: PHASE_CARD({ kind: "note", phase_reference: null }), teamName: "Fixes", now: 2, busy: false, onDecide: vi.fn(), onOpenTeam: vi.fn() }));
  expect([...host.querySelectorAll(".waiting-decisions button")].map((button) => button.textContent)).toEqual(["Acknowledge"]);
});

// The rejected merge: the card says who reverts it, since Zerus cannot.
it("handwritten a rejected merge card names who reverts it", () => {
  const sentence = waitingKindSentence({ kind: "rejected_merge", parameters: undefined, text: "x" });
  expect(sentence).toBe("The rejection did not undo the merge, and Zerus cannot revert it: ask the lead, which has the same item in its inbox, or revert the merge on the integration branch yourself, then acknowledge this card.");
});

// A3: a member that never started is started again by its start card's Retry.
it("a product not-created member offers Launch again, which opens the team's cards", () => {
  const m2 = teamView.members.find((member) => member.id === "m2")!;
  expect(m2.state).toBe("not_created");
  expect(memberActions(m2, "running").map((action) => action.id)).toContain("launch_again");
  expect(memberActions(m2, "stopped").map((action) => action.id)).not.toContain("launch_again");
  expect(memberActions(teamView.members[1], "running").map((action) => action.id)).not.toContain("launch_again");
  const onAction = vi.fn();
  const host = mount(() => [createComponent(MemberCard, { teamStatus: "running", member: m2, busy: false, onAction }), createComponent(MenuLayer, {})]);
  (host.querySelector('[aria-label="Member actions"]') as HTMLButtonElement).click();
  const item = [...document.querySelectorAll<HTMLButtonElement>(".menu-item")].find((button) => button.textContent === "Launch again…")!;
  expect(item).toBeDefined();
  item.click();
  expect(onAction).toHaveBeenCalledWith("launch_again", "m2");
  expect(host.textContent).toContain("Zerus starts this member when there is room. If its start failed, Retry on its card in Cards starts it again.");
});

// A2: an approved phase's task is changed by sending the phase back.
it("handwritten an approved phase's task names the phase send-back and opens its phase", () => {
  const card = teamView.groups.find((group) => group.state === "pending")!.cards[0];
  const task = { ...card, phase: 3 } as TaskView;
  const host = mount(() => createComponent(TaskPlacement, { view: teamView, task, busy: false, unresolved: false, otherFormOpen: false, onOpen: vi.fn(), onAction: vi.fn() }));
  expect(host.textContent).toContain("To change an approved phase's tasks, send the phase back with corrections once all its tasks are decided.");
  expect(host.textContent).not.toContain("plan edit");
  const open = host.querySelector<HTMLButtonElement>("[data-placement-open-phase]")!;
  expect(open.textContent).toBe("Open phase 3");
  open.click();
  expect(phaseFocus()).toMatchObject({ team: TEAM_ID, phase: 3 });
});

it("Launch again in the real Team tab opens the team's cards and sends nothing", () => {
  const action = vi.fn();
  const host = mount(() => [createComponent(TeamWorkspace, { view: teamView, busy: false, failure: null, onAction: action, onPage: vi.fn(), onAddTask: vi.fn() }), createComponent(MenuLayer, {})]);
  host.querySelector<HTMLButtonElement>("[data-team-members-toggle]")!.click();
  host.querySelector<HTMLButtonElement>('[data-member="m2"] [aria-label="Member actions"]')!.click();
  [...document.querySelectorAll<HTMLButtonElement>(".menu-item")].find((button) => button.textContent === "Launch again…")!.click();
  expect(host.querySelector('[data-team-drawer-panel="cards"]')).not.toBeNull();
  expect(action).not.toHaveBeenCalled();
});
