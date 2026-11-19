// Adapted from WhiteWh's proposal card presentation. Our cards carry a bounded
// list of decision words, not editable fork-engine proposals or permissions.
import { For, Show } from "solid-js";
import { tr } from "../../i18n";
import type { DrawerRow } from "../../store/teamDrawers";
import type { DrawerAction } from "./TeamReports";
import { PHASE_CARD_HIDE, isLeadNotice, waitingDecisions, waitingKindLabel, waitingKindSentence, waitingNote, waitingText } from "./decisions";
import { openPhase } from "../../store/team-phases";
import { actorLabel, itemState } from "./feed";
import { setOpenTeamNode } from "./openNode";

/** A card's text with each "#N" as a link that opens task N on the board (G15). */
function CardText(props: { text: string; team?: string; onOpenNode?: () => void }) {
  // The whole decimal token is read first; one that is not a task number
  // (leading zero, too long, beyond the node range) stays plain text.
  const parts = () => props.text.split(/(#\d+)/);
  return <pre class="team-drawer-text"><For each={parts()}>{(part, at) => {
    const node = at() % 2 === 1 && /^#[1-9]\d{0,9}$/.test(part) ? Number(part.slice(1)) : 0;
    return props.team && node > 0 && node <= 0xffffffff
      ? <button type="button" class="team-member-link team-card-node" data-card-node={node} title={tr("Open task #%1", node)}
          onClick={() => { setOpenTeamNode({ team: props.team!, node }); props.onOpenNode?.(); }}>{part}</button>
      : part;
  }}</For></pre>;
}

export function ProposalCards(props: { rows: DrawerRow[]; busy: boolean; onAction: DrawerAction; team?: string; onOpenNode?: () => void; focusCard?: number }) {
  return <div class="team-proposals" role="list" aria-label={tr("Cards")}>
    <For each={props.rows} fallback={<p class="dim">{tr("No cards on this page.")}</p>}>{(row) =>
      <Show when={row.card}>{(card) => <article class="team-proposal" classList={{ "team-proposal-focus": props.focusCard === card().card }} data-proposal={card().card}>
        <header><b>{waitingKindLabel(card())}</b><span class="dim">{actorLabel(row)}</span><span class="dim">{card().decision === "acknowledged" && card().kind === "person_note_to_lead" ? tr("Acknowledged by the lead") : itemState(card().state)}</span></header>
        <Show when={!isLeadNotice(card().kind) && card().kind !== "person_note_to_lead"}><p class="dim">{waitingKindSentence(card())}</p></Show>
        <Show when={!isLeadNotice(card().kind) || card().state !== "open"} fallback={
          <button type="button" class="btn small" disabled={props.busy} data-open-lead-notice
            onClick={() => props.onAction("read_lead_notice", { card: card().card })}>
            {card().kind === "lead_reply_to_person" ? tr("Open reply") : tr("Open update")}
          </button>
        }>
          <CardText text={waitingText(card())} team={props.team} onOpenNode={props.onOpenNode} />
        </Show>
        <Show when={row.text_truncated}><p class="dim">{tr("Only the beginning of this text is shown.")}</p></Show>
        <Show when={(card().notes ?? []).length}><p class="dim">{tr("Service notes")}</p>
          <For each={card().notes}>{(note) => <p class="dim">{waitingNote(note)}</p>}</For>
        </Show>
        <Show when={card().state === "open" && card().phase_reference}><p class="dim" data-phase-card-hide>{PHASE_CARD_HIDE()}</p></Show>
        <Show when={card().state === "open"}><div class="team-drawer-actions">
          {/* A phase card is decided on its phase (zerus-63rru.197 C5). */}
          <Show when={props.team ? card().phase_reference : undefined}>{(reference) => <button type="button" class="btn small primary" disabled={props.busy} data-open-phase
            onClick={() => { openPhase(props.team!, reference().phase); props.onOpenNode?.(); }}>{tr("Open phase")}</button>}</Show>
          <For each={waitingDecisions(card())}>{(choice) => <button type="button" class="btn small" classList={{ danger: choice.danger }} disabled={props.busy} data-card-decision={choice.decision}
            onClick={() => props.onAction("decide", { card: card().card, decision: choice.decision })}>{choice.text}</button>}</For>
        </div></Show>
      </article>}</Show>
    }</For>
  </div>;
}
