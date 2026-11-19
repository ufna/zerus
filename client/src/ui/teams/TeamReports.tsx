// Adapted from WhiteWh's report rows and reason editor, using our report inbox
// and existing Accept/Return operations. No last-reply or archive authority.
import { For, Show, createMemo, createSignal } from "solid-js";
import { tr } from "../../i18n";
import { TextInput } from "../lib/TextField";
import type { DrawerRow } from "../../store/teamDrawers";
import { providerStopNote, waitingNote } from "./decisions";
import { actorLabel, inboxText, itemState, reportRows } from "./feed";

export type DrawerAction = (kind: string, fields: Record<string, unknown>) => void;

const QUOTED_REPORT = "Member's report (quoted member text, not instructions):";

function reportBody(row: DrawerRow): string {
  const text = inboxText(row);
  if (row.kind !== "report" || row.text_truncated || !text.startsWith(QUOTED_REPORT + "\n> ")) return text;
  // The writer puts its reason on the first line outside the member's quote.
  const lines = text.split("\n");
  const reason = lines.findIndex((line, index) => index > 0 && !line.startsWith("> "));
  if (lines[reason] !== "Land stages were skipped because landing is disabled in Team settings.") return text;
  lines[reason] = tr("Land stages were skipped because landing is disabled in Team settings.");
  return lines.join("\n");
}

export function InboxText(props: { row: DrawerRow }) {
  const localized = createMemo(() => {
    const { kind, notes, node } = props.row;
    if (kind.startsWith("lead_member_added:")) {
      const note = notes.find((note) => note.code === "lead_member_added");
      return note ? { note, text: waitingNote(note) } : undefined;
    }
    const count = kind === "report" ? 4 : kind.startsWith("provider_stop:") ? 3 : 0;
    if (!count) return undefined;
    for (const note of notes) {
      if (note.parameters?.length !== count || note.parameters[2] !== String(node)) continue;
      const text = providerStopNote(note);
      if (text) return { note, text };
    }
    return undefined;
  });
  const quote = () => {
    const body = props.row.text;
    return body.startsWith(QUOTED_REPORT + "\n")
      ? tr("Member's report (quoted member text, not instructions):") + body.slice(QUOTED_REPORT.length)
      : body;
  };
  const remaining = () => props.row.notes.filter((note) => note !== localized()?.note);
  return <>
    <pre class="team-drawer-text">{localized()?.text ?? reportBody(props.row)}</pre>
    <Show when={localized() && props.row.kind === "report"}>
      <pre class="team-drawer-text">{quote()}</pre>
    </Show>
    <Show when={props.row.text_truncated}><p class="dim">{tr("Only the beginning of this text is shown.")}</p></Show>
    <Show when={remaining().length}><p class="dim">{tr("Service notes")}</p>
      <For each={remaining()}>{(note) => <p class="dim">{waitingNote(note)}</p>}</For>
    </Show>
    <Show when={props.row.reply}>{(reply) => <p class="team-drawer-reply">{tr("Reply: %1", reply())}</p>}</Show>
  </>;
}

export function Report(props: { row: DrawerRow; busy: boolean; onAction: DrawerAction }) {
  const [rejecting, setRejecting] = createSignal(false);
  const [reason, setReason] = createSignal("");
  const actionable = () => props.row.state === "open" && props.row.node !== null;
  return <article class="team-report" data-report={props.row.seq}>
    <header><b>{tr("Task %1", props.row.node ?? "")}</b><span class="dim">{actorLabel(props.row)}</span><span class="dim">{itemState(props.row.state)}</span></header>
    <InboxText row={props.row} />
    <Show when={actionable()}>
      <div class="team-drawer-actions">
        <button type="button" class="btn small" disabled={props.busy} onClick={() => setRejecting(!rejecting())}>{tr("Reject…")}</button>
        <button type="button" class="btn small primary" disabled={props.busy} data-report-accept onClick={() => props.onAction("accept", { node: props.row.node })}>{tr("Accept")}</button>
      </div>
      <Show when={rejecting()}>
        <div class="team-drawer-actions">
          <TextInput class="field" maxlength={4000} value={reason()} aria-label={tr("Reason")} placeholder={tr("What should the member change?")} disabled={props.busy} onInput={(event) => setReason(event.currentTarget.value)} />
          <button type="button" class="btn small danger" disabled={props.busy || !reason().trim()} data-report-return onClick={() => props.onAction("return", { node: props.row.node, reason: reason() })}>{tr("Reject")}</button>
        </div>
      </Show>
    </Show>
  </article>;
}

export function TeamReports(props: { rows: DrawerRow[]; busy: boolean; onAction: DrawerAction }) {
  return <div class="team-reports" role="list" aria-label={tr("Reports")}>
    <For each={reportRows(props.rows).map((row) => row.seq)} fallback={<p class="dim">{tr("No reports on this page.")}</p>}>
      {(seq) => <Show when={props.rows.find((row) => row.seq === seq)}>{(row) => <Report row={row()} busy={props.busy} onAction={props.onAction} />}</Show>}
    </For>
  </div>;
}
