// Adapted from WhiteWh's report rows, using our report inbox. A report is
// decided in the task's inspector, beside its evidence and with the gates of
// its decisions (zerus-63rru.197 A4); the row opens that task.
import { For, Show, createMemo } from "solid-js";
import { tr } from "../../i18n";
import type { DrawerRow } from "../../store/teamDrawers";
import { providerStopNote, waitingNote } from "./decisions";
import { actorLabel, inboxText, itemState, reportRows } from "./feed";
import { setOpenTeamNode } from "./openNode";

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

/** Where a report row leads: the task's inspector on the board. */
export interface ReportTarget { team?: string; onOpenNode?: () => void }

export function Report(props: { row: DrawerRow; busy: boolean } & ReportTarget) {
  const blocked = () => props.row.notes.some((note) => note.code === "provider_stop_provider_policy" && note.parameters?.[2] === String(props.row.node));
  const node = () => props.row.state === "open" && props.team ? props.row.node : null;
  return <article class="team-report" data-report={props.row.seq}>
    <header><b>{tr("Task %1", props.row.node ?? "")}</b><span class="dim">{actorLabel(props.row)}</span><span class="dim">{itemState(props.row.state)}</span></header>
    <InboxText row={props.row} />
    <Show when={node()}>{(task) =>
      <div class="team-drawer-actions">
        <span class="dim">{blocked() ? tr("Open the task to see the earlier report and its available decisions.")
          : tr("Accept or return this report from the task, beside its verdicts.")}</span>
        <button type="button" class="btn small primary" disabled={props.busy} data-report-open
          onClick={() => { setOpenTeamNode({ team: props.team!, node: task() }); props.onOpenNode?.(); }}>{tr("Open task")}</button>
      </div>
    }</Show>
  </article>;
}

export function TeamReports(props: { rows: DrawerRow[]; busy: boolean; onAction: DrawerAction } & ReportTarget) {
  return <div class="team-reports" role="list" aria-label={tr("Reports")}>
    <For each={reportRows(props.rows).map((row) => row.seq)} fallback={<p class="dim">{tr("No reports on this page.")}</p>}>
      {(seq) => <Show when={props.rows.find((row) => row.seq === seq)}>{(row) => <Report row={row()} busy={props.busy} team={props.team} onOpenNode={props.onOpenNode} />}</Show>}
    </For>
  </div>;
}
