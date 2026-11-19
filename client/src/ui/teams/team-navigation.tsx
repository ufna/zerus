import { batch, createSignal } from "solid-js";
import { tr } from "../../i18n";
import { page, setPage, type Page } from "../../store/ui";
import { openTask, openTasks } from "../../store/taskNavigation";

// The team page (zerus-63rru.198): a team opened by its ID alone, with or
// without its lead's session. One page and one return destination, as the
// catalog's Back keeps (catalog-navigation.ts); never a history or a cache.

export interface TeamPageTarget {
  team: string;
  /** Open the team's Settings once its view is shown (the row menu's Settings…). */
  settings?: boolean;
}
const [teamPage, setTeamPage] = createSignal<TeamPageTarget>();
export { teamPage };
let origin: Page | undefined;

export function openTeamPage(team: string, options: { settings?: boolean } = {}) {
  const from = page();
  if (from !== "team") origin = from;
  batch(() => { setTeamPage({ team, settings: options.settings }); setPage("team"); });
}

/** Back: where the page was opened (Teams, Waiting or the lead session). */
export function closeTeamPage() {
  const back = origin ?? "teams";
  origin = undefined;
  setPage(back === "team-catalog" ? "teams" : back);
}

/** The Settings request is taken once, by the team it names. */
export function takeTeamSettings(team: string): boolean {
  const target = teamPage();
  if (target?.team !== team || !target.settings) return false;
  setTeamPage({ team });
  return true;
}

/** A team task's Open task: the Sessions Task View, switching there from the
 * team page (`openTask` alone does not change the page). */
export function openTeamTask(project: string, uuid: string) {
  if (page() === "team") openTasks(project, undefined, uuid);
  else openTask(project, uuid);
}

/** The lead session's Team tab keeps the workspace and links to the page. */
export function OpenTeamPageButton(props: { team: string }) {
  return <button type="button" class="btn ghost small" data-open-team-page onClick={() => openTeamPage(props.team)}>{tr("Open team page")}</button>;
}
