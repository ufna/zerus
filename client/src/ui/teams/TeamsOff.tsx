import { Show } from "solid-js";
import { tr } from "../../i18n";
import { teams } from "../../store/teams";

/** Settings > Agent Teams, loaded on demand like the window's other links there. */
const openTeamsSettings = () => void import("../screens/settings/sections").then(({ openSettings }) => openSettings("teams"));

/** Why a Teams page is empty when Teams are off, and where to turn them on
 * (zerus-63rru.197 C9). A computer whose Teams are unavailable gets no link:
 * Settings cannot turn them on there. */
export function TeamsOffNotice(props: { text?: string }) {
  const unavailable = () => teams.off() === "unavailable";
  return <>
    <span>{unavailable() ? tr("Teams are unavailable on this computer.") : props.text ?? tr("Teams are off on this computer.")}</span>
    <Show when={!unavailable()}>
      <button type="button" class="btn small" data-teams-turn-on onClick={openTeamsSettings}>{tr("Turn on in Settings")}</button>
    </Show>
  </>;
}
