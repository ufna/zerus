import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { tr } from "../../i18n";
import { detailTab, page, selected, setPage } from "../../store/ui";
import { pageHidden } from "../../store/visibility";
import { newRequestId, teams, type TeamCall, type TeamView } from "../../store/teams";
import { teamCall, type TaskButton } from "./actions";
import { MemberCard } from "./MemberCard";
import { TeamBar } from "./team-bar";
import { TaskActions } from "./TeamActions";
import { TeamConfirmDialog } from "./TeamConfirmDialog";
import { stageLine } from "./task-line";
import { TextInput } from "../lib/TextField";
import type { MemberView, TaskView } from "./types";
import "../styles/teams.css";

// The Team tab (the fork's TeamWorkspace, adapted to M1): the team bar, the
// member column and the Board with the queue as its own group, each card with
// its stages, a card's detail and decisions, Add task and the next page. The
// graph, the phase header, the editor, the feed, proposals and the profile are
// not part of M1 and are not here. Data comes only from the store's followed
// team view; every action is one intent through the store.

export const GROUPS = ["waiting", "pending", "in_progress", "reported", "accepted", "cancelled"] as const;
const LABELS: Record<string, string> = {
  draft: "Draft", waiting: "Waiting", pending: "To do", in_progress: "In progress",
  reported: "In review", accepted: "Done", cancelled: "Cancelled",
};

/** The workspace, from a view the store has already projected. */
export function TeamWorkspace(props: {
  view: TeamView;
  busy: boolean;
  failure: { code: string; message: string } | null;
  onAction: (kind: string, fields?: Record<string, unknown>) => void;
  onPage: (cursor: string) => void;
  onAddTask: () => void;
}) {
  const [selectedNode, setSelectedNode] = createSignal<number | null>(null);
  const [asking, setAsking] = createSignal<TaskButton | null>(null);
  const [reason, setReason] = createSignal("");
  const groups = createMemo(() => {
    const byState = new Map(props.view.groups.map((group) => [group.state, group.cards]));
    return GROUPS.map((state) => ({ state, cards: byState.get(state) ?? [] }));
  });
  const all = createMemo(() => props.view.groups.flatMap((group) => group.cards));
  const card = createMemo(() => all().find((candidate) => candidate.node === selectedNode()) ?? null);
  const send = (kind: string, fields: Record<string, unknown> = {}) => { props.onAction(kind, fields); setAsking(null); setReason(""); };
  return (
    <div class="team-tab" data-team-tab>
      <TeamBar view={props.view} busy={props.busy} onAction={(kind) => send(kind)} />
      <Show when={props.failure}>
        {(failure) => (
          <div class="team-notice" role="alert" data-team-failure>
            {failure().code === "cursor_reset" ? failure().message : tr("Teams read failed: %1", failure().message)}
          </div>
        )}
      </Show>
      <div class="team-body">
        <aside class="team-members" data-team-members aria-label={tr("Members")}>
          <For each={props.view.members}>
            {(member: MemberView) => <MemberCard member={member} busy={props.busy}
              onAction={(kind, id) => send(kind, { member: id })} />}
          </For>
        </aside>
        <section class="team-main">
          <div class="team-canvas">
            <div class="team-board" data-team-board>
              <For each={groups()}>
                {(group) => (
                  <div class="team-column" data-group={group.state}>
                    <h3><span>{tr(LABELS[group.state] ?? group.state)}</span><span>{group.cards.length}</span></h3>
                    <For each={group.cards}>
                      {(task: TaskView) => (
                        <button type="button" class="team-task-card" classList={{ "team-task-selected": selectedNode() === task.node }}
                          data-node={task.node} onClick={() => setSelectedNode(task.node)} aria-label={task.title || tr("Task %1", task.node)}>
                          <span class="team-task-title ell">{task.title || tr("Task %1", task.node)}</span>
                          <span class="team-task-meta dim">
                            <span class="mono">{task.rank}</span>
                            <Show when={task.role}><span>{tr(task.role)}</span></Show>
                            <Show when={task.assignee}><span>{tr("by %1", task.assignee)}</span></Show>
                            <Show when={task.held}><span>{tr("held")}</span></Show>
                            <Show when={task.integrated}><span>{tr("merged")}</span></Show>
                          </span>
                          <Show when={task.stages.length}>
                            <span class="team-task-stages">
                              <For each={task.stages}>{(stage) => <span class="team-task-stage" data-stage={stage.node}><b>{tr(stage.state)}</b> {stageLine(stage)}</span>}</For>
                            </span>
                          </Show>
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </div>
            <Show when={card()}>
              {(shown) => (
                <div class="team-task-detail" data-team-detail>
                  <h3>{shown().title || tr("Task %1", shown().node)}</h3>
                  <dl class="team-facts">
                    <dt>{tr("Task")}</dt><dd>
                      <Show when={shown().task} fallback={tr("not sent yet")}>
                        {(task) => <><span class="team-block">{task().project}</span><span class="team-block mono">{task().uuid}</span></>}
                      </Show>
                    </dd>
                    <dt>{tr("Role")}</dt><dd>{tr(shown().role || "unknown")}</dd>
                    <dt>{tr("Pipeline")}</dt><dd>{shown().pipeline || tr("none")}</dd>
                    <dt>{tr("Assigned to")}</dt><dd>{shown().assignee || tr("free")}</dd>
                    <dt>{tr("Implemented by")}</dt><dd>{shown().implementer || tr("nobody yet")}</dd>
                    <dt>{tr("Round")}</dt><dd>{shown().round}</dd>
                    <Show when={shown().decided_by}><dt>{tr("Decided by")}</dt><dd>{tr(shown().decided_by)}</dd></Show>
                  </dl>
                  <Show when={shown().stages.length}>
                    <dl class="team-facts">
                      <For each={shown().stages}>{(stage) => (
                        <>
                          <dt>{tr("Stage %1", stage.index + 1)}</dt>
                          <dd><span class="team-block">{tr(stage.state)}</span><span class="team-block">{stageLine(stage)}</span></dd>
                        </>
                      )}</For>
                    </dl>
                  </Show>
                  <TaskActions card={shown()} busy={props.busy} onAction={(kind) => send(kind, { node: shown().node })}
                    onReason={(button) => { setAsking(button); setReason(""); }} />
                  <Show when={asking()}>
                    {(button) => (
                      <div class="team-reason">
                        <TextInput class="input" aria-label={tr("Reason")} historyKey={`team-reason-${shown().node}`}
                          placeholder={button().id === "return" ? tr("What must change?") : tr("Why is it cancelled?")}
                          value={reason()} onInput={(event) => setReason(event.currentTarget.value)} />
                        <button type="button" class="btn small primary" disabled={!reason().trim()}
                          onClick={() => send(button().id, { node: shown().node, reason: reason() })}>{tr("Send")}</button>
                        <button type="button" class="btn small" onClick={() => setAsking(null)}>{tr("Cancel")}</button>
                      </div>
                    )}
                  </Show>
                </div>
              )}
            </Show>
          </div>
          <Show when={!all().length}><div class="team-empty">{tr("The queue is empty. Send a task to the team from the task board.")}</div></Show>
          <div class="team-toolbar">
            <Show when={props.view.truncated}><span class="team-note">{tr("Some cards are not shown.")}</span></Show>
            <span class="grow" />
            <Show when={props.view.next}>
              {(cursor) => <button type="button" class="btn small" onClick={() => props.onPage(cursor())}>{tr("More…")}</button>}
            </Show>
            <button type="button" class="btn small" onClick={props.onAddTask}>{tr("Add task")}</button>
          </div>
        </section>
      </div>
    </div>
  );
}

/** The two exits from an uncertain action; nothing repeats it by itself. */
export function TeamUncertain(props: { detail: string; busy: boolean; onCheck: () => void; onDismiss: () => void }) {
  return (
    <div class="team-uncertain" role="alert" data-team-uncertain>
      <span>{props.detail}</span>
      <span class="team-finding-actions">
        <button type="button" class="btn small primary" disabled={props.busy} onClick={props.onCheck}>{tr("Check")}</button>
        <button type="button" class="btn small" disabled={props.busy} onClick={props.onDismiss}>{tr("Dismiss")}</button>
      </span>
    </div>
  );
}

/** The session's team: resolved from the list's lead session, then followed. */
export function TeamTab() {
  const row = selected();
  onMount(() => {
    void teams.probe();
    void teams.leadTeamFor(row?.key ?? "", { sessionId: row?.sessionId ?? null, runId: row?.runId ?? null });
  });
  createEffect(() => {
    const hidden = pageHidden() || page() !== "sessions" || detailTab() !== "team";
    void teams.hidden(hidden);
  });
  onCleanup(() => { void teams.closeTeam(); });
  const teamId = createMemo(() => (row ? teams.sessionTeams()[row.key] : undefined));
  const view = createMemo(() => {
    const shown = teams.team();
    return shown && teamId() && shown.bar.id === teamId() ? shown : undefined;
  });
  const act = (kind: string, fields: Record<string, unknown> = {}) => {
    const current = view();
    if (!current) return;
    const call: TeamCall = teamCall(current, newRequestId(), kind, fields);
    void teams.run(call);
  };
  return (
    <>
      <Show when={view()} fallback={
        <div class="team-loading" role="status" data-team-waiting>
          <Show when={teamId() === undefined} fallback={<span class="dim">{tr("This session is not a team lead.")}</span>}>
            <span class="dim">{tr("Finding the team of this session…")}</span>
          </Show>
        </div>
      }>
        {(current) => (
          <>
            <TeamWorkspace view={current()} busy={teams.busy()} failure={teams.failure()}
              onAction={act} onPage={(cursor) => void teams.page(`team:${current().bar.id}`, cursor)}
              onAddTask={() => setPage("projects")} />
            <Show when={teams.uncertain()} keyed>
              {(asking) => <TeamUncertain detail={asking.detail} busy={teams.busy()} onCheck={() => void teams.check()} onDismiss={() => void teams.dismiss()} />}
            </Show>
          </>
        )}
      </Show>
      <TeamConfirmDialog />
    </>
  );
}
