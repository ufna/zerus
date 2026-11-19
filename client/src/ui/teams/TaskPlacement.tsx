import { createEffect, createMemo, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { tr } from "../../i18n";
import { TextInput } from "../lib/TextField";
import { activeTeam, liveTask } from "./actions";
import { roleLabel } from "./TeamChips";
import type { EditAction } from "./TeamEditors";
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
  const [draft, setDraft] = createSignal<{ kind: Placement; scope: string }>();
  const [member, setMember] = createSignal("");
  const [reason, setReason] = createSignal("");
  const [role, setRole] = createSignal("");
  const [pipeline, setPipeline] = createSignal("");
  /** The team revision the person reviewed the open form against (review of #2548). */
  const [reviewed, setReviewed] = createSignal(0);
  // The form belongs to one task of one team: a refreshed card of the same task
  // keeps it open (zerus-3u5yu 7), only another task or team closes it.
  const scope = createMemo(() => `${props.view.bar.id}:${props.task.node}`);
  createEffect(on(scope, () => setDraft(undefined), { defer: true }));
  createEffect(on(() => props.otherFormOpen, (open) => { if (open) setDraft(undefined); }, { defer: true }));

  const members = createMemo(() => {
    const task = props.task;
    const current = task.assignee || (task.holder === undefined ? task.implementer : task.holder);
    return props.view.members.filter((seat) => seat.role === task.role && !seat.lead
      && seat.state === "active" && !!seat.session?.session_id && seat.id !== current
      // A stage is never handed to a member who implemented any round of its
      // target; the board names them (zerus-3u5yu 5).
      && (task.kind !== "stage" || (seat.id !== task.implementer && !(task.earlier_implementers ?? []).includes(seat.id))));
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
      if (task.phase) return tr("Changing an approved task's route requires a plan edit.");
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
    setReviewed(props.view.revision);
    setDraft({ kind, scope: scope() });
  };
  // The open form stays open across other team writes and keeps its choices
  // (zerus-3u5yu 7), but it sends only the revision the person reviewed. A newer
  // revision marks it stale until Review again binds the current one, so a
  // draft is never sent against facts the person has not seen (review of #2548).
  const stale = () => reviewed() !== props.view.revision;
  const formBlocked = (current: NonNullable<ReturnType<typeof draft>>) => blocked(current.kind)
    ?? (stale() ? tr("The team changed after you opened this form. Review the choices, then use Review again.") : undefined);
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
    props.onAction(current.kind, fields, reviewed(), props.view.bar.id);
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
    </div>
    {/* A refreshed view rebuilds the option lists; each option carries its own
        selected state so the form keeps showing the choice it will send. */}
    <Show when={draft()} keyed>{(current) =>
      <form class="team-reason" data-placement-form={current.kind} onSubmit={submit}>
        <Show when={current.kind === "reassign"} fallback={<>
          <label class="team-block">
            <span class="team-block">{tr("Role")}</span>
            <select class="field" value={role()} onChange={(event) => { setRole(event.currentTarget.value); setPipeline(""); }}>
              <option value="">{tr("Choose a role")}</option>
              <For each={roles()}>{(id) => <option value={id} selected={id === role()}>{roleLabel(id) !== id ? roleLabel(id) : props.view.member_roles?.find((item) => item.id === id)?.name || id}</option>}</For>
            </select>
          </label>
          <label class="team-block">
            <span class="team-block">{tr("Pipeline")}</span>
            <select class="field" value={pipeline()} disabled={!role()} onChange={(event) => setPipeline(event.currentTarget.value)}>
              <option value="">{tr("Choose a pipeline")}</option>
              <For each={pipelines()}>{(choice) => <option value={choice.id} selected={choice.id === pipeline()}>{choice.name || choice.id}</option>}</For>
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
              <For each={members()}>{(seat) => <option value={seat.id} selected={seat.id === member()}>{seat.id}</option>}</For>
            </select>
          </label>
          <TextInput class="field" aria-label={tr("Reason")} historyKey={`team-reassign-${props.task.node}`}
            placeholder={tr("Why are you reassigning this task?")} maxlength={16384}
            value={reason()} onInput={(event) => setReason(event.currentTarget.value)} />
        </Show>
        <Show when={formBlocked(current)}>{(message) => <span class="team-action-why dim" role="status">{message()}</span>}</Show>
        <Show when={!blocked(current.kind) && stale()}>
          <button type="button" class="btn small" data-placement-review-again onClick={() => setReviewed(props.view.revision)}>{tr("Review again")}</button>
        </Show>
        <button type="submit" class="btn small primary" disabled={!!formBlocked(current) || !valid(current.kind)}>{tr("Review change")}</button>
        <button type="button" class="btn small" onClick={() => setDraft(undefined)}>{tr("Cancel")}</button>
      </form>
    }</Show>
  </>;
}
