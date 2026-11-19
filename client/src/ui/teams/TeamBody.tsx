import type { JSX } from "solid-js";
import { tr } from "../../i18n";
import { newRequestId, teams } from "../../store/teams";
import { teamCall } from "./actions";
import type { EditAction } from "./TeamEditors";
import { TeamWorkspace } from "./TeamTab";
import type { TeamView } from "./types";

/** The team's workspace as the lead session's tab and the team page both show
 * it (zerus-63rru.198): the same reads, keyed by the team ID, and the same
 * calls, `teamCall(view)` through `teams.run` with the asking view's source.
 * Nothing here needs a session. */
export function TeamBody(props: {
  view: TeamView;
  /** The asking view's failure key: `team-tab:…` or `team-page:<id>`. */
  source: string;
  onAddTask: () => void;
  /** Why the team cannot be changed here (another home computer); shown read-only. */
  readOnly?: string;
  /** Why Add task cannot run here, beside its disabled button. */
  addTaskBlocked?: string;
  barExtra?: JSX.Element;
  openSettings?: boolean;
}) {
  const id = () => props.view.bar.id;
  const act: EditAction = (kind: string, fields: Record<string, unknown> = {}, revision?: number, team?: string) => {
    const current = props.view;
    if (team !== undefined && team !== current.bar.id) return {
      outcome: "failed", request_id: "", detail: tr("The selected team changed. Reopen the form."),
    };
    if (props.readOnly) return { outcome: "failed", request_id: "", detail: props.readOnly };
    const call = teamCall(current, newRequestId(), kind, fields);
    if (revision !== undefined) call.expected_revision = revision;
    return teams.run(call, props.source);
  };
  return <TeamWorkspace view={props.view} busy={!!props.readOnly || teams.busy(id())} failure={teams.readFailureFor(`team:${id()}`)}
    actionFailure={teams.failureFor(props.source)} onDismissFailure={() => teams.clearFailure(props.source)}
    providerHome={teams.listed().find((row) => row.id === id() && row.project === props.view.bar.project)?.home}
    onSettingsSave={(call) => teams.run(call, props.source)}
    settingsBlocked={teams.unresolved().some((entry) => entry.call.team === id() && ["settings", "validation_config"].includes(entry.call.action.kind))}
    placementUnresolved={(node) => teams.unresolved().some(({ call }) => call.team === id()
      && call.action.node === node && (call.action.kind === "reassign" || call.action.kind === "route"))}
    onAction={act} onPage={(cursor) => void teams.page(`team:${id()}`, cursor)} onAddTask={props.onAddTask}
    addTaskBlocked={props.addTaskBlocked} barExtra={props.barExtra} openSettings={props.openSettings} />;
}
