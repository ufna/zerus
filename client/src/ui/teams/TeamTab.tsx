import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { tr } from "../../i18n";
import { detailTab, page, selected, setPage } from "../../store/ui";
import { pageHidden } from "../../store/visibility";
import { newRequestId, teams, type TeamCall, type TeamCard, type TeamStage, type TeamView } from "../../store/teams";
import { TeamConfirmDialog } from "./TeamConfirmDialog";
import { TextInput } from "../lib/TextField";
import "./teams.css";

// The Team tab on the lead's session (M1): the team bar, the member cards and
// the work as a Board with the queue as its own group, each card with its
// stages. The data comes from the followed `team:<id>` view only; the tab
// never reads or polls on its own, it asks the store to follow and to act.

/** The board's groups, in their order. */
export const GROUPS = ["waiting", "pending", "in_progress", "reported", "accepted", "cancelled"] as const;
const LABELS: Record<string, string> = {
  draft: "Draft", waiting: "Waiting", pending: "To do", in_progress: "In progress",
  reported: "In review", accepted: "Done", cancelled: "Cancelled",
};

/** The stage's worker (`assignee`) and the implementer of the task it checks are separate fields. */
export function stageLine(stage: TeamStage): string {
  return tr("%1: %2, checks the work of %3", tr(stage.role), stage.assignee || tr("unassigned"), stage.implementer || tr("nobody"));
}

export function TeamBar(props: { view: TeamView }) {
  const counts = props.view.bar.counts;
  const chip = (value: number, label: string) => (
    <span class="tm-count"><b>{value}</b>{tr(label)}</span>
  );
  return (
    <div class="tm-bar" data-team-bar>
      <h2>{props.view.bar.name}</h2>
      <span class="tm-status">{tr(props.view.bar.status)}</span>
      <span class="tm-goal" title={props.view.bar.goal}>{props.view.bar.goal || props.view.bar.project}</span>
      <div class="tm-counts">
        {chip(counts.members.active, "active")}
        {chip(counts.nodes.waiting, "waiting")}
        {chip(counts.nodes.pending, "to do")}
        {chip(counts.nodes.in_progress, "in progress")}
        {chip(counts.nodes.reported, "in review")}
        {chip(counts.nodes.accepted, "done")}
        {chip(counts.inbox, "inbox")}
        {chip(counts.cards, "cards")}
      </div>
    </div>
  );
}

export function TeamMembers(props: { view: TeamView }) {
  return (
    <div class="tm-members" data-team-members>
      <For each={props.view.members}>
        {(member) => (
          <div class="tm-member" classList={{ "tm-lead": member.lead }}>
            <div class="tm-id">{member.id}{member.lead ? ` (${tr("lead")})` : ""}</div>
            <div class="tm-role">{tr(member.role)}</div>
            <div class="tm-role">{member.agent}</div>
            <div class="tm-state">{tr(member.state)}<Show when={member.session}> ({tr("session bound")})</Show></div>
          </div>
        )}
      </For>
    </div>
  );
}

/** One card: its stages, and the lead's decisions once selected. */
export function TeamCardView(props: {
  card: TeamCard;
  selected: boolean;
  busy: boolean;
  onSelect: () => void;
  onAction: (kind: string, fields?: Record<string, unknown>) => void;
}) {
  const [asking, setAsking] = createSignal<string | null>(null);
  const [reason, setReason] = createSignal("");
  const send = (kind: string, fields: Record<string, unknown> = {}) => {
    props.onAction(kind, fields);
    setAsking(null);
    setReason("");
  };
  const ask = (kind: string) => { setAsking(kind); setReason(""); };
  return (
    <div class="tm-card" classList={{ "tm-selected": props.selected }} role="button" tabindex={0}
      aria-label={props.card.title || tr("Task %1", props.card.node)}
      onClick={props.onSelect}
      onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); props.onSelect(); } }}>
      <span class="tm-title">{props.card.title || tr("Task %1", props.card.node)}</span>
      <span class="tm-meta">
        <span class="tm-rank">{props.card.rank}</span>
        <Show when={props.card.role}><span>{tr(props.card.role)}</span></Show>
        <Show when={props.card.pipeline}><span>{props.card.pipeline}</span></Show>
        <Show when={props.card.assignee}><span>{tr("by %1", props.card.assignee)}</span></Show>
        <Show when={props.card.held}><span>{tr("held")}</span></Show>
        <Show when={props.card.integrated}><span>{tr("merged")}</span></Show>
      </span>
      <Show when={props.card.stages.length}>
        <ul class="tm-stages">
          <For each={props.card.stages}>{(stage) => (
            <li class="tm-stage" data-stage={stage.node}>
              <b>{tr(stage.state)}</b>
              <span>{stageLine(stage)}</span>
            </li>
          )}</For>
        </ul>
      </Show>
      <Show when={props.selected}>
        <div class="tm-card-actions" onClick={(event) => event.stopPropagation()}>
          <Show when={props.card.state === "waiting"}>
            <button type="button" class="btn small" disabled={props.busy} onClick={() => send("admit", { node: props.card.node })}>{tr("Admit")}</button>
          </Show>
          <Show when={props.card.state === "reported"}>
            <button type="button" class="btn small primary" disabled={props.busy} onClick={() => send("accept", { node: props.card.node })}>{tr("Accept")}</button>
          </Show>
          <Show when={props.card.state === "reported" || props.card.state === "in_progress"}>
            <button type="button" class="btn small" disabled={props.busy} onClick={() => ask("return")}>{tr("Return")}</button>
          </Show>
          <Show when={props.card.state === "pending" || props.card.state === "in_progress"}>
            <button type="button" class="btn small" disabled={props.busy}
              onClick={() => send(props.card.held ? "release" : "hold", { node: props.card.node })}>{props.card.held ? tr("Release") : tr("Hold")}</button>
          </Show>
          <Show when={props.card.state !== "accepted" && props.card.state !== "cancelled"}>
            <button type="button" class="btn small danger" disabled={props.busy} onClick={() => ask("cancel")}>{tr("Cancel task")}</button>
          </Show>
        </div>
        <Show when={asking()}>
          {(kind) => (
            <div class="tm-reason" onClick={(event) => event.stopPropagation()}>
              <TextInput class="input" aria-label={tr("Reason")} historyKey={`team-reason-${props.card.node}`}
                placeholder={kind() === "return" ? tr("What must change?") : tr("Why is it cancelled?")}
                value={reason()} onInput={(event) => setReason(event.currentTarget.value)} />
              <button type="button" class="btn small primary" disabled={!reason().trim()}
                onClick={() => send(kind(), { node: props.card.node, reason: reason() })}>{tr("Send")}</button>
              <button type="button" class="btn small" onClick={() => setAsking(null)}>{tr("Cancel")}</button>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}

/** The card's details: its task, role, pipeline and stages. */
function TeamDetail(props: { card: TeamCard }) {
  return (
    <div class="tm-detail" data-team-detail>
      <h3>{props.card.title || tr("Task %1", props.card.node)}</h3>
      <dl class="tm-pair">
        <dt>{tr("Task")}</dt><dd><Show when={props.card.task} fallback={tr("not sent yet")}>{(task) => <><span class="tm-block">{task().project}</span><span class="tm-block">{task().uuid}</span></>}</Show></dd>
        <dt>{tr("Role")}</dt><dd>{tr(props.card.role || "unknown")}</dd>
        <dt>{tr("Pipeline")}</dt><dd>{props.card.pipeline || tr("none")}</dd>
        <dt>{tr("Assigned to")}</dt><dd>{props.card.assignee || tr("free")}</dd>
        <dt>{tr("Implemented by")}</dt><dd>{props.card.implementer || tr("nobody yet")}</dd>
        <dt>{tr("Round")}</dt><dd>{props.card.round}</dd>
        <Show when={props.card.decided_by}><dt>{tr("Decided by")}</dt><dd>{tr(props.card.decided_by)}</dd></Show>
      </dl>
      <Show when={props.card.stages.length}>
        <dl class="tm-pair">
          <For each={props.card.stages}>{(stage) => (
            <>
              <dt>{tr("Stage %1", stage.index + 1)}</dt>
              <dd><span class="tm-block">{tr(stage.state)}</span><span class="tm-block">{stageLine(stage)}</span></dd>
            </>
          )}</For>
        </dl>
      </Show>
    </div>
  );
}

/** The board: the queue as its own group, then the states that carry cards. */
export function TeamBoard(props: {
  view: TeamView;
  busy: boolean;
  selected: number | null;
  onSelect: (node: number) => void;
  onAction: (card: TeamCard, kind: string, fields?: Record<string, unknown>) => void;
}) {
  const groups = createMemo(() => {
    const byState = new Map(props.view.groups.map((group) => [group.state, group.cards]));
    return GROUPS.map((state) => ({ state, cards: byState.get(state) ?? [] }));
  });
  return (
    <section class="tm-board" aria-label={tr("Queue")} data-team-board>
      <For each={groups()}>
        {(group) => (
          <div class="tm-group" data-group={group.state}>
            <h3><span>{tr(LABELS[group.state] ?? group.state)}</span><span>{group.cards.length}</span></h3>
            <For each={group.cards}>
              {(card) => (
                <TeamCardView card={card} selected={props.selected === card.node} busy={props.busy}
                  onSelect={() => props.onSelect(card.node)}
                  onAction={(kind, fields) => props.onAction(card, kind, fields)} />
              )}
            </For>
          </div>
        )}
      </For>
    </section>
  );
}

/** The tab's panel, from a view the store has already projected. */
export function TeamPanel(props: {
  view: TeamView;
  busy: boolean;
  failure: { code: string; message: string } | null;
  onAction: (card: TeamCard, kind: string, fields?: Record<string, unknown>) => void;
  onPage: (cursor: string) => void;
  onAddTask: () => void;
}) {
  const [selectedNode, setSelectedNode] = createSignal<number | null>(null);
  const all = createMemo(() => props.view.groups.flatMap((group) => group.cards));
  const selectedCard = createMemo(() => all().find((card) => card.node === selectedNode()) ?? null);
  return (
    <div class="tm-panel">
      <TeamBar view={props.view} />
      <TeamMembers view={props.view} />
      <Show when={props.failure}>
        {(failure) => (
          <div class="tm-notice tm-bad" role="alert" data-team-failure>
            {failure().code === "cursor_reset" ? failure().message : tr("Teams read failed: %1", failure().message)}
          </div>
        )}
      </Show>
      <div class="tm-toolbar">
        <span class="grow" />
        <Show when={props.view.truncated}><span class="tm-empty">{tr("Some cards are not shown.")}</span></Show>
        <Show when={props.view.next}>
          {(cursor) => <button type="button" class="btn small" onClick={() => props.onPage(cursor())}>{tr("More…")}</button>}
        </Show>
        <button type="button" class="btn small" onClick={props.onAddTask}>{tr("Add task")}</button>
      </div>
      <TeamBoard view={props.view} busy={props.busy} selected={selectedNode()} onSelect={setSelectedNode} onAction={props.onAction} />
      <Show when={selectedCard()}>{(card) => <TeamDetail card={card()} />}</Show>
      <Show when={!all().length}><div class="tm-empty">{tr("The queue is empty. Send a task to the team from the task board.")}</div></Show>
    </div>
  );
}

/** The two exits from an uncertain action; nothing repeats it by itself. */
export function TeamUncertain(props: { detail: string; busy: boolean; onCheck: () => void; onDismiss: () => void }) {
  return (
    <div class="tm-uncertain" role="alert" data-team-uncertain>
      <span>{props.detail}</span>
      <span class="tm-actions">
        <button type="button" class="btn small primary" disabled={props.busy} onClick={props.onCheck}>{tr("Check")}</button>
        <button type="button" class="btn small" disabled={props.busy} onClick={props.onDismiss}>{tr("Dismiss")}</button>
      </span>
    </div>
  );
}

/** The selected session's team: resolved on opening the tab, then followed. */
export function TeamTab() {
  const row = selected();
  let closed = false;
  onMount(() => {
    void teams.probe();
    const identity = { sessionId: row?.sessionId ?? null, runId: row?.runId ?? null };
    void teams.leadTeamFor(row?.key ?? "", identity);
  });
  createEffect(() => {
    const hidden = pageHidden() || page() !== "sessions" || detailTab() !== "team";
    void teams.hidden(hidden);
  });
  onCleanup(() => { closed = true; void teams.closeTeam(); });

  const view = createMemo(() => teams.team());
  const teamId = createMemo(() => (row ? teams.sessionTeams()[row.key] : undefined));
  const teamView = createMemo(() => (view() && teamId() && view()!.bar.id === teamId() ? view()! : undefined));
  const notice = createMemo(() => teams.failure());

  /** One lead decision; the core asks for the person's confirmation when it is destructive. */
  const act = (card: TeamCard, kind: string, fields: Record<string, unknown> = {}) => {
    const current = teamView();
    if (!current || closed) return;
    const call: TeamCall = {
      team: current.bar.id,
      team_name: current.bar.name,
      expected_revision: current.revision,
      request_id: newRequestId(),
      action: { kind, ...fields },
    };
    void teams.run(call);
  };
  return (
    <>
      <Show when={teamView()} fallback={
        <div class="tm-empty" data-team-waiting>
          <Show when={teamId() === undefined} fallback={tr("This session is not a team lead.")}>
            {tr("Finding the team of this session…")}
          </Show>
        </div>
      }>
        {(current) => (
          <>
            <TeamPanel view={current()} busy={teams.busy()} failure={notice()}
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
