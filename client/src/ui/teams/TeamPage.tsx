import { Show, createEffect, on, onCleanup, untrack } from "solid-js";
import { tr } from "../../i18n";
import { teams } from "../../store/teams";
import { machineCards } from "../../store/machines";
import { desktopService } from "../../store/desktopService";
import { Icon } from "../lib/icons";
import { stateLabel, stateTone } from "./team-model";
import { TeamBody } from "./TeamBody";
import { TeamConfirmDialog } from "./TeamConfirmDialog";
import { TeamUnresolvedList } from "./TeamTab";
import { ActionFailureNotice } from "./TeamActionFailure";
import { closeTeamPage, takeTeamSettings, teamPage } from "./team-navigation";
import { addTeamTaskFromPage, teamPageAddTaskBlocked } from "./team-page-tasks";
import "../styles/team-page.css";

/** The team page (zerus-63rru.198): the team the page names, followed by its
 * ID alone, with the same workspace the lead session's tab shows. It needs no
 * session; only Lead chat stays with the lead's own session. */
export function TeamPage() {
  const target = () => teamPage()?.team;
  const source = () => `team-page:${target() ?? ""}`;
  // One followed team at a time: the page follows its team and lets go when
  // it closes (pages are exclusive, so the lead tab never competes).
  createEffect(on(target, (id) => { if (id) void teams.openTeam(id).catch(() => {}); }));
  onCleanup(() => {
    const id = untrack(target);
    if (id && teams.confirmation()?.call.team === id) teams.cancelConfirmation();
    teams.clearFailure(untrack(source));
    void teams.closeTeam().catch(() => {});
  });
  const summary = () => teams.listed().find((row) => row.id === target());
  const view = () => { const shown = teams.team(); return shown && shown.bar.id === target() ? shown : undefined; };
  const name = () => view()?.bar.name ?? summary()?.name ?? "";
  const status = () => view()?.bar.status ?? summary()?.status;
  const readFailure = () => target() ? teams.readFailureFor(`team:${target()}`) : null;
  // A team whose home is another computer, or a window without its desktop
  // service, is shown read-only with one banner instead of a refusal per press.
  const home = () => {
    const shown = summary()?.home, here = machineCards[""]?.machine_id;
    if (!shown || !here || shown === here) return undefined;
    const found = Object.entries(machineCards).find(([, card]) => card.machine_id === shown);
    return tr("This team's home is another computer (%1). Open it there.", found ? found[0] || found[1].host || shown : shown);
  };
  const service = () => desktopService.enabled() && !desktopService.state()?.connected
    ? tr("Connect this window to its desktop service before changing a team.") : undefined;
  // An archived team opens by its ID like any other (the #2558 review), read-only.
  const archived = () => status() === "archived" ? tr("This team is archived. It is shown read-only.") : undefined;
  const readOnly = () => archived() ?? home() ?? service();
  return <section class="team-page" data-team-page aria-label={tr("Team")}>
    <header class="team-page-head">
      <button type="button" class="btn ghost small" data-team-page-back onClick={closeTeamPage}><Icon name="chevron-left" size={14} />{tr("Back")}</button>
      <Icon name="users" size={18} />
      <h1 class="ell">{name()}</h1>
      <Show when={status()}>{(shown) => <span class={`tm-state-badge tone-${stateTone(shown())}`}>{stateLabel(shown())}</span>}</Show>
    </header>
    <Show when={teams.available() !== false} fallback={<p class="notice team-notice" role="status">{teams.off() === "unavailable" ? tr("Teams are unavailable on this computer.") : tr("Teams are off on this computer.")}</p>}>
      <Show when={archived()}>{(reason) => <p class="notice team-notice" role="status" data-team-page-archived>{reason()}</p>}</Show>
      <Show when={home()}>{(reason) => <p class="notice team-notice" role="status" data-team-page-home>{reason()}</p>}</Show>
      <Show when={service()}>{(reason) => <p class="notice team-notice" role="status" data-team-page-service>{reason()}</p>}</Show>
      <Show when={view()} fallback={<div class="team-loading" role="status" data-team-waiting>
        <span class="dim">{readFailure() ? tr("Teams read failed: %1", readFailure()!.message) : tr("Reading the team…")}</span>
      </div>}>{(current) => <TeamBody view={current()} source={source()} openSettings={untrack(() => takeTeamSettings(current().bar.id))}
        readOnly={readOnly()} addTaskBlocked={readOnly() ?? teamPageAddTaskBlocked(current().bar)}
        onAddTask={() => addTeamTaskFromPage(current().bar)} />}</Show>
      <Show when={!view() && teams.failureFor(source())}>{(reason) => <ActionFailureNotice failure={reason()} onDismiss={() => teams.clearFailure(source())} />}</Show>
      <TeamUnresolvedList entries={teams.unresolved()} busy={teams.busy()} onCheck={(entry) => void teams.check(entry, source())} onDismiss={(entry) => void teams.dismiss(entry, source())} />
      <TeamConfirmDialog source={source()} />
    </Show>
  </section>;
}
