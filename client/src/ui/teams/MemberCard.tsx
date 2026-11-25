import { For, Show, createMemo, createSignal } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { accountLabel } from "./members";
import { memberStatus } from "./team-model";
import { MemberActions } from "./TeamActions";
import { RoleChip, StageChip, TeamAgent } from "./TeamChips";
import { memberWork, type MemberWork } from "./pipeline-view";
import { memberSessions, openMemberSession, type MemberSession } from "./member-session";
import "../styles/team-member-link.css";
import type { MemberView, TaskView } from "./types";

// The reference's compact right-side member card. Only current page assignments
// establish its work line/queue; missing model, findings and full-queue data are
// omitted, never inferred from a role or a live session. The member's name,
// "Session linked" and Open session in its menu open the member's own bound
// session; `session` is the members panel's one lookup for every member.
export function MemberCard(props: { member: MemberView; teamStatus: string; cards?: readonly TaskView[]; busy: boolean; onAction: (kind: string, member: string) => void; onSelect?: (node: number) => void; session?: MemberSession }) {
  const [detailsOpen, setDetailsOpen] = createSignal(false);
  const session = createMemo(() => props.session ?? memberSessions([props.member]).get(props.member.id)!);
  const openSession = () => { const row = session().row; if (row) openMemberSession(row); };
  const status = () => memberStatus(props.member);
  const work = createMemo(() => memberWork(props.member, props.cards ?? []));
  const current = () => work().find((item) => (item.stage?.state ?? item.card.state) === "in_progress") ?? work()[0];
  const queue = () => work().filter((item) => item.stage && ["pending", "waiting"].includes(item.stage.state));
  const activity = (item: MemberWork) => {
    if (item.stage) {
      // The task a stage checks: the card it is folded into, or an orphan
      // stage's own target; an orphan without one names its own node only.
      const target = item.card.kind === "stage" ? item.stage.target : item.card.node;
      if (target === undefined || target === null) return tr("Stage #%1", item.stage.node);
      if (item.stage.state === "in_progress") return item.stage.kind === "review" ? tr("Reviewing #%1", target)
        : item.stage.kind === "test" ? tr("Testing #%1", target)
        : item.stage.kind === "land" ? tr("Landing #%1", target) : tr("Stage work on #%1", target);
      return item.stage.state === "reported" ? tr("Stage reported for #%1", target) : tr("Queued for #%1", target);
    }
    if (item.card.held) return tr("Task #%1 is held", item.card.node);
    return item.card.state === "in_progress" ? tr("Working on #%1", item.card.node) : item.card.state === "reported" ? tr("Reported #%1", item.card.node) : tr("Assigned to #%1", item.card.node);
  };
  const assignment = (item: MemberWork) => <>
    <span class="team-member-assignment" title={activity(item)}>{activity(item)}</span>
    <span class="team-member-task-title ell" title={item.card.title}>{item.card.title}</span>
  </>;
  return <div class="team-member-card" classList={{ "team-lead": props.member.lead }} data-member={props.member.id} data-state={status().state} data-state-color={status().tone}>
    <TeamAgent agent={props.member.agent} />
    <div class="team-member-content">
      <div class="team-member-line">
        {/* A member whose ID equals its role shows only its role chip: that
            chip is its displayed name, so it opens the session too. */}
        <Show when={props.member.id === props.member.role && session().row} fallback={<RoleChip role={props.member.role || props.member.id} lead={props.member.lead} />}>
          <button type="button" class="team-member-name team-member-link team-member-chip-link" title={tr("Open this member's session")} onClick={openSession}>
            <RoleChip role={props.member.role || props.member.id} lead={props.member.lead} title={tr("Open this member's session")} />
          </button>
        </Show>
        <Show when={props.member.id !== props.member.role}>
          <Show when={session().row} fallback={<b class="team-member-name ell" title={props.member.id}>{props.member.id}</b>}>
            <button type="button" class="team-member-name team-member-link ell" title={tr("Open this member's session")} onClick={openSession}>{props.member.id}</button>
          </Show>
        </Show>
        <span class="team-member-agent ell" title={props.member.model || props.member.agent}>{props.member.model || props.member.agent}</span>
        <Show when={props.member.state !== "active"}><span class="team-member-state tm-state-badge" data-tone={status().tone}>{status().label}</span></Show>
        <MemberActions member={props.member} teamStatus={props.teamStatus} busy={props.busy} onAction={props.onAction} detailsOpen={detailsOpen()} onDetails={() => setDetailsOpen(!detailsOpen())}
          session={session()} onOpenSession={openSession} />
      </div>
      {/* A member who is not active shows its state chip alone; a line under
          the name that repeated the chip said nothing more. A member that was
          never started says how it starts (zerus-63rru.197 A3). */}
      <Show when={props.member.state === "not_created" && ["running", "paused"].includes(props.teamStatus)}>
        <p class="team-member-start dim" data-member-start>{tr("Zerus starts this member when there is room. If its start failed, Retry on its card in Cards starts it again.")}</p>
      </Show>
      <Show when={props.member.state === "active" ? current() : undefined} fallback={<Show when={props.member.state === "active"}><p class="team-member-idle">{tr("No current assignment on this page.")}</p></Show>}>
        {(item) => <div class="team-member-activity">
          <Show when={props.onSelect} fallback={<div class="team-member-work-copy">{assignment(item()!)}</div>}><button type="button" class="team-member-work-copy" onClick={() => props.onSelect?.(item()!.card.node)}>{assignment(item()!)}</button></Show>
          <Show when={item()!.stage} fallback={<Show when={item()!.card.round > 0}><span class="dim">{tr("Round %1", item()!.card.round)}</span></Show>}>{(stage) => <StageChip stage={stage()} rounds={item()!.card.rounds} />}</Show>
        </div>}
      </Show>
      <Show when={queue().length}>
        <div class="team-member-queue"><span class="team-queue-caption"><Icon name="workflow" size={11} />{tr("Stage queue on this page")} <span>{queue().length}</span></span>
          <For each={queue().slice(0, 5)}>{(item) => <div class="team-member-queue-row"><StageChip stage={item.stage!} rounds={item.card.rounds} /><Show when={props.onSelect} fallback={<span>#{item.card.node} {item.card.title}</span>}><button type="button" class="ell" title={item.card.title} onClick={() => props.onSelect?.(item.card.node)}>#{item.card.node} {item.card.title}</button></Show></div>}</For>
          <Show when={queue().length > 5}><span class="dim">{tr("+%1 more", queue().length - 5)}</span></Show>
        </div>
      </Show>
      <Show when={detailsOpen()}><div class="team-member-details-body"><span class="team-account">{accountLabel(props.member)}</span>
        <Show when={session().row} fallback={<>
          <span class="team-member-session">{props.member.session ? tr("Session linked") : tr("No linked session")}</span>
          <Show when={props.member.session}><span class="dim">{session().reason}</span></Show>
        </>}>
          <button type="button" class="team-member-session team-member-link" title={tr("Open this member's session")} onClick={openSession}>{tr("Session linked")}</button>
        </Show>
      </div></Show>
    </div>
  </div>;
}
