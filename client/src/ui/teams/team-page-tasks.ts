import { tr } from "../../i18n";
import { orderedRows } from "../../store/catalog";
import { groupOf } from "../../store/organization";
import { openTasks } from "../../store/taskNavigation";
import { setTeamTaskCreation, type TeamTaskSource } from "../../store/taskCreation";
import { teams, type TeamBar } from "../../store/teams";
import { activeTeam } from "./actions";

/** Why the team page (zerus-63rru.198) cannot add a task: the form is its
 * project's New task form in Sessions, which follows a session of that project. */
export function teamPageAddTaskBlocked(team: TeamBar): string | undefined {
  return orderedRows().some((row) => row.state !== "archived" && groupOf(row.key) === team.project) ? undefined
    : tr("This team's project has no session on the session list to add a task from.");
}

/** Add task on the team page: the Sessions tasks panel on the team's
 * project, then its New task form with this team chosen (TaskCreate takes
 * the request once the form's project matches). */
export function addTeamTaskFromPage(view: TeamTaskSource) {
  const team = view.bar;
  if (teams.available() !== true || !activeTeam(team.status) || teamPageAddTaskBlocked(team)) return;
  openTasks(team.project);
  setTeamTaskCreation({ project: team.project, team: team.id, fromPage: { bar: team, routing_pipelines: view.routing_pipelines } });
}
