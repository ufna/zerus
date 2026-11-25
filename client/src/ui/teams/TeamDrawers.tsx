// Adapted from WhiteWh's TeamDrawers: the feed/inbox rows and drawer header.
// Our store returns one ascending snapshot page, not the fork's event cache.
import { For, Show, Switch, Match, Suspense, createEffect, createMemo, createSignal, lazy, on, onCleanup } from "solid-js";
import { tr } from "../../i18n";
import { teams } from "../../store/teams";
import { RecordedPageExport, exportRecordedPage, type PageExportPort } from "../../store/teamProfileExport";
import { Icon } from "../lib/icons";
import { TextInput } from "../lib/TextField";
import { TeamDrawerRead, readTeamDrawer, type DrawerKind, type DrawerRow, type TeamDrawer, type DrawerPort } from "../../store/teamDrawers";
import { actorLabel, feedClock, historyLine, inboxCloses, inboxKind, itemState, journalDetail, journalSentence } from "./feed";
import { InboxText, Report, TeamReports, type DrawerAction, type ReportTarget } from "./TeamReports";
import { activeTeam } from "./actions";
import { ProposalCards } from "./ProposalCards";
import type { TeamView } from "./types";
import "../styles/team-drawers.css";

const TeamTimeline = lazy(() => import("./TeamTimeline").then((module) => ({ default: module.TeamTimeline })));

const TeamLeadChat = lazy(() => import("./TeamLeadChat").then((module) => ({ default: module.TeamLeadChat })));

export const drawerLabel = (kind: TeamDrawer) => ({ timeline: tr("Timeline"), feed: tr("Feed"), inbox: tr("Inbox"), reports: tr("Reports"), cards: tr("Cards"), chat: tr("Lead chat") })[kind];

export function TeamDrawerButtons(props: { value: TeamDrawer | null; onOpen: (kind: TeamDrawer) => void }) {
  return <div class="team-drawer-buttons" role="group" aria-label={tr("Team drawers")}>
    <For each={["timeline", "feed", "inbox", "reports", "cards", "chat"] as const}>{(kind) =>
      <button type="button" class="btn small" aria-expanded={props.value === kind} data-team-drawer={kind} onClick={() => props.onOpen(kind)}>{drawerLabel(kind)}</button>
    }</For>
  </div>;
}

export function TeamFeed(props: { rows: DrawerRow[] }) {
  return <div class="team-drawer-rows" aria-label={tr("Feed")}>
    <For each={props.rows} fallback={<p class="dim">{tr("No events on this page.")}</p>}>{(row) =>
      <article class="team-drawer-row" data-feed-seq={row.seq} title={historyLine(row)}>
        <header>
          <time class="mono" title={row.at_ms ? new Date(row.at_ms).toLocaleString() : ""}>{feedClock(row.at_ms)}</time>
          <span class="dim" data-feed-actor>{actorLabel(row)}</span>
          <b data-feed-sentence>{journalSentence(row.kind)}</b>
          <Show when={row.subject_node}>{(node) => <span class="team-pill" data-feed-task>#{node()}</span>}</Show>
          <Show when={row.subject_member}>{(member) => <span class="team-pill" data-feed-member>{member()}</span>}</Show>
        </header>
        <Show when={journalDetail(row)}>{(detail) => <p class="team-drawer-text" data-feed-detail>{detail()}</p>}</Show>
        <Show when={row.text_truncated}><span class="dim">{tr("Only the beginning of this text is shown.")}</span></Show>
      </article>
    }</For>
  </div>;
}

/** The team's status decides what an item offers: the store answers a
 * question only on a running or paused team (ops talk.rs, common.rs active). */
type InboxProps = { busy: boolean; onAction: DrawerAction; status: string } & ReportTarget;

function InboxRow(props: { row: DrawerRow } & InboxProps) {
  const [answer, setAnswer] = createSignal("");
  const question = () => props.row.state === "open" && props.row.kind.startsWith("question:");
  return <Show when={props.row.kind !== "report"} fallback={<Report row={props.row} busy={props.busy} team={props.team} onOpenNode={props.onOpenNode} />}>
    <article class="team-drawer-row" data-inbox={props.row.seq}>
      <header><span class="team-pill">{inboxKind(props.row.kind)}</span><b>{actorLabel(props.row)}</b><span class="dim">{itemState(props.row.state)}</span></header>
      <InboxText row={props.row} />
      <Show when={question() && activeTeam(props.status)}><div class="team-drawer-actions">
        <TextInput class="field" maxlength={4000} value={answer()} aria-label={tr("Your answer")} disabled={props.busy} onInput={(event) => setAnswer(event.currentTarget.value)} />
        <button type="button" class="btn small" disabled={props.busy || !answer().trim()} data-inbox-answer onClick={() => props.onAction("reply", { item: props.row.seq, text: answer() })}>{tr("Answer")}</button>
      </div></Show>
      <Show when={question() && !activeTeam(props.status)}><p class="dim" data-inbox-closes>{tr("Questions can be answered only while the team is running or paused.")}</p></Show>
      <Show when={inboxCloses(props.row, props.status)}>{(line) => <p class="dim" data-inbox-closes>{line()}</p>}</Show>
    </article>
  </Show>;
}

export function TeamInbox(props: { rows: DrawerRow[] } & InboxProps) {
  return <div class="team-drawer-rows" aria-label={tr("Inbox")}>
    <For each={props.rows.map((row) => row.seq)} fallback={<p class="dim">{tr("No inbox items on this page.")}</p>}>{(seq) => <Show when={props.rows.find((row) => row.seq === seq)}>{(row) =>
      <InboxRow row={row()} busy={props.busy} onAction={props.onAction} status={props.status} team={props.team} onOpenNode={props.onOpenNode} />}</Show>}</For>
  </div>;
}

/** The kinds of card a member's start writes (src/desktop/teams/launch/card.rs). */
const START_KINDS = new Set(["launch_blocked", "launch_uncertain"]);
/** Cards pages Launch again reads before it says the card was not found. */
export const START_CARD_PAGES = 4;

/** The member a start card names: the writer's member parameter, else the
 * older writer's text prefix ("Member m2: ..."). */
function startMember(card: NonNullable<DrawerRow["card"]>): string | undefined {
  const named = Array.isArray(card.parameters) ? card.parameters.find((pair) => Array.isArray(pair) && pair[0] === "member")?.[1] : undefined;
  if (typeof named === "string") return named;
  return /^Member (lead|m[1-9][0-9]{0,9}):/.exec(card.text)?.[1];
}

type StartFocus = { state: "seeking"; pages: number } | { state: "found"; card: number; retry: boolean } | { state: "none"; searched: boolean };

export function TeamDrawers(props: { kind: TeamDrawer; view: TeamView; busy: boolean; onAction: DrawerAction; onClose: () => void; port?: DrawerPort; visible?: boolean; exportPort?: PageExportPort;
  /** Launch again: the member whose own start card Cards looks for (review of #2558). */
  focusMember?: string }) {
  const [tick, setTick] = createSignal(0);
  const [readRevision, setReadRevision] = createSignal<number>();
  const model = new TeamDrawerRead(props.port ?? readTeamDrawer, () => setTick((n) => n + 1));
  const read = () => (tick(), model);
  const exporter = new RecordedPageExport(props.exportPort ?? exportRecordedPage, () => setTick((n) => n + 1));
  const exported = () => (tick(), exporter);
  const canExport = () => props.kind === "timeline" && !!read().page?.timeline && !read().loading && !read().error && !exported().busy;
  const active = () => props.kind !== "timeline" || (props.visible ?? teams.visible?.() ?? true);
  const target = createMemo(() => JSON.stringify([props.view.bar.id, props.kind, active()]));
  createEffect(on(target, () => {
    setReadRevision(props.view.revision);
    exporter.retire();
    if (!active() || props.kind === "chat") model.close();
    else model.open(props.view.bar.id, props.kind === "timeline" ? "feed" : props.kind === "reports" ? "inbox" : props.kind as DrawerKind);
  }));
  createEffect(on(() => props.view.revision, (revision) => { setReadRevision(revision); if (active() && props.kind !== "timeline") model.refresh(); }, { defer: true }));
  onCleanup(() => { model.close(); exporter.retire(); });
  // Launch again leads to the member's own start card: a bounded lookup from
  // the first Cards page, then its Retry is focused, or one sentence says why
  // there is none. It decides nothing itself (review of #2558).
  const [focus, setFocus] = createSignal<StartFocus>();
  createEffect(on(() => props.focusMember, (member) => {
    setFocus(member && props.kind === "cards" ? { state: "seeking", pages: 1 } : undefined);
    if (member && props.kind === "cards" && read().page?.after) model.first();
  }));
  createEffect(() => {
    const seeking = focus(), member = props.focusMember, state = read();
    if (seeking?.state !== "seeking" || !member || state.loading || state.error || !state.page) return;
    const row = state.page.rows.find((candidate) => candidate.card && candidate.card.state === "open" && START_KINDS.has(candidate.card.kind) && startMember(candidate.card) === member);
    if (row?.card) {
      const retry = (row.card.decisions ?? []).includes("retry");
      setFocus({ state: "found", card: row.card.card, retry });
      if (retry) queueMicrotask(() => document.querySelector<HTMLButtonElement>(`[data-proposal="${row.card!.card}"] [data-card-decision="retry"]`)?.focus());
      return;
    }
    if (state.page.next && seeking.pages < START_CARD_PAGES) { setFocus({ state: "seeking", pages: seeking.pages + 1 }); model.next(); return; }
    setFocus({ state: "none", searched: !!state.page.next });
  });
  const focusLine = () => {
    const shown = focus(), member = props.focusMember ?? "";
    if (shown?.state === "found") return shown.retry ? tr("Retry on %1's start card starts it again.", member) : tr("%1's start card offers no Retry now; the card says why.", member);
    if (shown?.state === "none") return shown.searched ? tr("%1's start card is not on the Cards pages that were read.", member) : tr("%1 has no open start card. Zerus starts the member when there is room.", member);
    return undefined;
  };
  const busy = () => props.busy || read().loading || !!read().error || readRevision() !== props.view.revision;
  const act: DrawerAction = (kind, fields) => { if (!busy()) props.onAction(kind, fields); };
  return <section class="team-drawer" aria-label={drawerLabel(props.kind)} data-team-drawer-panel={props.kind}>
    <header class="team-drawer-head"><b>{drawerLabel(props.kind)}</b><span class="grow" />
      <Show when={props.kind === "timeline"}>
        <button type="button" class="btn small" data-timeline-timings disabled={read().loading || !read().page || exported().busy} onClick={() => model.timings(!model.timingsRequested())}>{read().timingsRequested() ? tr("Events only") : tr("Read recorded timings")}</button>
        <button type="button" class="btn small" data-timeline-export disabled={!canExport()} onClick={() => { const page = model.page; if (canExport() && page) void exporter.save(page); }}>{tr("Export page…")}</button>
      </Show>
      <Show when={props.kind !== "chat"}><button type="button" class="btn small" disabled={read().loading} data-drawer-refresh onClick={() => model.refresh()}>{tr("Refresh")}</button></Show>
      <button type="button" class="icon-btn small" aria-label={tr("Close")} onClick={props.onClose}><Icon name="x" size={14} /></button>
    </header>
    <Show when={exported().outcome === "saved"}><p role="status">{tr("Page exported.")}</p></Show>
    <Show when={exported().outcome === "cancelled"}><p role="status">{tr("Export cancelled.")}</p></Show>
    <Show when={exported().outcome === "unknown"}><p role="alert">{tr("Export not confirmed: %1. Check the destination before exporting again.", exported().error)}</p></Show>
    <Show when={focusLine()}>{(line) => <p class="notice team-notice" role="status" data-launch-focus>{line()}</p>}</Show>
    <Show when={read().loading}><p role="status">{tr("Reading the team page…")}</p></Show>
    <Show when={read().error}>{(error) => <p class="error-text" role="alert">{error().startsWith("drawer_busy:") ? tr("A team drawer read is still finishing. Use Refresh.") : tr("Teams read failed: %1", error())}</p>}</Show>
    <Show when={props.kind === "chat"}><Suspense fallback={<p>{tr("Loading…")}</p>}><TeamLeadChat view={props.view} /></Suspense></Show>
    <Show when={read().page}>{(page) => <>
      <Switch>
        <Match when={props.kind === "timeline"}><Suspense fallback={<p>{tr("Loading…")}</p>}><TeamTimeline page={page()} /></Suspense></Match>
        <Match when={props.kind === "feed"}><TeamFeed rows={page().rows} /></Match>
        <Match when={props.kind === "inbox"}><TeamInbox rows={page().rows} busy={busy()} onAction={act} status={props.view.bar.status} team={props.view.bar.id} onOpenNode={props.onClose} /></Match>
        <Match when={props.kind === "reports"}><TeamReports rows={page().rows} busy={busy()} onAction={act} team={props.view.bar.id} onOpenNode={props.onClose} /></Match>
        <Match when={props.kind === "cards"}><ProposalCards rows={page().rows} busy={busy()} onAction={act} team={props.view.bar.id} onOpenNode={props.onClose}
          focusCard={(() => { const shown = focus(); return shown?.state === "found" ? shown.card : undefined; })()} /></Match>
      </Switch>
      <footer class="team-drawer-actions"><span class="dim">{tr("One page is shown. Refresh reads this page again.")}</span><span class="grow" />
        <Show when={page().after}><button type="button" class="btn small" disabled={read().loading} data-drawer-first onClick={() => model.first()}>{tr("First page")}</button></Show>
        <Show when={page().next}><button type="button" class="btn small" disabled={read().loading} data-drawer-next onClick={() => model.next()}>{tr("Next page")}</button></Show>
      </footer>
    </>}</Show>
  </section>;
}
