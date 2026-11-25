import { tr } from "../i18n";
import { notify } from "./actions";
import { openInspector } from "./inspector";
import { tasks } from "./tasks";
import { teams, type TeamBar } from "./teams";
import { sendToTeam, writingTask } from "./taskBoard";
import { draggedTask, readTaskDrag, setDraggedTask, TASK_MIME } from "./taskAssignment";
import { setTeamTaskCreation } from "./taskCreation";

/** Why Add task cannot open the New task form for this team now: the store
 * takes tasks only on a running or paused team, and the task panel must show
 * the team's project (zerus-63rru.199 A6). The button and the request share it. */
export function teamTaskBlocked(team: Pick<TeamBar, "project" | "status">, panelProject: string | undefined): string | undefined {
  if (!["running", "paused"].includes(team.status)) return tr("Only a running or paused team takes new tasks.");
  if (panelProject !== team.project) return tr("The task panel shows another project. Open a session of %1 to add a task to this team.", team.project);
  return undefined;
}

export function requestTeamTask(team: TeamBar) {
  const blocked = teamTaskBlocked(team, tasks.project());
  if (teams.available() !== true || blocked) {
    notify("error", blocked ?? tr("The chosen team is no longer offered for this project. Nothing was sent.")); return;
  }
  setTeamTaskCreation({ project: team.project, team: team.id });
  openInspector("tasks");
}

/** Only a current local drag in this project's writable queue can be offered. */
export function canDropTeamTask(team: TeamBar) {
  const drag = draggedTask(), task = drag && tasks.find(drag.uuid);
  return teams.available() === true && tasks.project() === team.project && drag?.project === team.project
    && ["running", "paused"].includes(team.status) && !writingTask()
    && !!task?.writable && ["backlog", "todo"].includes(task.stage);
}
export function teamTaskDragOver(event: DragEvent, team: TeamBar) {
  if (!event.dataTransfer?.types.includes(TASK_MIME)) return;
  event.stopPropagation();
  const allowed = canDropTeamTask(team);
  event.dataTransfer.dropEffect = allowed ? "move" : "none";
  if (allowed) event.preventDefault();
}
export function teamTaskDrop(event: DragEvent, team: TeamBar) {
  if (!event.dataTransfer?.types.includes(TASK_MIME)) return;
  event.preventDefault(); event.stopPropagation();
  const task = readTaskDrag(event, team.project); setDraggedTask(undefined);
  if (task && teams.available() === true) void sendToTeam(team.project, task, team.id, team.name);
}

/** Pointer dragging uses the same followed team as the HTML drop target. */
export function teamTaskAt(element: Element | null, project: string) {
  const team = teams.team()?.bar;
  return teams.available() === true && team?.project === project && tasks.project() === project
    && element?.closest<HTMLElement>("[data-team-task-drop]")?.dataset.teamTaskDrop === team.id ? team : undefined;
}
