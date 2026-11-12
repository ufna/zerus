import { expect, it } from "vitest";
import { boardColumns, currentStage, memberWork } from "./pipeline-view";
import { fidelityView, teamView } from "./fixtures";

it("groups by explicit pipeline kinds in order, independently of role names", () => {
  const columns = boardColumns(fidelityView);
  expect(columns.map((column) => column.key)).toEqual(["queue", "work", "submitted", "review", "test", "accepted"]);
  expect(columns.find((column) => column.key === "test")?.cards.map((card) => card.node)).toEqual([4]);
  expect(columns.find((column) => column.key === "review")?.cards.map((card) => card.node)).toEqual([3]);
  expect(columns.find((column) => column.key === "submitted")?.cards.map((card) => card.node)).toEqual([5, 10]);
  expect(currentStage(columns.find((column) => column.key === "test")!.cards[0])?.role).toBe("quality");
  expect(columns.flatMap((column) => column.cards)).toHaveLength(fidelityView.groups.flatMap((group) => group.cards).length);
});

it("keeps null/absent kinds role-labelled and adds no empty test column", () => {
  const columns = boardColumns(teamView);
  expect(columns.map((column) => column.key)).toEqual(["queue", "work", "submitted", "role:reviewer", "accepted"]);
  expect(columns.find((column) => column.key === "role:reviewer")?.cards).toEqual([]);
  expect(columns.find((column) => column.key === "submitted")?.cards[0].node).toBe(2);
  const nulls = { ...teamView, groups: teamView.groups.map((group) => ({ ...group, cards: group.cards.map((card) => ({ ...card, kind: null, rounds: null, stages: card.stages.map((stage) => ({ ...stage, kind: null, target: null, round: null })) })) })) };
  expect(boardColumns(nulls).map((column) => column.key)).toEqual(columns.map((column) => column.key));
});

it("shows landing and custom-role columns without losing cancelled or orphan stages", () => {
  const base = teamView.groups.flatMap((group) => group.cards)[0];
  const cards = [
    { ...base, node: 11, state: "reported", stages: [{ node: 99, index: 0, kind: "land" as const, target: 11, round: 2, role: "publisher", state: "in_progress", assignee: "lead", implementer: "m1" }] },
    { ...base, node: 12, state: "cancelled" },
    { ...base, node: 100, kind: "stage" as const, role: "checks", state: "pending" },
  ];
  const columns = boardColumns({ ...teamView, groups: [{ state: "mixed", cards }] });
  expect(columns.find((column) => column.key === "land")?.cards.map((card) => card.node)).toEqual([11]);
  expect(columns.find((column) => column.key === "role:checks")?.cards.map((card) => card.node)).toEqual([100]);
  expect(columns.find((column) => column.key === "cancelled")?.cards.map((card) => card.node)).toEqual([12]);
});

it("builds member stage assignments from exact assignees on this page only", () => {
  const cards = fidelityView.groups.flatMap((group) => group.cards);
  const reviewer = fidelityView.members.find((member) => member.id === "reviewer")!;
  expect(memberWork(reviewer, cards).map((item) => item.card.node)).toEqual([10, 3]);
  expect(memberWork({ ...reviewer, id: "somebody-else" }, cards)).toEqual([]);
});
