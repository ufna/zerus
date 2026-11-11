import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount, untrack } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { actionNeeded } from "../../store/attention";
import { orderedRows } from "../../store/catalog";
import { now, setDetailTab, setPage, setSelectedKey } from "../../store/ui";
import { teams, type WaitingItem } from "../../store/teams";
import { WaitingList } from "../teams/Waiting";
import { TeamConfirmDialog } from "../teams/TeamConfirmDialog";
import { TeamUncertain } from "../teams/TeamTab";
import { setOpenTeamNode } from "../teams/openNode";
import { rowIsSession, rowSessionKeys } from "../teams/session-binding";
import { teamRequests, teamSessions, type RequestRow, type TeamRequest } from "./waiting-requests";
import "../styles/teams.css";

// Waiting for you: the open owner cards of every team (the `waiting` key, one
// bounded page at a time, opened on its first page) and, from the desktop's
// own session attention data, the requests of team sessions that only a
// person can answer. A card is decided in place through the Teams store; a
// session request opens the session. Open team and Open task find the lead's
// row by the same machine and launch binding the Team tab uses. With the
// preview off the rail entry does not exist and this page reads nothing.

/** The session rows that could be a person's request, read lazily: the
 * matcher stops at its limit. */
function* requestRows(): Generator<RequestRow> {
  for (const row of orderedRows()) {
    if (row.attention !== "question" && row.attention !== "approval") continue;
    yield { key: row.key, name: row.name, attention: row.attention, caption: row.caption, actionNeeded: actionNeeded(row), quiet: !!row.quiet, identities: rowSessionKeys(row) };
  }
}

export function WaitingScreen() {
  const [busy, setBusy] = createSignal(false);
  const [notice, setNotice] = createSignal("");
  /** An Open team or Open task whose team is still being looked up in the list. */
  const [opening, setOpening] = createSignal<{ item: WaitingItem; task: boolean } | null>(null);
  onMount(() => { void teams.openWaiting().catch(() => {}); });
  // Leaving the page goes back to the first page, which the rail counts.
  onCleanup(() => { void teams.firstWaitingPage().catch(() => {}); teams.clearFailure("waiting"); });
  const view = () => teams.waitingView();
  const names = createMemo(() => new Map(teams.listed().map((team) => [team.id, team.name])));
  const sessions = createMemo(() => teamSessions(teams.listed(), teams.team()));
  const requests = createMemo<TeamRequest[]>(() => teamRequests(requestRows(), sessions()));
  createEffect(() => {
    const asked = opening();
    if (!asked) return;
    const found = teams.summaryOf(asked.item.team);
    if (found.state === "reading") { setNotice(tr("Reading the team list…")); return; }
    setOpening(null);
    if (found.state !== "found") {
      setNotice(found.state === "none" ? tr("The team is not on this computer's team list.") : tr("The team is not on the pages of the team list that were read."));
      return;
    }
    const row = untrack(() => orderedRows().find((candidate) => rowIsSession(candidate, found.summary.lead_session)));
    if (!row) { setNotice(tr("The team's lead session is not on the session list.")); return; }
    setNotice("");
    if (asked.task && asked.item.node !== null) setOpenTeamNode({ team: asked.item.team, node: asked.item.node });
    setPage("sessions"); setSelectedKey(row.key); setDetailTab("team");
  });
  const decide = async (item: WaitingItem, decision: string) => {
    if (busy()) return;
    setBusy(true); setNotice("");
    try {
      const outcome = await teams.decide(item, decision);
      if (!outcome) setNotice(tr("The team could not be read. Nothing was sent."));
      else if (outcome.outcome === "failed") setNotice(outcome.detail);
      else if (outcome.outcome === "pending") setNotice(tr("The decision waits for the team."));
    } finally { setBusy(false); }
  };
  /** Open team, and Open task with the node selected, so the decision on its
   * reported work is made on the board, not here. */
  const open = (item: WaitingItem, task: boolean) => { if (!task || item.node !== null) setOpening({ item, task }); };
  const openRequest = (request: TeamRequest) => {
    setPage("sessions"); setSelectedKey(request.key); setDetailTab("activity");
  };
  const readFailure = () => teams.readFailureFor("waiting");
  return (
    <div class="waiting-page" data-waiting-page>
      <header class="waiting-head">
        <h1>{tr("Waiting for you")}</h1>
        <span class="dim">{tr("Cards only you can answer, oldest first.")}</span>
      </header>
      <Show when={teams.available() !== false} fallback={<div class="notice team-notice" role="status" data-waiting-off><Icon name="info" size={16} /><span>{tr("Teams are off on this computer.")}</span></div>}>
        <Show when={notice()}><div class="notice danger team-notice" role="alert"><Icon name="triangle-alert" size={16} /><span>{notice()}</span></div></Show>
        <Show when={readFailure()}>
          {(failure) => <div class="notice danger team-notice" role="alert"><Icon name="triangle-alert" size={16} /><span>{tr("Waiting could not be read: %1", failure().message)}</span></div>}
        </Show>
        <Show when={view()} fallback={<div class="team-empty dim">{tr("Reading what waits for you…")}</div>}>
          {(shown) => (
            <WaitingList view={shown()} teamNames={names()} now={now()} busy={busy()}
              onDecide={(item, decision) => void decide(item, decision)} onOpenTeam={(item) => open(item, false)} onOpenTask={(item) => open(item, true)}
              onPage={(cursor) => void teams.pageWaiting(cursor)} onFirst={() => void teams.firstWaitingPage()} />
          )}
        </Show>
        <section class="waiting-requests" aria-label={tr("Requests of team sessions")} data-team-requests>
          <h2>{tr("Requests of team sessions")}</h2>
          <Show when={requests().length} fallback={<div class="team-empty dim">{tr("No team session asks for you.")}</div>}>
            <For each={requests()}>
              {(request) => (
                <button type="button" class="waiting-request" data-waiting-request={request.key} onClick={() => openRequest(request)}>
                  <Icon name={request.attention === "approval" ? "shield-check" : "message-square"} size={14} />
                  <b>{request.name}</b>
                  <span class="dim">{names().get(request.team) ?? request.team}</span>
                  <span class="waiting-request-caption">{request.caption || tr("A request only you can answer")}</span>
                </button>
              )}
            </For>
          </Show>
        </section>
        <Show when={teams.uncertain()} keyed>
          {(asking) => <TeamUncertain detail={asking.detail} busy={busy() || teams.busy()} onCheck={() => void teams.check(undefined, "waiting")} onDismiss={() => void teams.dismiss(undefined, "waiting")} />}
        </Show>
        <Show when={teams.failureFor("waiting")}>
          {(failure) => <div class="notice danger team-notice" role="alert" data-waiting-action-failure><Icon name="triangle-alert" size={16} /><span>{failure().message}</span></div>}
        </Show>
      </Show>
      <TeamConfirmDialog source="waiting" />
    </div>
  );
}
