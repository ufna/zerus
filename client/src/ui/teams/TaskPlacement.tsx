import { createEffect, createMemo, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { tr } from "../../i18n";
import { TextInput } from "../lib/TextField";
import { activeTeam, liveTask } from "./actions";
import type { EditAction } from "./TeamEditors";
import { openPhase } from "../../store/team-phases";
import type { TaskView, TeamView } from "./types";

type Placement = "reassign" | "route";

export function TaskPlacement(props: {
  view: TeamView;
  task: TaskView;
  busy: boolean;
  unresolved: boolean;
  otherFormOpen: boolean;
  onOpen: () => void;
  onAction: EditAction;
}) {
  const [draft, setDraft] = createSignal<{ kind: Placement; revision: number; scope: string }>();
  const [member, setMember] = createSignal("");
  const [reason, setReason] = createSignal("");
  const [role, setRole] = createSignal("");
  const [pipeline, setPipeline] = createSignal("");
  const scope = () => `${props.view.bar.id}:${props.task.node}`;
  createEffect(on(scope, () => setDraft(undefined), { defer: true }));
  createEffect(on(() => props.otherFormOpen, (open) => { if (open) setDraft(undefined); }, { defer: true }));

  const members = createMemo(() => {
    const task = props.task;
    const current = task.assignee || (task.holder === undefined ? task.implementer : task.holder);
    return props.view.members.filter((seat) => seat.role === task.role && !seat.lead
      && seat.state === "active" && !!seat.session?.session_id && seat.id !== current
      && (task.kind !== "stage" || seat.id !== task.implementer));
  });
  const routes = () => props.view.routing_pipelines ?? [];
  const roles = createMemo(() => [...new Set(routes().map((choice) => choice.role))]);
  const pipelines = createMemo(() => routes().filter((choice) => choice.role === role()));

  function blocked(kind: Placement) {
    const task = props.task;
    if (!activeTeam(props.view.bar.status)) return tr("This team cannot change task assignments.");
    if (!liveTask(task.state)) return tr("This task is already finished.");
    if (props.unresolved) return tr("Check or dismiss the unresolved task action first.");
    if (task.pending_decision && (kind === "route" || !task.pending_replaceable)) {
      return tr("The task's previous decision has not settled yet.");
    }
    if (kind === "reassign") {
      if (!task.role) return tr("Route this task before reassigning it.");
      if (!members().length) return tr("No other active member of this role has a session.");
    } else {
      if (task.kind === "stage") return tr("A stage's role comes from its pipeline. Reassign the stage to change its member.");
      // No logged plan edit exists yet; an approved phase changes its tasks
      // through a send-back with corrections, which its phase offers once every
      // task is decided (ops rework.rs send_back, zerus-63rru.197 A2).
      if (task.phase) return tr("To change an approved phase's tasks, send the phase back with corrections once all its tasks are decided.");
      if (task.state !== "waiting" && task.state !== "pending") return tr("Only waiting or pending tasks can be routed.");
      if (!routes().length) return tr("This team's routing choices are unavailable.");
    }
    return props.busy ? tr("A team action is in progress.") : undefined;
  }

  const reasons = createMemo(() => [...new Set([blocked("reassign"), blocked("route")].filter((reason) => !!reason))]);
  const reasonIds = createUniqueId();
  const label = (kind: Placement) => kind === "reassign" ? tr("Reassign…") : tr("Route…");
  const open = (kind: Placement) => {
    if (blocked(kind)) return;
    props.onOpen();
    setMember("");
    setReason("");
    setRole(roles().includes(props.task.role) ? props.task.role : "");
    setPipeline(pipelines().some((choice) => choice.id === props.task.pipeline) ? props.task.pipeline : "");
    setDraft({ kind, revision: props.view.revision, scope: scope() });
  };
  const formBlocked = (current: NonNullable<ReturnType<typeof draft>>) =>
    current.revision !== props.view.revision ? tr("The team changed. Close this form and open it again.") : blocked(current.kind);
  const valid = (kind: Placement) => kind === "reassign"
    ? !!reason().trim() && members().some((seat) => seat.id === member())
    : pipelines().some((choice) => choice.id === pipeline());

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const current = draft();
    if (!current || current.scope !== scope() || formBlocked(current) || !valid(current.kind)) return;
    const node = props.task.node;
    const fields = current.kind === "reassign"
      ? { node, member: member(), reason: reason() }
      : { node, role: role(), pipeline: pipeline(), keep_task_fields: true };
    setDraft(undefined);
    props.onAction(current.kind, fields, current.revision, props.view.bar.id);
  };

  return <>
    <div class="team-finding-actions" data-task-placement>
      <For each={["reassign", "route"] as const}>{(kind) => {
        const refusal = () => blocked(kind);
        return <button type="button" class="btn small" data-task-action={kind}
          aria-expanded={draft()?.kind === kind} disabled={!!refusal()} title={refusal()}
          aria-describedby={refusal() ? `${reasonIds}-${reasons().indexOf(refusal()!)}` : undefined}
          onClick={() => open(kind)}>{label(kind)}</button>;
      }}</For>
      <For each={reasons()}>{(line, i) => <span id={`${reasonIds}-${i()}`} class="team-action-why dim">{line}</span>}</For>
      <Show when={!blocked("reassign") && props.task.pending_replaceable}>
        <span class="team-action-why dim">{tr("Reassign replaces the pending decision.")}</span>
      </Show>
      <Show when={props.task.kind !== "stage" && props.task.phase}>{(phase) =>
        <button type="button" class="btn small" data-placement-open-phase onClick={() => openPhase(props.view.bar.id, phase())}>{tr("Open phase %1", phase())}</button>
      }</Show>
    </div>
    <Show when={draft()} keyed>{(current) =>
      <form class="team-reason" data-placement-form={current.kind} onSubmit={submit}>
        <Show when={current.kind === "reassign"} fallback={<>
          <label class="team-block">
            <span class="team-block">{tr("Role")}</span>
            <select class="field" value={role()} onChange={(event) => { setRole(event.currentTarget.value); setPipeline(""); }}>
              <option value="">{tr("Choose a role")}</option>
              <For each={roles()}>{(id) => <option value={id}>{props.view.member_roles?.find((item) => item.id === id)?.name || id}</option>}</For>
            </select>
          </label>
          <label class="team-block">
            <span class="team-block">{tr("Pipeline")}</span>
            <select class="field" value={pipeline()} disabled={!role()} onChange={(event) => setPipeline(event.currentTarget.value)}>
              <option value="">{tr("Choose a pipeline")}</option>
              <For each={pipelines()}>{(choice) => <option value={choice.id}>{choice.name || choice.id}</option>}</For>
            </select>
          </label>
          <Show when={props.task.assignee && role() && role() !== props.task.role}>
            <span class="team-action-why dim">{tr("Changing the role clears the current assignment.")}</span>
          </Show>
        </>}>
          <label class="team-block">
            <span class="team-block">{tr("Replacement member")}</span>
            <select class="field" value={member()} onChange={(event) => setMember(event.currentTarget.value)}>
              <option value="">{tr("Choose a member")}</option>
              <For each={members()}>{(seat) => <option value={seat.id}>{seat.id}</option>}</For>
            </select>
          </label>
          <TextInput class="field" aria-label={tr("Reason")} historyKey={`team-reassign-${props.task.node}`}
            placeholder={tr("Why are you reassigning this task?")} maxlength={16384}
            value={reason()} onInput={(event) => setReason(event.currentTarget.value)} />
        </Show>
        <Show when={formBlocked(current)}>{(message) => <span class="team-action-why dim" role="status">{message()}</span>}</Show>
        <button type="submit" class="btn small primary" disabled={!!formBlocked(current) || !valid(current.kind)}>{tr("Review change")}</button>
        <button type="button" class="btn small" onClick={() => setDraft(undefined)}>{tr("Cancel")}</button>
      </form>
    }</Show>
  </>;
}
