import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";
import { actionNeeded } from "../../store/attention";
import { orderedRows } from "../../store/catalog";
import { now, setDetailTab, setPage, setSelectedKey } from "../../store/ui";
import { teams, type WaitingItem } from "../../store/teams";
import { WaitingList } from "../teams/Waiting";
import { TeamConfirmDialog } from "../teams/TeamConfirmDialog";
import { TeamUncertain } from "../teams/TeamTab";
import { sessionIdentity } from "./tasks-ui";
import { teamRequests, type RequestRow, type TeamRequest } from "./waiting-requests";
import "../styles/teams.css";

// Waiting for you: the open owner cards of every team (the `waiting` key, one
// bounded page at a time) and, from the desktop's own session attention data,
// the requests of team sessions that only a person can answer. A card is
// decided in place through the Teams store; a session request opens the
// session. With the preview off the rail entry and this page do not exist.

const identity = (value: { session_id?: string | null; run_id?: string | null; sessionId?: string | null; runId?: string | null } | null | undefined) => {
  if (!value) return "";
  if ("session_id" in value && value.session_id) return value.session_id;
  if ("run_id" in value && value.run_id) return `run_id:${value.run_id}`;
  if ("sessionId" in value && value.sessionId) return `launch_id:${value.sessionId}`;
  if ("runId" in value && value.runId) return `run_id:${value.runId}`;
  return "";
};

/** The team of each lead session, and of the sessions that nest under one. */
function teamsBySession() {
  const byLead = new Map<string, { team: string; name: string }>();
  for (const summary of teams.list()?.teams ?? []) {
    const id = identity(summary.lead_session);
    if (id) byLead.set(id, { team: summary.id, name: summary.name });
  }
  return byLead;
}

export function WaitingScreen() {
  const [busy, setBusy] = createSignal(false);
  const [notice, setNotice] = createSignal("");
  onMount(() => { void teams.probe(); void teams.openWaiting(); });
  const view = () => teams.waitingView();
  const names = createMemo(() => new Map((teams.list()?.teams ?? []).map((team) => [team.id, team.name])));
  const leads = createMemo(() => teamsBySession());
  const requests = createMemo<TeamRequest[]>(() => {
    const rows: RequestRow[] = orderedRows().map((row) => ({
      key: row.key, name: row.name, attention: row.attention, caption: row.caption,
      actionNeeded: actionNeeded(row), identity: sessionIdentity(row), parentIdentity: identity(row.parent),
    }));
    return teamRequests(rows, leads());
  });
  const openTeam = (item: WaitingItem) => {
    const summary = teams.list()?.teams.find((candidate) => candidate.id === item.team);
    const row = summary ? orderedRows().find((candidate) => identity(candidate) === identity(summary.lead_session)) : undefined;
    if (!row) { setNotice(tr("The team's lead session is not on the session list.")); return; }
    setPage("sessions"); setSelectedKey(row.key); setDetailTab("team");
  };
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
  const openRequest = (request: TeamRequest) => {
    setPage("sessions"); setSelectedKey(request.key); setDetailTab("activity");
  };
  return (
    <div class="waiting-page" data-waiting-page>
      <header class="waiting-head">
        <h1>{tr("Waiting for you")}</h1>
        <span class="dim">{tr("Cards only you can answer, oldest first.")}</span>
      </header>
      <Show when={notice()}><div class="team-notice" role="alert">{notice()}</div></Show>
      <Show when={teams.failure()}>
        {(failure) => (
          <Show when={failure().key === "waiting"}>
            <div class="team-notice" role="alert">{tr("Waiting could not be read: %1", failure().message)}</div>
          </Show>
        )}
      </Show>
      <Show when={view()} fallback={<div class="team-empty dim">{tr("Reading what waits for you…")}</div>}>
        {(shown) => (
          <WaitingList view={shown()} teamNames={names()} now={now()} busy={busy()}
            onDecide={(item, decision) => void decide(item, decision)} onOpenTeam={openTeam}
            onPage={(cursor) => void teams.pageWaiting(cursor)} />
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
        {(asking) => <TeamUncertain detail={asking.detail} busy={busy()} onCheck={() => void teams.check()} onDismiss={() => void teams.dismiss()} />}
      </Show>
      <TeamConfirmDialog />
    </div>
  );
}
