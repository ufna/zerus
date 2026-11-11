import type { MemberView, StageKind, StageView, TaskView, TeamView } from "./types";

// Presentation of the bounded page the core sent. These functions never read
// another page or turn a role name, account or idle session into pipeline facts.
export const closedStage = (stage: Pick<StageView, "state">) => ["accepted", "cancelled"].includes(stage.state);
export const orderedStages = (card: TaskView) => [...card.stages].sort((a, b) => a.index - b.index || a.node - b.node);
export function currentStage(card: TaskView): StageView | undefined {
  if (card.kind === "stage") return orphanStage(card);
  const stages = orderedStages(card);
  return stages.find((stage) => stage.state === "in_progress") ?? stages.find((stage) => !closedStage(stage));
}
export const orphanStage = (card: TaskView): StageView => ({ node: card.node, index: 0, role: card.role, state: card.state, assignee: card.assignee, implementer: card.implementer });
export const stageKey = (stage: Pick<StageView, "kind" | "role">) => stage.kind || `role:${stage.role}`;
export interface BoardColumn { key: string; kind?: StageKind; role?: string; cards: TaskView[] }
export function boardColumns(view: TeamView): BoardColumn[] {
  const cards = view.groups.flatMap((group) => group.cards);
  const stageColumns = new Map<string, BoardColumn>();
  const ensure = (stage: StageView) => {
    const key = stageKey(stage);
    if (!stageColumns.has(key)) stageColumns.set(key, { key, kind: stage.kind ?? undefined, role: stage.kind ? undefined : stage.role, cards: [] });
  };
  for (const card of cards) {
    if (card.kind === "stage") ensure(orphanStage(card));
    for (const stage of card.stages) ensure(stage);
  }
  const columns: BoardColumn[] = ["queue", "work", "submitted"].map((key) => ({ key, cards: [] }));
  const kinds: string[] = ["review", "test", "land"];
  columns.push(...[...stageColumns.values()].sort((a, b) => {
    const rank = (key: string) => kinds.includes(key) ? kinds.indexOf(key) : kinds.length;
    return rank(a.key) - rank(b.key);
  }), { key: "accepted", cards: [] });
  for (const card of cards) {
    const stage = currentStage(card);
    const key = card.state === "accepted" ? "accepted" : card.state === "cancelled" ? "cancelled"
      : card.kind === "stage" && stage ? stageKey(stage)
      : ["waiting", "pending", "draft"].includes(card.state) ? "queue"
      : stage && ["in_progress", "reported"].includes(stage.state) ? stageKey(stage) : card.state === "in_progress" ? "work" : card.state === "reported" ? "submitted" : card.state;
    let column = columns.find((column) => column.key === key);
    if (!column) { column = { key, cards: [] }; columns.push(column); }
    column.cards.push(card);
  }
  return columns;
}
export const cardWorker = (card: TaskView, members: readonly MemberView[]) => members.find((member) => member.id === (card.implementer || card.assignee));
export interface MemberWork { card: TaskView; stage?: StageView }
export function memberWork(member: MemberView, cards: readonly TaskView[]): MemberWork[] {
  const work: MemberWork[] = [];
  for (const card of cards) {
    if (["accepted", "cancelled"].includes(card.state)) continue;
    if (card.kind === "stage") {
      if (card.assignee === member.id) work.push({ card, stage: orphanStage(card) });
      continue;
    }
    if (card.assignee === member.id) work.push({ card });
    for (const stage of card.stages) if (!closedStage(stage) && stage.assignee === member.id) work.push({ card, stage });
  }
  return work;
}
