import type { ActivityEvent, LocalMessage } from "../../types";

export const CODEX_QUEUE_SOURCE = "zerus_codex_queue";
// This mark belongs only to validated local and native receipts, never to a journal event.
export const CODEX_QUEUE_RECEIPT = Symbol("accepted Codex queue receipt");
export type QueueReceiptEvent = ActivityEvent & { [CODEX_QUEUE_RECEIPT]?: true };

const uuid = (id: unknown): id is string => typeof id === "string" && /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(id)
  && id !== "00000000-0000-0000-0000-000000000000";
const obj = (value: unknown): value is ActivityEvent => !!value && typeof value === "object" && !Array.isArray(value);
const sameScope = (a: ActivityEvent, b: ActivityEvent) => a.run_id === b.run_id && a.conversation_id === b.conversation_id;
const mainPrompt = (event: ActivityEvent) => event.type === "UserPromptSubmit" && !event.origin && !event.inherited_from
  && (!event.agent_id || event.agent_id === "main") && (!event.turn_origin || event.turn_origin === "user");

export function mergeCodexQueueReceipts(events: readonly ActivityEvent[], summary: Record<string, unknown> | undefined,
  local: readonly LocalMessage[], validFiles: (request: string, files: unknown) => boolean, pending?: ReadonlySet<string>): readonly ActivityEvent[] {
  const receipts = new Map<string, ActivityEvent>();
  const conflicted = new Set<string>();
  function add(row: ActivityEvent) {
    const request = row.request_id;
    if (!uuid(request) || !uuid(row.native_queued_message_id) || !row.run_id || !row.conversation_id
      || typeof row.detail !== "string" || typeof row.at !== "number" || !Number.isFinite(row.at) || row.at <= 0
      || !validFiles(request, row.attachments) || (!row.detail && !row.attachments?.length)) return;
    const previous = receipts.get(request);
    if (previous && (!sameScope(previous, row) || previous.native_queued_message_id !== row.native_queued_message_id
      || (previous.codex_client_id && row.codex_client_id && previous.codex_client_id !== row.codex_client_id))) {
      conflicted.add(request); return;
    }
    receipts.set(request, row);
  }
  for (const row of local) {
    if (row.source !== CODEX_QUEUE_SOURCE || row.status !== "sent"
      || (summary?.run_id && summary.run_id !== row.run_id)
      || (summary?.conversation_id && summary.conversation_id !== row.conversation_id)) continue;
    add({ request_id: row.id, native_queued_message_id: row.message_id, run_id: row.run_id, conversation_id: row.conversation_id,
      detail: row.text, at: row.submitted_at, attachments: row.attachments as ActivityEvent["attachments"] });
  }
  if (Array.isArray(summary?.codex_queued_messages)) {
    for (const row of summary.codex_queued_messages) {
      if (obj(row) && row.type === "CodexQueueReceipt" && row.source === "hgs_delivery" && row.status === "submitted"
        && row.transport === "codex_queue" && row.message_id === row.request_id && sameScope(row, summary)) add(row);
    }
  }
  if (!receipts.size) return events;
  const out: QueueReceiptEvent[] = events.slice();
  const byClient = new Map<string, number[]>(), byBody = new Map<string, number[]>();
  function index(map: Map<string, number[]>, key: string, i: number) {
    const positions = map.get(key);
    if (positions) positions.push(i); else map.set(key, [i]);
  }
  for (const [i, event] of events.entries()) {
    if (uuid(event.codex_client_id)) index(byClient, event.codex_client_id, i);
    if (event.message_id) index(byBody, event.message_id, i);
  }
  const nativeIds = new Map<unknown, number>(), clients = new Map<string, number>();
  for (const receipt of receipts.values()) {
    nativeIds.set(receipt.native_queued_message_id, 1 + (nativeIds.get(receipt.native_queued_message_id) ?? 0));
    if (uuid(receipt.codex_client_id)) clients.set(receipt.codex_client_id, 1 + (clients.get(receipt.codex_client_id) ?? 0));
  }
  const bodies = Array.isArray(summary?.user_message_bodies) ? summary.user_message_bodies.slice(-100).filter(obj) : [];
  const accepted: QueueReceiptEvent[] = [], queued: QueueReceiptEvent[] = [];
  const hidden = new Set<number>();
  for (const [request, receipt] of receipts) {
    if (pending?.has(request) || conflicted.has(request)) continue;
    const display = { activity_display_key: `local-${request}`, detail: receipt.detail, attachments: receipt.attachments };
    const client = receipt.codex_client_id;
    const exactOwner = uuid(client) && clients.get(client) === 1 && nativeIds.get(receipt.native_queued_message_id) === 1;
    const stillQueued = receipt.native_queue_state === "queued" && exactOwner;
    if (exactOwner) {
      const boundBodies = bodies.filter(body => body.type === "UserMessageBody" && body.source === "codex_transcript"
        && !body.agent_id && !body.origin && !body.inherited_from && body.codex_client_id === client
        && (!body.turn_origin || body.turn_origin === "user") && body.conversation_id === receipt.conversation_id
        && (body.run_id === undefined || body.run_id === receipt.run_id)
        && typeof body.message_id === "string" && !!body.message_id);
      const body = boundBodies.length === 1 ? boundBodies[0] : undefined;
      const messageId = body?.message_id;
      const bodyId = messageId && bodies.filter(candidate => candidate.message_id === messageId).length === 1 ? messageId : undefined;
      const claims = new Set(byClient.get(client) ?? []);
      if (bodyId) for (const i of byBody.get(bodyId) ?? []) claims.add(i);
      const matches = [...claims].filter(i => {
        const event = events[i];
        return mainPrompt(event) && (!event.source || event.source === "codex_transcript")
          && (event.codex_client_id === undefined || event.codex_client_id === client)
          && ((event.codex_client_id === client && sameScope(event, receipt))
            || (bodyId && event.source === "codex_transcript" && event.message_id === bodyId))
          && (event.run_id === undefined || event.run_id === receipt.run_id)
          && (event.conversation_id === undefined || event.conversation_id === receipt.conversation_id)
          && (!event.activity_display_key || event.activity_display_key === display.activity_display_key)
          && (!event.attachment_request_id || event.attachment_request_id === request)
          && (!event.attachments?.some(file => file.request_id && file.request_id !== request));
      });
      const echo = matches.length === 1 ? matches[0] : undefined;
      const at = body?.at;
      const timed = receipt.native_queue_state === "consumed" && bodyId && typeof at === "number"
        && at > 0 && at <= 8.64e12 && claims.size === (echo === undefined ? 0 : 1);
      const promptText = { ...display, message_text: receipt.detail, message_text_truncated: false };
      if (echo !== undefined) {
        if (stillQueued) {
          // Current native queue membership takes precedence over one exactly bound echo.
          hidden.add(echo);
        } else {
          const event = { ...out[echo], ...promptText };
          if (timed) {
            hidden.add(echo);
            accepted.push({ ...event, at });
          } else out[echo] = event;
          continue;
        }
      }
      if (timed) {
        // Native history can precede hooks. Use the original receipt for text and files.
        accepted.push({ type: "UserPromptSubmit", source: "codex_transcript", run_id: receipt.run_id,
          conversation_id: receipt.conversation_id, message_id: bodyId, codex_client_id: client, at, ...promptText });
        continue;
      }
    }
    const row: QueueReceiptEvent = { ...receipt, ...display, [CODEX_QUEUE_RECEIPT]: true };
    if (stillQueued) queued.push(row);
    else {
      // Admission alone proves no current queue position.
      if (row.native_queue_state === "queued") delete row.native_queue_state;
      accepted.push(row);
    }
  }
  if (!queued.length && !accepted.length) return out;
  accepted.sort((a, b) => a.at! - b.at!);
  queued.sort((a, b) => a.at! - b.at!);
  // Preserve journal order. Only current native queue entries sit at the live end.
  const merged: QueueReceiptEvent[] = [];
  let cursor = 0;
  for (const [i, event] of out.entries()) {
    if (hidden.has(i)) continue;
    while (cursor < accepted.length && accepted[cursor].at! < (event.at ?? 0)) merged.push(accepted[cursor++]);
    merged.push(event);
  }
  return merged.concat(accepted.slice(cursor), queued);
}
