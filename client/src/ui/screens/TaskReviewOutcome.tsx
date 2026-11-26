import { createMemo, Show } from "solid-js";
import { tr } from "../../i18n";
import { machineCards } from "../../store/machines";
import { projectSessions } from "../../store/taskContext";
import { teams } from "../../store/teams";
import { openTeamPage } from "../teams/team-navigation";
import { stateLabel } from "../teams/team-model";
import { assigneeKey, outcomeLabel, quickAssignee, type QuickAssignee } from "./quick-review";
import { reviewNext, sessionIdentity, sessionLabel, type Task } from "./tasks-ui";

// Task writes use the local hgs. Its replica can read decisions made on
// peers, but it can only verify a new check assignee on its own computer.
export const reviewSessionChoices = createMemo(() => {
  const rows = new Map<string, { assignee: QuickAssignee; label: string } | null>();
  for (const session of projectSessions()) {
    if (session.computer !== "") continue;
    const id = sessionIdentity(session);
    const assignee = quickAssignee(machineCards[session.computer]?.machine_id, id);
    if (!assignee) continue;
    const key = assigneeKey(assignee);
    rows.set(key, rows.has(key) ? null : { assignee,
      label: sessionLabel({ host: session.computer, name: session.name, id, known: true }) });
  }
  return rows;
});

function teamFacts(project: string, task: Task) {
  const team = task.team, view = teams.team();
  if (!team || view?.bar.id !== team.id || view.bar.home !== team.home) return;
  const cards = view.groups.flatMap((group) => group.cards)
    .filter((card) => card.kind !== "stage" && card.task?.project === project && card.task.uuid === task.uuid);
  if (cards.length !== 1) return;
  const card = cards[0];
  const stages = card.stages.filter((stage) => ["in_progress", "reported"].includes(stage.state));
  const stage = stages.length === 1 ? stages[0] : undefined;
  const holder = stages.length ? stage?.holder : card.holder;
  const policy = card.phase != null ? view.settings?.phase_acceptance : undefined;
  return {
    state: stateLabel(stage?.state ?? card.state),
    actor: holder || undefined,
    phasePolicy: policy === "lead" ? tr("Team lead") : policy === "person" ? tr("Person") : undefined,
  };
}

function executorLabel(task: Task): string | undefined {
  if (task.executor.known && task.executor.id && task.executor.name) return sessionLabel(task.executor);
}

export function reviewOutcome(project: string, task: Task): { label: string; actor?: string } | undefined {
  if (task.stage !== "review") return;
  if (task.teamRouted) return { label: tr("Team review"), actor: teamFacts(project, task)?.actor };
  if (task.quickStatus === "unresolved") return { label: tr("History reconciliation") };
  const disposition = task.quickReport?.disposition;
  if (!disposition) {
    if (task.quickReport) return { label: tr("Review needed") };
    const legacy = reviewNext(task);
    return legacy ? { label: legacy } : undefined;
  }
  let actor: string | undefined;
  if (disposition.kind === "ready") actor = tr("Your decision");
  if (disposition.kind === "check") actor = reviewSessionChoices().get(assigneeKey(disposition.assignee))?.label ?? task.quickReport?.assigneeLabel;
  if (disposition.kind === "rework") actor = executorLabel(task);
  return { label: outcomeLabel(disposition.kind), actor };
}

export function TaskReviewOutcome(props: { project: string; task: Task }) {
  const outcome = () => reviewOutcome(props.project, props.task);
  return <Show when={outcome()}>{(shown) =>
    <p class="tk-review-owner ell" title={shown().actor ? tr("%1 — %2", shown().label, shown().actor!) : shown().label}>
      {shown().actor ? tr("%1 — %2", shown().label, shown().actor!) : shown().label}
    </p>
  }</Show>;
}

export function TaskTeamReview(props: { project: string; task: Task }) {
  const facts = () => teamFacts(props.project, props.task);
  return <section class="tv-doc" aria-label={tr("Team review")}>
    <header><span class="eyebrow">{tr("Team review")}</span></header>
    <Show when={facts()}>{(facts) => <>
      <div>{tr("Team stage: %1", facts().state)}</div>
      <Show when={facts().actor}>{(holder) => <div>{tr("Current holder: %1", holder())}</div>}</Show>
      <Show when={facts().phasePolicy}>{(policy) => <div>{tr("Phase acceptance: %1", policy())}</div>}</Show>
    </>}</Show>
    <Show when={props.task.team && teams.available() === true}>
      <button type="button" class="btn small ghost" onClick={() => {
        if (props.task.team && teams.available() === true) openTeamPage(props.task.team.id);
      }}>{tr("Open team page")}</button>
    </Show>
  </section>;
}
