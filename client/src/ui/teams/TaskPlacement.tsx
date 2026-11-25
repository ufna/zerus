import { createEffect, createMemo, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { tr } from "../../i18n";
import { TextArea, TextInput } from "../lib/TextField";
import { activeTeam, liveTask } from "./actions";
import { roleLabel } from "./TeamChips";
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
  const [draft, setDraft] = createSignal<{ kind: Placement; scope: string }>();
  const [member, setMember] = createSignal("");
  const [reason, setReason] = createSignal("");
  const [role, setRole] = createSignal("");
  const [pipeline, setPipeline] = createSignal("");
  /** Route's optional assignee ("" is nobody) and write paths, one per line (zerus-63rru.199 B3). */
  const [assignee, setAssignee] = createSignal("");
  const [writes, setWrites] = createSignal("");
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
  /** Who may take the task in the chosen role: its active, started members, never the lead. */
  const assignees = createMemo(() => props.view.members.filter((seat) => seat.role === role() && !seat.lead
    && seat.state === "active" && !!seat.session?.session_id));
  /** What an untouched form keeps: the assignment while the role stays, and the paths. */
  const keptAssignee = () => role() === props.task.role ? props.task.assignee || "" : "";
  const keptWrites = () => props.task.writes ?? [];
  const writeList = () => writes().split("\n").map((path) => path.trim()).filter(Boolean);
  const explicit = () => !props.task.fields_incomplete && (assignee() !== keptAssignee()
    || writeList().length !== keptWrites().length || writeList().some((path, i) => path !== keptWrites()[i]));
  /** The store's write-path rule, checked before anything is sent (core check_route). */
  const writesWhy = () => {
    const list = writeList();
    if (list.length > 200) return tr("A task names at most 200 write paths.");
    if (list.some((path) => path.length > 4096 || /^[\\/]|^[A-Za-z]:/.test(path))) return tr("Write paths are relative to the repository.");
    return undefined;
  };

  function blocked(kind: Placement) {
    const task = props.task;
    if (!activeTeam(props.view.bar.status)) return tr("This team cannot change task assignments.");
    if (!liveTask(task.state)) return tr("This task is already finished.");
    if (task.provider_policy_stopped) return tr("The provider blocked this attempt. Keep it stopped.");
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
    setAssignee(keptAssignee());
    setWrites(keptWrites().join("\n"));
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
    : pipelines().some((choice) => choice.id === pipeline()) && !(explicit() && writesWhy())
      && (!assignee() || assignees().some((seat) => seat.id === assignee()));

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const current = draft();
    if (!current || current.scope !== scope() || formBlocked(current) || !valid(current.kind)) return;
    const node = props.task.node;
    const fields = current.kind === "reassign"
      ? { node, member: member(), reason: reason() }
      : explicit()
        // The whole placement: the task's own dependencies, the edited paths
        // and the chosen assignee replace what the service would keep.
        ? { node, role: role(), pipeline: pipeline(), keep_task_fields: false, deps: props.task.deps ?? [], writes: writeList(), assignee: assignee() || null }
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
      <Show when={props.task.kind !== "stage" && props.task.phase}>{(phase) =>
        <button type="button" class="btn small" data-placement-open-phase onClick={() => openPhase(props.view.bar.id, phase())}>{tr("Open phase %1", phase())}</button>
      }</Show>
    </div>
    {/* A refreshed view rebuilds the option lists; each option carries its own
        selected state so the form keeps showing the choice it will send. */}
    <Show when={draft()} keyed>{(current) =>
      <form class="team-reason" data-placement-form={current.kind} onSubmit={submit}>
        <Show when={current.kind === "reassign"} fallback={<>
          <label class="team-block">
            <span class="team-block">{tr("Role")}</span>
            <select class="field" value={role()} onChange={(event) => { setRole(event.currentTarget.value); setPipeline(""); setAssignee(keptAssignee()); }}>
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
          <label class="team-block">
            <span class="team-block">{tr("Assign to")}</span>
            <select class="field" data-route-assignee value={assignee()} disabled={!role() || !!props.task.fields_incomplete} onChange={(event) => setAssignee(event.currentTarget.value)}>
              <option value="" selected={!assignee()}>{tr("Nobody")}</option>
              <For each={assignees()}>{(seat) => <option value={seat.id} selected={seat.id === assignee()}>{seat.id}</option>}</For>
            </select>
          </label>
          <label class="team-block">
            <span class="team-block">{tr("Write paths, one per line")}</span>
            <TextArea class="field" data-route-writes rows={3} value={writes()} maxLength={200 * 4097} disabled={!!props.task.fields_incomplete}
              onInput={(event) => setWrites(event.currentTarget.value)} />
          </label>
          <Show when={props.task.fields_incomplete}>
            <span class="team-action-why dim">{tr("This task's dependencies or write paths could not all be read, so Route keeps its assignment and write paths.")}</span>
          </Show>
          <Show when={explicit() ? writesWhy() : undefined}>{(why) => <span class="team-action-why dim" role="status">{why()}</span>}</Show>
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
