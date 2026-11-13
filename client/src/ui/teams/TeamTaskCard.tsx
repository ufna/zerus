import { For, Show } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { RoleChip, StageChip, TeamAgent, stageIcon } from "./TeamChips";
import { cardWorker, currentStage, orderedStages, orphanStage, workerOf } from "./pipeline-view";
import type { MemberView, TaskView } from "./types";

/** The stop outcome's words from its stable code (zerus-63rru.52 round 2):
 * the code maps through the catalogs and the store's refusal reason shows
 * as secondary text. */
export function stopOutcomeText(outcome: { code: string; reason?: string } | null | undefined): string {
  if (!outcome) return "";
  const reason = outcome.reason?.trim();
  switch (outcome.code) {
    case "cancelled": return tr("The team stop cancelled this task");
    case "released": return tr("The team stop released this task");
    case "cancel_refused": return reason ? tr("The team stop's cancel was refused: %1", reason) : tr("The team stop's cancel was refused");
    case "key_cleared": return tr("The team stop cleared this task's key");
    case "key_clear_refused": return reason ? tr("The team stop's key clear was refused: %1", reason) : tr("The team stop's key clear was refused");
    case "pending": return tr("The team stop has not reached this task yet");
    case "unchanged": return tr("The team stop left this finished task as it was");
    default: return tr("The team stop outcome %1", outcome.code);
  }
}

export function TeamTaskCard(props: { task: TaskView; members: readonly MemberView[]; selected: boolean; onSelect: () => void }) {
  const stage = () => currentStage(props.task);
  const worker = () => cardWorker(props.task, props.members);
  const decision = () => props.task.state === "reported" && !stage();
  const stages = () => props.task.kind === "stage" ? [orphanStage(props.task)] : orderedStages(props.task);
  const icon = () => props.task.state === "accepted" ? "circle-check" : stage() && ["in_progress", "reported"].includes(stage()!.state) ? stageIcon(stage()!) : props.task.state === "in_progress" ? "circle-play" : props.task.state === "reported" ? "circle-arrow-right" : "circle";
  const segments = () => [
    ...(props.task.kind === "stage" ? [] : [{ node: props.task.node, kind: null, state: ["reported", "accepted"].includes(props.task.state) ? "accepted" : props.task.state }]),
    ...stages(),
  ];
  return <button type="button" class="team-task-card" classList={{ "team-task-selected": props.selected, "needs-decision": decision(), "is-working": props.task.state === "in_progress" || stage()?.state === "in_progress", "is-held": props.task.held }}
    data-node={props.task.node} data-state={props.task.state} onClick={props.onSelect} aria-pressed={props.selected} aria-label={props.task.title || tr("Task %1", props.task.node)}>
    <span class="team-task-heading"><Icon name={icon()} size={12} /><span class="team-task-ref" title={props.task.task?.uuid}>#{props.task.node}</span>

      <span class="team-task-title" title={props.task.title}>{props.task.title || tr("Task %1", props.task.node)}</span>
    </span>
    <span class="team-task-meta">
      <Show when={props.task.role}><RoleChip role={props.task.role} /></Show>
      <Show when={worker()}>{(member) => <TeamAgent agent={member().agent} />}</Show>
      <Show when={workerOf(props.task) && workerOf(props.task) !== props.task.role}><span class="team-task-worker">{workerOf(props.task)}</span></Show>
      <Show when={props.task.kind === "stage" && props.task.implementer}><span class="team-task-worker dim" data-checks>{tr("checks the work of %1", props.task.implementer)}</span></Show>
      <Show when={props.task.state === "accepted" && props.task.decided_by} fallback={<Show when={props.task.round > 0}><span class="team-task-round">{tr("Attempt %1", props.task.round)}</span></Show>}><span class="team-task-decision">{tr("Accepted by %1", props.task.decided_by)}</span></Show>
    </span>
    <span class="team-task-stages">
      <For each={stages()}>{(stage) => <StageChip stage={stage} rounds={props.task.rounds} on={props.task} />}</For>
      <Show when={decision()}><span class="team-stage-chip team-decision-chip"><Icon name="crown" size={11} />{tr("Decision needed")}</span></Show>
      <Show when={props.task.held}><span class="team-stage-chip team-held-chip"><Icon name="lock" size={11} />{tr("held")}</span></Show>
      <Show when={props.task.stop_outcome}><span class="team-stage-chip team-stop-chip" role="note">{stopOutcomeText(props.task.stop_outcome)}</span></Show>
      <Show when={props.task.integrated}><span class="team-stage-chip"><Icon name="git-merge" size={11} />{tr("merged")}</span></Show>
      <span class="team-card-progress" role="img" aria-label={tr("Pipeline progress")}><For each={segments()}>{(segment) => <i data-state={segment.state} data-kind={segment.kind} />}</For></span>
    </span>
  </button>;
}
