import { Show, onCleanup } from "solid-js";
import { tr } from "../../i18n";
import { menu, setMenu, type MenuEntry } from "../../store/ui";
import { orderedRows } from "../../store/catalog";
import { newRequestId, teams, type TeamCall, type TeamSummary } from "../../store/teams";
import { Icon } from "../lib/icons";
import { TeamsScreen } from "../screens/Teams";
import { openTeamLead } from "./lead-navigation";
import { rowIsSession } from "./session-binding";
import { openTeamPage } from "./team-navigation";
import { TeamConfirmDialog } from "./TeamConfirmDialog";
import { ActionFailureNotice } from "./TeamActionFailure";
import { activeTeam } from "./actions";
import "../styles/team-page.css";

// Each Teams row's menu (zerus-63rru.198): the team's page, its lead's
// session, Settings, and the lifecycle actions the bar offers for its status.
// A lifecycle action is sent from the row with the list's revision through
// the same native confirmation; a stale revision is refused, and Review again
// reads the list again (no session is needed anywhere).

/** The failure key of a row's own actions. */
export const rowSource = (team: string) => `teams-row:${team}`;

interface RowAction { id: "pause" | "resume" | "stop" | "archive"; text: string; danger?: boolean }
/** The bar's lifecycle actions (actions.ts barActions) for a list row. A
 * planning team's Pause needs its planning state, which only the page reads. */
export function rowActions(status: string): RowAction[] {
  const actions: RowAction[] = [];
  if (status === "running") actions.push({ id: "pause", text: tr("Pause") });
  if (status === "paused") actions.push({ id: "resume", text: tr("Resume") });
  if (activeTeam(status) || status === "planning") actions.push({ id: "stop", text: tr("Stop…"), danger: true });
  if (status === "stopped" || status === "finished") actions.push({ id: "archive", text: tr("Archive…"), danger: true });
  return actions;
}

/** One lifecycle intent from the row: its ID, name and the list's revision. */
export function rowCall(team: TeamSummary, kind: RowAction["id"]): TeamCall {
  return { team: team.id, team_name: team.name, expected_revision: team.revision, request_id: newRequestId(), action: { kind } };
}

export function TeamRowMenu(props: { team: TeamSummary }) {
  let items: MenuEntry[] | undefined;
  onCleanup(() => { if (items && menu()?.items === items) setMenu(undefined); });
  return <button type="button" class="icon-btn small teams-row-more" data-team-menu={props.team.id} aria-haspopup="menu"
    aria-label={tr("Team actions: %1", props.team.name)} title={tr("Team actions")}
    onClick={(event) => {
      const trigger = event.currentTarget, box = trigger.getBoundingClientRect(), team = props.team;
      const busy = teams.busy(team.id), lead = team.lead_session;
      const row = lead ? orderedRows().find((candidate) => rowIsSession(candidate, lead)) : undefined;
      const leadWhy = !lead ? tr("The team's lead has no session yet.") : !row ? tr("The team's lead session is not on the session list.") : undefined;
      const settingsWhy = activeTeam(team.status) ? undefined : tr("Settings are available while the team runs or is paused.");
      items = [
        { label: tr("Open"), icon: "external-link", action: () => openTeamPage(team.id) },
        { label: tr("Open lead session"), icon: "terminal", disabled: !row, note: leadWhy, hint: leadWhy,
          action: () => { if (row) openTeamLead(team, row); } },
        { label: tr("Settings…"), icon: "settings", disabled: !!settingsWhy, note: settingsWhy, hint: settingsWhy,
          action: () => { if (!settingsWhy) openTeamPage(team.id, { settings: true }); } },
        // Pausing planning needs its planning state, which only the page reads.
        ...(team.status === "planning" ? [{ label: tr("Pause or resume planning…"), icon: "pause", action: () => openTeamPage(team.id) } satisfies MenuEntry] : []),
        ...rowActions(team.status).map((action): MenuEntry => ({
          label: action.text, icon: action.id === "pause" ? "pause" : action.id === "resume" ? "play" : action.id === "stop" ? "square" : "archive",
          danger: action.danger, disabled: busy,
          action: () => { if (busy) return; trigger.focus(); void teams.run(rowCall(team, action.id), rowSource(team.id)); },
        })),
      ];
      setMenu({ x: box.right - 220, y: box.bottom + 4, items });
    }}><Icon name="ellipsis" size={14} /></button>;
}

/** A row action's refusal under its row; a stale revision offers Review again. */
export function TeamRowNotice(props: { team: TeamSummary }) {
  const failure = () => teams.failureFor(rowSource(props.team.id));
  const review = () => {
    teams.clearFailure(rowSource(props.team.id));
    void teams.page("list", (teams.listPage() ?? teams.list())?.after ?? null).catch(() => {});
  };
  return <Show when={failure()}>{(shown) => <div class="teams-row-notice" data-team-row-notice={props.team.id}>
    <ActionFailureNotice failure={shown()} onDismiss={() => teams.clearFailure(rowSource(props.team.id))} />
    <Show when={shown().code === "stale_revision"}>
      <button type="button" class="btn small" data-team-review-again onClick={review}><Icon name="refresh-cw" size={12} />{tr("Review again")}</button>
    </Show>
  </div>}</Show>;
}

/** The Teams page as the app shows it: the list with each row's menu and
 * notice, and the one confirmation its rows' destructive actions use. */
export function TeamsListPage() {
  const asking = () => teams.confirmation();
  return <>
    <TeamsScreen rowActions={(team) => <TeamRowMenu team={team} />} rowDetails={(team) => <TeamRowNotice team={team} />} />
    <TeamConfirmDialog source={asking() ? rowSource(asking()!.call.team) : undefined} />
  </>;
}
