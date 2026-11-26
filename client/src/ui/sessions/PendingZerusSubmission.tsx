import { Show } from "solid-js";
import { activity } from "../../store/session";
import { selected } from "../../store/ui";
import { tr } from "../../i18n";
import { timeText } from "./transcript";

interface Submission {
  request_id?: unknown;
  run_id?: unknown;
  conversation_id?: unknown;
  submitted_at?: unknown;
  sender?: unknown;
}

const requestId = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** Renders the host receipt as its own record, independent of native previews. */
export function PendingZerusSubmission() {
  const receipt = () => {
    const session = selected(), target = activity.target(), summary = activity.currentSummary();
    if (!session || session.archiveId || !target || target.id !== session.key
      || target.key !== session.catalogKey || target.computer !== session.computer
      || summary?.previous_message_pending !== true || summary.run_id !== session.runId
      || !session.conversationId || summary.conversation_id !== session.conversationId) return;
    const value = summary.pending_zerus_submission as Submission | undefined;
    if (!value || value.run_id !== summary.run_id || value.conversation_id !== summary.conversation_id
      || typeof value.request_id !== "string" || !requestId.test(value.request_id) || !/[1-9a-f]/.test(value.request_id)
      || typeof value.submitted_at !== "number" || !(value.submitted_at > 0 && value.submitted_at <= 8.64e12)
      || typeof value.sender !== "string" || !value.sender) return;
    return { at: value.submitted_at, sender: value.sender };
  };
  return <Show when={receipt()}>{(entry) => (
    <aside class="act-zerus-submission" aria-label={tr("Pending Zerus submission")}>
      <time datetime={new Date(entry().at * 1000).toISOString()}>{tr("Zerus submission recorded at %1", timeText(entry().at))}</time>
      <span>{tr("Sender declared as %1", entry().sender)}</span>
    </aside>
  )}</Show>;
}
