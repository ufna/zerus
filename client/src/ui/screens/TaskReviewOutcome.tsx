import { createEffect, createMemo, on, createResource, createSignal, onCleanup, Show } from "solid-js";
import { tr } from "../../i18n";
import { machineCards } from "../../store/machines";
import { projectSessions } from "../../store/taskContext";
import { teams, type TeamView } from "../../store/teams";
import { openTeamPage } from "../teams/team-navigation";
import { taskState } from "../teams/planTasks";
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

type TeamBinding = { project: string; uuid: string; id: string; home: string };
type TeamFacts = { revision: number; state: string; actor?: string; phasePolicy?: string; defaultPhasePolicy?: string };

function teamBinding(project: string, task: Task): TeamBinding | undefined {
  return task.team && { project, uuid: task.uuid, ...task.team };
}
const bindingKey = (binding: TeamBinding) => JSON.stringify([binding.project, binding.uuid, binding.id, binding.home]);
const policyLabel = (policy: unknown) => policy === "lead" ? tr("Team lead") : policy === "person" ? tr("Person") : undefined;

const teamReadFailed = (binding: TeamBinding) => !!teams.readFailureFor(`team:${binding.id}`);
const minimumTeamRevision = (binding: TeamBinding) => teams.listed()
  .find((entry) => entry.id === binding.id && entry.home === binding.home && entry.project === binding.project)?.revision ?? 0;
const currentFacts = (binding: TeamBinding, facts: TeamFacts | undefined) => facts
  && !teamReadFailed(binding) && facts.revision >= minimumTeamRevision(binding) ? facts : undefined;

function nativeTeamFacts(binding: TeamBinding, view: TeamView | undefined): TeamFacts | undefined {
  if (view?.bar.id !== binding.id || view.bar.home !== binding.home || view.bar.project !== binding.project
    || !Number.isSafeInteger(view.revision) || view.revision < 0) return;
  const cards = view.groups.flatMap((group) => group.cards)
    .filter((card) => card.kind !== "stage" && card.task?.project === binding.project && card.task.uuid === binding.uuid);
  if (cards.length !== 1) return;
  const card = cards[0];
  const stages = card.stages.filter((stage) => ["in_progress", "reported"].includes(stage.state));
  const stage = stages.length === 1 ? stages[0] : undefined;
  const holder = stages.length ? stage?.holder : card.holder;
  const phase = card.phase && card.phase > 0
    ? [view.plan?.draft?.phase, ...(view.plan?.list?.rows ?? [])].find((phase) => phase?.n === card.phase)
    : undefined;
  const concretePolicy = policyLabel(phase?.acceptance);
  return {
    revision: view.revision,
    state: taskState(stage ? { ...card, state: stage.state, held: false } : card),
    actor: holder || undefined,
    phasePolicy: concretePolicy,
    defaultPhasePolicy: card.phase && card.phase > 0 && !concretePolicy
      ? policyLabel(view.settings?.phase_acceptance) : undefined,
  };
}

// Share only the selected task's facts with its compact row. No row has its own reader.
const [inspected, setInspected] = createSignal<{ owner: symbol; binding: string; facts: TeamFacts }>();

function teamFacts(project: string, task: Task): TeamFacts | undefined {
  const binding = teamBinding(project, task);
  if (!binding) return;
  const pane = inspected();
  return currentFacts(binding, nativeTeamFacts(binding, teams.currentTeam()))
    ?? currentFacts(binding, pane?.binding === bindingKey(binding) ? pane.facts : undefined);
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
  const owner = Symbol();
  const binding = createMemo(() => teamBinding(props.project, props.task));
  const request = createMemo(() => {
    const task = binding();
    if (!task || teams.available() !== true) return;
    const revision = minimumTeamRevision(task);
    return { task, revision, key: JSON.stringify([bindingKey(task), revision]) };
  }, undefined, { equals: (previous, next) => previous?.key === next?.key });
  const [snapshot, { mutate, refetch }] = createResource(request, async (pinned) => {
    const view = await teams.readTeamSnapshot(pinned.task.id, pinned.revision);
    return { key: pinned.key, facts: nativeTeamFacts(pinned.task, view) };
  });
  let mounted = true;
  createEffect(on(() => {
    const task = binding();
    return task ? teamReadFailed(task) : false;
  }, (failed) => {
    if (!failed) return;
    mutate(undefined);
    const pinned = request();
    queueMicrotask(() => {
      if (mounted && pinned && request()?.key === pinned.key && teamReadFailed(pinned.task)) void refetch();
    });
  }, { defer: true }));
  const facts = createMemo(() => {
    const pinned = request();
    if (!pinned) return;
    const live = currentFacts(pinned.task, nativeTeamFacts(pinned.task, teams.currentTeam()));
    const current = snapshot();
    return live ?? currentFacts(pinned.task, current?.key === pinned.key ? current.facts : undefined);
  });
  const clear = () => setInspected((current) => current?.owner === owner ? undefined : current);
  createEffect(() => {
    const known = facts(), task = binding();
    if (known && task) setInspected({ owner, binding: bindingKey(task), facts: known });
    else clear();
  });
  onCleanup(() => { mounted = false; clear(); });
  return <section class="tv-doc" aria-label={tr("Team review")}>
    <header><span class="eyebrow">{tr("Team review")}</span></header>
    <Show when={facts()}>{(facts) => <>
      <div>{tr("Team stage: %1", facts().state)}</div>
      <Show when={facts().actor}>{(holder) => <div>{tr("Current holder: %1", holder())}</div>}</Show>
      <Show when={facts().phasePolicy}>{(policy) => <div>{tr("Phase acceptance: %1", policy())}</div>}</Show>
      <Show when={facts().defaultPhasePolicy}>{(policy) => <div>{tr("Default phase acceptance: %1", policy())}</div>}</Show>
    </>}</Show>
    <Show when={props.task.team && teams.available() === true}>
      <button type="button" class="btn small ghost" onClick={() => {
        if (props.task.team && teams.available() === true) openTeamPage(props.task.team.id);
      }}>{tr("Open team page")}</button>
    </Show>
  </section>;
}
