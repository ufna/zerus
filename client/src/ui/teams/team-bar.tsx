import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { progress, stateLabel, stateTone } from "./team-model";
import { TeamBarActions } from "./TeamActions";
import type { TeamView } from "./types";

// Task #427/#463 (the fork's strip): the team bar. One row with fixed gaps: the
// name shrinks, the progress track keeps a min and a max, the counts never
// wrap, and below 1100 px the actions fold by the strip's own width. M1 shows
// only the operations of a queue team.

/** The strip's fold thresholds, on the strip's own width. */
export const compactFor = (width: number): "none" | "low" | "high" => (width < 900 ? "high" : width < 1100 ? "low" : "none");

export function TeamBar(props: {
  view: TeamView;
  busy: boolean;
  onAction: (kind: string) => void;
  /** The strip's width (tests); the strip's own measured width otherwise. */
  width?: number;
}) {
  const counts = createMemo(() => progress(props.view));
  const [measured, setMeasured] = createSignal(0);
  const [viewport, setViewport] = createSignal(typeof window === "undefined" ? 1200 : window.innerWidth);
  let bar: HTMLElement | undefined;
  onMount(() => {
    const resize = () => setViewport(window.innerWidth);
    window.addEventListener("resize", resize);
    onCleanup(() => window.removeEventListener("resize", resize));
    if (!bar) return;
    const observe = () => setMeasured(Math.round(bar!.getBoundingClientRect().width));
    observe();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => observe());
    observer.observe(bar);
    onCleanup(() => observer.disconnect());
  });
  const width = () => props.width ?? (measured() || viewport());
  const compact = (): "none" | "low" | "high" => compactFor(width());
  const segments = () => [
    { key: "accepted", n: counts().accepted },
    { key: "reported", n: counts().reported },
    { key: "active", n: counts().active },
    { key: "waiting", n: counts().waiting },
  ].filter((segment) => segment.n > 0);
  const countsTip = () => [tr("Accepted %1", counts().accepted), tr("In review %1", counts().reported),
    tr("In progress %1", counts().active), tr("Waiting %1", counts().waiting)].join(", ");
  return (
    <header class="team-bar" ref={bar} classList={{ "team-bar-compact": compact() !== "none" }} data-compact={compact()} data-strip data-team-bar>
      <Icon name="users" size={16} />
      <b class="team-name ell" title={props.view.bar.goal || props.view.bar.name}>{props.view.bar.name}</b>
      <span class={`tm-state-badge tone-${stateTone(props.view.bar.status)}`} data-state={props.view.bar.status}>{stateLabel(props.view.bar.status)}</span>
      <div class="team-progress" role="img" aria-label={countsTip()} title={countsTip()}>
        <For each={segments()}>{(segment) => <i class={`seg ${segment.key}`} style={{ "flex-grow": String(segment.n) }} />}</For>
      </div>
      <span class="mono team-count">{counts().accepted}/{counts().total}</span>
      <span class="team-chips" data-strip-chips>
        <span class="team-chip" title={tr("Active members")}>{tr("%1 active", props.view.bar.counts.members.active)}</span>
        <span class="team-chip" title={tr("Open inbox items")}>{tr("%1 inbox", props.view.bar.counts.inbox)}</span>
        <span class="team-chip" title={tr("Open cards")}>{tr("%1 cards", props.view.bar.counts.cards)}</span>
      </span>
      <span class="grow" />
      <Show when={compact() === "none"}>
        <TeamBarActions view={props.view} busy={props.busy} onAction={props.onAction} />
      </Show>
    </header>
  );
}
