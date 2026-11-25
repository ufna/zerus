import { Show, batch, createMemo } from "solid-js";
import { tr } from "../../i18n";
import { updating } from "../../store/actions";
import { archive } from "../../store/archive";
import { running as batchRunning } from "../../store/batch";
import { catalog, computerOffline } from "../../store/catalog";
import { isRestorePending } from "../../store/restores";
import {
  setAccountFilter, setFolderFilter, setMachineFilter, setPage, setProjectFilter,
  setSearch, setSelectedKey, setStateFilter, setSubagentOpen,
} from "../../store/ui";
import { archiveNameOccupied, menuState } from "../sessions/actions-ui";
import type { Session } from "../sessions/types";
import { memberActions } from "./actions";
import { isRunningMemberSession, openMemberSession } from "./member-session";
import { rowIsSession } from "./session-binding";
import type { MemberView } from "./types";

function browseArchive() {
  batch(() => {
    setSelectedKey(undefined);
    setSubagentOpen(undefined);
    setSearch("");
    setMachineFilter([]);
    setProjectFilter([]);
    setAccountFilter([]);
    setFolderFilter(undefined);
    setStateFilter("archived");
    setPage("sessions");
  });
}

export function TeamLeadAvailability(props: {
  lead: MemberView | undefined;
  row: Session | undefined;
  busy: boolean;
  readOnly?: string;
  pending: boolean;
  onAction: (kind: "stop_member" | "swap_member", member: string) => void;
}) {
  const current = () => props.row?.state === "archived" ? undefined : props.row;
  const launchOnly = () => props.lead?.session?.session_id.startsWith("launch_id:");
  const archived = createMemo(() => {
    const binding = props.lead?.session;
    if (!binding?.session_id.startsWith("run_id:") || current()) return undefined;
    // A loaded page can hide other runs of the same launch. Only a run-bound
    // seat can identify an archived row and its recovery capabilities here.
    const candidates = archive.rows().filter((row) => !!row.archiveId && row.state === "archived" && rowIsSession(row, binding));
    const shown = props.row;
    if (shown?.archiveId && shown.state === "archived" && rowIsSession(shown, binding)
      && !candidates.some((row) => row.key === shown.key)) return candidates.length ? undefined : shown;
    return candidates.length === 1 ? candidates[0] : undefined;
  });
  const state = () => isRunningMemberSession(current()) ? "live" : archived() ? "archived" : "unavailable";
  const status = () => state() === "live" ? tr("The lead's session is live.")
    : state() === "archived" ? tr("The lead's session is archived.")
    : !props.lead?.session ? tr("The team's lead has no session yet.")
    : current() ? tr("The lead's session is unavailable for live work.")
    : tr("The lead's session is missing from the current list.");
  const recovery = createMemo(() => {
    const lead = props.lead;
    if (state() === "live" || !lead?.session) return undefined;
    if (props.pending) return { text: tr("Use Check on the pending lead action before changing the seat.") };
    const row = archived();
    if (!row) return { text: current() ? tr("Open the lead's session to check its available actions.")
      : launchOnly() ? tr("Browse Archive and verify the lead's run. A launch can have more than one archived run.")
      : tr("Browse Archive to see if the lead's session can be restored.") };
    if (row.restorePending || isRestorePending(row)) return { text: tr("Check the pending restore in the archived session.") };
    if (computerOffline(row.computer)) return { text: tr("Machine is offline") };
    if (updating(row.computer, row.name) || batchRunning()) return { text: tr("Wait for the session update to finish before checking Restore session.") };
    if (row.allowed === undefined || archiveNameOccupied(row, catalog.rows)) {
      return { text: tr("Check the available recovery actions in the archived session.") };
    }
    if (menuState(row, true, false).change.enabled) {
      return { text: tr("Open the archived session and choose Restore session to try to bring back the lead.") };
    }

    const why = row.restoreReason;
    const detail = why ? tr("Restore is unavailable: %1", tr(why)) : tr("Restore is unavailable for this archive.");
    if (props.readOnly) return { detail, text: props.readOnly };
    if (lead.open_work === undefined || lead.open_work === null) {
      return { detail, text: tr("Refresh the team to check the lead's open work before replacing the seat.") };
    }
    if (lead.open_work > 0) return { detail, text: tr("The lead has open or unsettled work (%1). Swap requires that work to settle.", lead.open_work) };
    const actions = memberActions(lead, "running");
    if (actions.some((action) => action.id === "swap_member" && !action.disabled)) {
      return { detail, text: tr("Review Swap for this lead seat. Zerus checks the saved launch and delivery receipts."),
        kind: "swap_member" as const, label: tr("Review Swap") };
    }
    if (actions.some((action) => action.id === "stop_member" && !action.disabled)) {
      return { detail, text: tr("Stop the lead seat before reviewing Swap. Zerus checks the saved receipts before any replacement."),
        kind: "stop_member" as const, label: tr("Review Stop member") };
    }
    return { detail, text: tr("A replacement requires a person-stopped lead seat and settled work and receipts.") };
  });
  const open = () => {
    const row = archived();
    if (row) batch(() => { setStateFilter("archived"); openMemberSession(row); });
    else if (current()) openMemberSession(current()!);
    else browseArchive();
  };
  return <div class="notice team-notice team-lead-availability" data-team-lead-availability={state()}>
    <div role="status">
      <b>{status()}</b>
      <Show when={recovery()}>{(step) => <div>
        <Show when={step().detail}>{(detail) => <div>{detail()}</div>}</Show>
        <div>{step().text}</div>
      </div>}</Show>
    </div>
    <Show when={state() !== "live" && props.lead?.session}>
      <button type="button" class="btn small" data-team-lead-open onClick={open}>
        {archived() ? tr("Open archived lead session") : current() ? tr("Open lead session") : tr("Browse Archive")}
      </button>
    </Show>
    <Show when={recovery()?.kind}>{(kind) =>
      <button type="button" class="btn small" data-team-lead-review={kind()} disabled={props.busy}
        onClick={() => { const lead = props.lead, step = recovery(); if (lead && step?.kind && !props.busy) props.onAction(step.kind, lead.id); }}>
        {recovery()?.label}
      </button>
    }</Show>
  </div>;
}
