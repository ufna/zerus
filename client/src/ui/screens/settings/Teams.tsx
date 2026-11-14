import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { withoutCode } from "../../../store/sentences";
import { tr, trNoop } from "../../../i18n";
import { teams } from "../../../store/teams";
import { Group, Row, SectionHead, Switch } from "./ui";
import { Icon } from "../../lib/icons";

// Settings > Agent Teams (preview): one row. Its state comes from the
// application's own discovery, never from a poll of its own; a change runs the
// person's `hgs team preview on|off` through the native command and then asks
// discovery again, so the entry points appear or disappear without a restart.
// Turning it off asks first unless the list is complete and shows no live
// team: the teams stop being served, nothing is deleted. A list that is not
// read yet, goes on past its first page or was cut short asks too. A refusal keeps its reason in the row; an unknown result
// moves nothing on screen and offers a way to read the state again.

/** The statuses the service still serves: a team live right now. */
const LIVE = new Set(["planning", "running", "paused"]);

export type PreviewState = "on" | "off" | "unavailable" | "unknown";

/** The row's state, from the discovery the application already followed. */
export function previewState(available: boolean | undefined, off: "inactive" | "unavailable" | null): PreviewState {
  if (available === true) return "on";
  if (off === "inactive") return "off";
  if (off === "unavailable") return "unavailable";
  return "unknown";
}

/** The confirmation of turning the preview off: Cancel is the default. */
function PreviewOffDialog(props: { onCancel: () => void; onConfirm: () => void }) {
  let dialog: HTMLDialogElement | undefined;
  const origin = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
  onMount(() => { dialog?.showModal(); queueMicrotask(() => dialog?.querySelector<HTMLElement>("[data-cancel]")?.focus()); });
  onCleanup(() => { if (dialog?.open) dialog.close(); if (origin?.isConnected) origin.focus(); });
  return (
    <dialog ref={dialog} class="dlg" aria-labelledby="teams-off-title" aria-describedby="teams-off-text"
      onCancel={(event) => { event.preventDefault(); props.onCancel(); }}>
      <div class="dlg-head"><Icon name="triangle-alert" size={18} class="warn" /><h2 id="teams-off-title">{tr("Turn Agent Teams off?")}</h2></div>
      <div class="dlg-body"><p id="teams-off-text">{tr("Teams on this computer stop being served. Nothing is deleted.")}</p></div>
      <div class="dlg-foot">
        <span class="spacer" />
        <button type="button" class="btn solid-danger" onClick={props.onConfirm}>{tr("Turn off")}</button>
        <button type="button" class="btn primary" data-cancel autofocus onClick={props.onCancel}>{tr("Cancel")}</button>
      </div>
    </dialog>
  );
}

export function TeamsSection() {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [uncertain, setUncertain] = createSignal("");
  const [confirming, setConfirming] = createSignal(false);
  const state = () => previewState(teams.available(), teams.off());
  /** Whether turning off needs the confirmation: unless the first page is the
   * whole list and shows no live team. */
  const mayHaveLive = () => {
    const list = teams.list();
    return !list || !!list.next || list.truncated || list.teams.some((team) => LIVE.has(team.status));
  };
  const disabled = () => busy() || state() === "unavailable" || state() === "unknown";
  const apply = async (on: boolean) => {
    setBusy(true); setError(""); setUncertain("");
    try {
      const outcome = await teams.preview(on);
      if (outcome.outcome === "failed") setError(outcome.detail);
      else if (outcome.outcome === "uncertain") setUncertain(outcome.detail);
    } finally { setBusy(false); }
  };
  const change = (on: boolean) => {
    if (on === (state() === "on") || disabled()) return;
    if (!on && mayHaveLive()) { setConfirming(true); return; }
    void apply(on);
  };
  return (
    <>
      <SectionHead title={trNoop("Agent Teams")} />
      <Group>
        <Row label={trNoop("Agent Teams (preview)")} for="set-teams-preview" disabled={disabled()}
          hint={trNoop("Applies to this computer. Nothing runs until a team is created.")}>
          <Switch id="set-teams-preview" checked={state() === "on"} disabled={disabled()}
            label={trNoop("Agent Teams (preview)")} onChange={change} />
        </Row>
        <Show when={state() === "unavailable"}>
          <p class="set-group-hint" role="note" data-teams-unavailable>{teams.readFailureFor("list")?.message ?? tr("hgs is unavailable on this computer, so the preview cannot be changed.")}</p>
          <button type="button" class="btn small" disabled={busy()} onClick={() => void teams.recheck()}>{tr("Read again")}</button>
        </Show>
        <Show when={state() === "unknown"}>
          <p class="set-group-hint" role="status" data-teams-unknown>
            {tr("The preview state is unknown.")}{" "}
            <button type="button" class="btn small" disabled={busy()} onClick={() => { void teams.recheck(); }}>{tr("Read again")}</button>
          </p>
        </Show>
        <Show when={error()}>
          <p class="set-group-hint" role="alert" data-teams-error>{withoutCode(error())}</p>
        </Show>
        <Show when={uncertain()}>
          <p class="set-group-hint" role="status" data-teams-uncertain>
            {tr("The change could not be confirmed. Nothing on screen changed.")}{" "}
            <button type="button" class="btn small" disabled={busy()} onClick={() => { setUncertain(""); void teams.recheck(); }}>{tr("Read the state again")}</button>
          </p>
        </Show>
      </Group>
      <Show when={confirming()}>
        <PreviewOffDialog onCancel={() => setConfirming(false)} onConfirm={() => { setConfirming(false); void apply(false); }} />
      </Show>
    </>
  );
}
