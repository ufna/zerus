import { page } from "../../store/ui";
import { openTask, openTasks } from "../../store/taskNavigation";

/** A team task's Open task: the Sessions Task View, switching there from the
 * team page (zerus-63rru.198); `openTask` alone does not change the page. */
export function openTeamTask(project: string, uuid: string) {
  if (page() === "team") openTasks(project, undefined, uuid);
  else openTask(project, uuid);
}
