// Adapted from WhiteWh's CatalogPage; this page edits our catalog only.
import { Show } from "solid-js";
import { tr } from "../../i18n";
import { closeTeamCatalog } from "./catalog-navigation";
import { teams } from "../../store/teams";
import { TeamCatalogEditor } from "./TeamCatalogEditor";
import { TeamsOffNotice } from "./TeamsOff";
export function CatalogPage() {
  return <div class="catalog-page"><header class="nt-page-head"><h1>{tr("Team catalog")}</h1><button class="btn ghost small" onClick={closeTeamCatalog}>{tr("Back")}</button></header><Show when={teams.available() === true} fallback={<p class="notice team-notice" role="status"><Show when={teams.available() === false} fallback={tr("Teams preview is off.")}><TeamsOffNotice text={tr("Teams preview is off.")} /></Show></p>}><TeamCatalogEditor /></Show></div>;
}
