import { For, Show } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import type { WaitingItem, WaitingView } from "../../store/teams";
import { waitingAge, waitingDecisions, waitingKindIcon, waitingKindLabel } from "./decisions";

// Waiting for you (the fork's Waiting.tsx, adapted to M1): the open owner
// cards of every team, oldest first, each with its team, its kind and the
// person's one-click decisions. The fork's proposals, permissions, deferred
// requests and report drawers belong to later milestones and are not here.

/** One card: its team, kind, text, age and decisions. */
export function WaitingCard(props: {
  item: WaitingItem;
  teamName: string;
  now: number;
  busy: boolean;
  onDecide: (item: WaitingItem, decision: string) => void;
  onOpenTeam: (item: WaitingItem) => void;
}) {
  return (
    <article class="waiting-card" data-waiting={props.item.id} data-kind={props.item.kind}>
      <div class="waiting-card-head">
        <Icon name={waitingKindIcon(props.item.kind)} size={14} />
        <b class="waiting-kind">{waitingKindLabel(props.item)}</b>
        <button type="button" class="waiting-team btn ghost small" title={tr("Open the team")} onClick={() => props.onOpenTeam(props.item)}>
          {props.teamName || props.item.team}
        </button>
        <span class="waiting-age dim">{waitingAge(props.item.created_ms, props.now)}</span>
      </div>
      <p class="waiting-text">{props.item.text}</p>
      <div class="waiting-decisions">
        <For each={waitingDecisions(props.item)}>
          {(choice) => (
            <button type="button" class="btn small" classList={{ danger: choice.danger, primary: !choice.danger }}
              disabled={props.busy} data-decision={choice.decision}
              onClick={() => props.onDecide(props.item, choice.decision)}>{choice.text}</button>
          )}
        </For>
      </div>
    </article>
  );
}

/** The page's list, paged by its `next` cursor. */
export function WaitingList(props: {
  view: WaitingView;
  teamNames: ReadonlyMap<string, string>;
  now: number;
  busy: boolean;
  onDecide: (item: WaitingItem, decision: string) => void;
  onOpenTeam: (item: WaitingItem) => void;
  onPage: (cursor: string) => void;
}) {
  return (
    <section class="waiting-list" aria-label={tr("Waiting for you")} data-waiting-list>
      <Show when={props.view.items.length} fallback={<div class="team-empty dim">{tr("Nothing waits for you.")}</div>}>
        <For each={props.view.items}>
          {(item) => <WaitingCard item={item} teamName={props.teamNames.get(item.team) ?? ""} now={props.now} busy={props.busy}
            onDecide={props.onDecide} onOpenTeam={props.onOpenTeam} />}
        </For>
      </Show>
      <Show when={props.view.next}>
        {(cursor) => (
          <button type="button" class="btn small" disabled={props.busy} onClick={() => props.onPage(cursor())}>{tr("More…")}</button>
        )}
      </Show>
    </section>
  );
}
