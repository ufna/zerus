import type { ActivityEvent, LocalMessage, Reply } from "../../types";
import { eventKey, opencodePromptId } from "../../store/activity";
import type { ActivityItem, InboxItem, LocalItem, MediaMessage, OutboxItem, RouteLine, ToolEvent, TranscriptItem, VoiceState } from "./types";
import type { RouteActivity } from "../../store/questionRouteActivity";
import { imageSize } from "./attachment-image";
import { messageTimeOf } from "../lib/dates";
import { tr, trNoop } from "../../i18n";
import { ownMessageText, ownMessageTruncated, restoreOwnMessages } from "./full-own-text";
import { earlierInputRefusal } from "./send-error";

// A Symbol cannot arrive from a provider's JSON. Only our own local send
// supplies these immutable-cache preview paths and oriented dimensions.
const LOCAL_FILES = Symbol("local attachment presentation");
// Likewise only the service's own route read supplies a route line, never a journal event.
const ROUTE = Symbol("question route line");
type DisplayEvent = ActivityEvent & { [LOCAL_FILES]?: LocalMessage["attachments"]; [ROUTE]?: RouteLine };
const routeOf = (e: ActivityEvent) => (e as DisplayEvent)[ROUTE];

// Journal events as the Activity items of the approved design, after the
// rules of tray/src/ActivityView.cpp: messages by role, thinking, cleared
// sessions, notices (subagent reports, peer messages, background tasks),
// voice and video messages, and adjacent operational events collapsed into one
// tool group (at most 24, never across a message or an attention event).

/** Delivery labels, as in ActivityView.cpp (unknown states read "Delivery not confirmed"). */
export const VOICE_STATE_LABEL: Record<VoiceState, string> = {
  uploading: trNoop("Sending recording…"),
  queued: trNoop("Waiting for recognition"),
  transcribing: trNoop("Recognizing…"),
  ready: trNoop("Ready to deliver"),
  waiting_agent: trNoop("Waiting for agent"),
  delivering: trNoop("Delivering…"),
  submitted: trNoop("Accepted by native app"),
  failed: trNoop("Voice message failed"),
  cancelled: trNoop("Cancelled"),
  uncertain: trNoop("Delivery not confirmed"),
};

/** Tooltip detail: only Accepted explains what it does not confirm. */
export function voiceStateDetail(state: VoiceState): string {
  return state === "submitted"
    ? tr("The native app accepted the recognized text. Queue status, when present, is shown alongside this recording. This does not confirm model delivery, reading or completion.")
    : tr(VOICE_STATE_LABEL[state]);
}

/** States drawn with the warning glyph (VoicePlayerImage::deliveryIcon). */
export const voiceStateIssue = (state: VoiceState) => state === "failed" || state === "uncertain";

const EVENT_TITLE: Record<string, string> = {
  SessionStart: trNoop("Session started"), SessionEnd: trNoop("Session closed"),
  TurnStarted: trNoop("Turn started"), UserPromptQueued: trNoop("Message queued"),
  PreToolUse: trNoop("Tool started"), ToolProgress: trNoop("Tool running"), PostToolUse: trNoop("Tool finished"),
  PostToolUseFailure: trNoop("Tool failed"), PermissionRequest: trNoop("Approval requested"),
  PermissionResult: trNoop("Approval resolved"), Stop: trNoop("Response finished"),
  StopFailure: trNoop("Response failed"), Interrupt: trNoop("Turn interrupted"),
  PreCompact: trNoop("Compacting context"), PostCompact: trNoop("Context compacted"),
  CompactFailure: trNoop("Context compaction failed"), CompactCancelled: trNoop("Context compaction cancelled"),
  SubagentStart: trNoop("Subagent started"), SubagentStop: trNoop("Subagent finished"),
  TaskStarted: trNoop("Background task started"), Notification: trNoop("Agent notification"),
};
export const eventTitle = (type: string | undefined) => (type ? (type in EVENT_TITLE ? tr(EVENT_TITLE[type]) : type) : tr("Activity"));

/** "Oct 27 09:12" ("27 октября 09:12" in Russian), local time, as the design shows message times. */
export function timeText(at: number | undefined): string {
  if (!at || at <= 0) return "";
  return messageTimeOf(new Date(at * 1000));
}

type Role = "user" | "assistant" | "notice" | "";
const childEvent = (event: ActivityEvent): event is ActivityEvent & { agent_id: string } => !!event.agent_id && event.agent_id !== "main";

/** ActivityView.cpp messageRole: who wrote an event, if anyone. */
export function messageRole(e: ActivityEvent): Role {
  if (e.source === "zerus_task" && e.type === "TaskEntry") return e.task_role === "person" ? "user" : e.task_role === "agent" ? "assistant" : "";
  if (e.source === "hgs_voice" && e.voice && typeof e.voice === "object") return "user";
  if (childEvent(e) || (!e.detail && !(e.attachments?.length))) return "";
  if (e.origin === "task_notification" || e.origin === "subagent_report" || e.origin === "peer_message") return "notice";
  switch (e.type) {
    case "UserPromptSubmit": case "UserPromptQueued": case "UserMessage": case "TurnStarted": case "QuestionAnswered": case "LocalMessage": return "user";
    case "Stop": case "AgentMessage": return "assistant";
  }
  return "";
}

/** Approvals and failures open their group and are never merged with others. */
const attention = (e: ActivityEvent) => e.type === "PermissionRequest" || e.type === "PostToolUseFailure" || e.type === "StopFailure" || e.type === "Interrupt";

const openCodeCompaction = (e: ActivityEvent) => e.source === "opencode_native"
  && (e.type === "PreCompact" || e.type === "PostCompact" || e.type === "CompactFailure" || e.type === "CompactCancelled");

/** A message's files as Qt's attachment cards show them (ActivityView.cpp:759-782). */
export function attachments(e: ActivityEvent): ActivityItem["attachments"] {
  const list = Array.isArray(e.attachments) ? e.attachments : [];
  if (!list.length) return undefined;
  return list.map((a, index) => {
    const text = (value: unknown) => (typeof value === "string" ? value : "");
    const name = text(a?.name), reference = text(a?.reference), mime = text(a?.mime);
    const bytes = typeof a?.bytes === "number" && Number.isFinite(a.bytes) ? a.bytes : 0;
    const local = (e as DisplayEvent)[LOCAL_FILES]?.[index];
    const own = !!local && local.name === name && local.mime === mime && local.bytes === bytes && local.reference === reference;
    const preview = own && typeof local.preview_path === "string" && /^cache\/[a-f0-9]{64}\/[^/\\]+$/u.test(local.preview_path) ? local.preview_path : undefined;
    return {
      name,
      caption: (reference ? `${reference} ` : "") + name,
      mime,
      meta: bytes > 0 ? `${Math.max(1, Math.ceil(bytes / 1024))} KiB` : "File",
      request_id: text(a?.request_id),
      index: typeof a?.index === "number" && Number.isInteger(a.index) ? a.index : -1,
      image: mime.startsWith("image/") && mime !== "image/svg+xml",
      ...(own ? { own: true, preview_path: preview, image_size: imageSize(local.image_size) } : {}),
    };
  });
}

interface AttachmentEcho {
  reference: string;
  kind: "Image" | "File";
  path: string;
}

/** The numbered terminal echoes produced by attachments::terminal_text. */
export function attachmentEchoes(text: string): AttachmentEcho[] {
  const pattern = /\[(Image|File) #([1-9][0-9]{0,8})\] ?\((Inspect the attached image at|Read the attached file at) ("(?:[^"\\]|\\.)*") ?\)/g;
  const echoes: AttachmentEcho[] = [];
  for (const found of text.matchAll(pattern)) {
    const kind = found[1] as AttachmentEcho["kind"];
    if ((kind === "Image") !== (found[3] === "Inspect the attached image at")) continue;
    try {
      const path = JSON.parse(found[4]);
      if (typeof path === "string") echoes.push({ reference: found[0].slice(0, found[0].indexOf("]") + 1), kind, path });
    } catch { /* A malformed path stays ordinary message text. */ }
  }
  return echoes;
}

function echoMatchesFiles(text: string, files: ActivityEvent["attachments"]): boolean {
  const echoes = attachmentEchoes(text);
  if (!files?.length || echoes.length !== files.length) return false;
  return echoes.every((echo, index) => {
    const file = files[index];
    const name = typeof file?.name === "string" ? file.name : "";
    const reference = typeof file?.reference === "string" ? file.reference : "";
    const mime = typeof file?.mime === "string" ? file.mime : "";
    const filename = `${String(index + 1).padStart(2, "0")}-${name.replace(/[^A-Za-z0-9._-]/g, "_").slice(0, 100)}`;
    const path = echo.path.replaceAll("\\", "/");
    return echo.reference === reference && (echo.kind === "Image") === mime.startsWith("image/")
      && /(?:^|\/)attachments\/message-[A-Za-z0-9]+\/[^/]+$/.test(path) && path.split("/").at(-1) === filename;
  });
}

/**
 * Reconciles a native numbered prompt echo with its one exact delivery receipt.
 * Public receipt metadata supplies the request and index; an echoed host path
 * is never used to read a file. Unknown or ambiguous echoes stay untouched.
 */
function claudePromptMatch(event: ActivityEvent, prompt: string | undefined, run: string | undefined,
  conversation: string | undefined): boolean {
  return event.type === "UserPromptSubmit" && !event.agent_id && !event.origin && !event.inherited_from
    && (!event.turn_origin || event.turn_origin === "user")
    && !!prompt && event.claude_prompt_id === prompt && !!run && event.run_id === run
    && !!conversation && event.conversation_id === conversation;
}

export function mergeAttachmentMessages(given: readonly ActivityEvent[], summary?: Record<string, unknown>): readonly ActivityEvent[] {
  const receipts = Array.isArray(summary?.attachment_messages) ? summary.attachment_messages as ActivityEvent[] : [];
  if (!receipts.length) return given;
  const events = [...given];
  const valid = receipts.filter(receipt => receipt.source === "hgs_delivery" && receipt.attachments?.length
    && typeof receipt.submitted_text === "string" && receipt.submitted_text.trim() && echoMatchesFiles(receipt.submitted_text, receipt.attachments));
  const own = (event: ActivityEvent) => event.source !== "hgs_voice" && !childEvent(event) && !event.origin
    && (!event.turn_origin || event.turn_origin === "user") && !event.inherited_from && PROMPTS.has(event.type ?? "");
  const requests = (event: ActivityEvent): string[] => [...new Set([
    event.attachment_request_id, ...(event.source === "hgs_delivery" ? [event.message_id] : []),
    ...(opencodePromptId(event) ? [event.request_id] : []),
    ...(event.attachments ?? []).map(file => file.request_id),
  ].filter((id): id is string => typeof id === "string" && !!id))];
  let changed = false;
  for (const receipt of valid) {
    const request = typeof receipt.message_id === "string" ? receipt.message_id : "";
    const submitted = String(receipt.submitted_text).trim();
    if (!request || receipts.filter(other => other.message_id === request).length !== 1) continue;
    if (!receipt.claude_outbox && receipts.filter(other => typeof other.submitted_text === "string"
      && other.submitted_text.trim() === submitted).length !== 1) continue;
    // Every already bound row reserves its request, including complete rows,
    // local cards and rows that are no longer eligible for body replacement.
    const owners = events.flatMap((event, index) => requests(event).includes(request) ? [index] : []);
    if (owners.length > 1) continue;
    const exact = receipt.claude_outbox
      ? events.flatMap((event, index) => claudePromptMatch(event, receipt.claude_prompt_id, receipt.run_id, receipt.conversation_id)
        && requests(event).every(id => id === request) ? [index] : [])
      : owners.length ? owners : events.flatMap((event, index) => own(event) && !opencodePromptId(event)
        && !ownMessageTruncated(event) && !requests(event).length && !event.attachments?.length
        && ownMessageText(event).trim() === submitted ? [index] : []);
    if (exact.length !== 1 || !own(events[exact[0]])) continue;
    const at = exact[0], event = events[at];
    events[at] = {
      ...event, detail: typeof receipt.detail === "string" ? receipt.detail : "", attachments: receipt.attachments,
      message_text: typeof receipt.detail === "string" ? receipt.detail : "", message_text_truncated: false,
      attachment_echo: receipt.submitted_text, attachment_request_id: receipt.message_id, attachment_submitted_text: receipt.submitted_text,
    };
    changed = true;
  }
  return changed ? events : given;
}

function media(e: ActivityEvent, id: string): MediaMessage {
  const voice = (e.voice ?? {}) as Record<string, unknown>;
  const audio = (voice.audio ?? {}) as Record<string, unknown>;
  const video = (voice.video ?? undefined) as Record<string, unknown> | undefined;
  const state = String(voice.state ?? "uncertain") as VoiceState;
  const waveform = Array.isArray(audio.waveform) ? (audio.waveform as unknown[]).map((v) => Math.max(0, Math.min(255, Number(v) || 0))) : [];
  const file = video && Object.keys(video).length ? video : audio;
  const index = Number(file.index);
  const message = typeof e.message_id === "string" && e.message_id || (typeof voice.id === "string" ? voice.id : "");
  return {
    id, kind: "media", media: video && Object.keys(video).length ? "video" : "voice", at: timeText(e.at),
    state: state in VOICE_STATE_LABEL ? state : "uncertain",
    durationMs: Math.max(0, Number(audio.duration_ms) || 0),
    waveform: waveform.length === 64 ? waveform : [],
    posterMessageId: video?.poster_available === true && message ? message : undefined,
    file: typeof audio.request_id === "string" && !!audio.request_id && typeof file.request_id === "string" && !!file.request_id
      && typeof file.index === "number" && Number.isSafeInteger(index) && index >= 0 ? { request_id: file.request_id, index } : undefined,
    transcript: typeof voice.transcript === "string" && voice.transcript ? voice.transcript : undefined,
    error: typeof voice.error === "string" && voice.error ? voice.error : undefined,
  };
}

const GROUP_LIMIT = 24;
const PREVIEW = 110;
/** At most this much of an output is read for its preview, whatever its whitespace (zerus-1nsc6). */
export const TOOL_PREVIEW_SCAN = 8_192;
const SPACE = /\s/;
/**
 * The text's whitespace runs as single spaces, trimmed and cut to 110
 * characters: read only as far as the preview shows and never past its first
 * TOOL_PREVIEW_SCAN characters, so a multi-megabyte output costs no more than
 * a short one (zerus-5wmh.13.8.1). A preview that stopped at that bound with
 * more text left ends with "…".
 */
export function toolPreview(text: string | undefined): string {
  const source = text ?? "";
  const end = Math.min(source.length, TOOL_PREVIEW_SCAN);
  let flat = "", space = false, i = 0;
  for (; i < end && flat.length <= PREVIEW; i++) {
    const c = source[i];
    if (SPACE.test(c)) { space = flat.length > 0; continue; }
    if (space) { flat += " "; space = false; }
    flat += c;
  }
  if (flat.length > PREVIEW || (i === TOOL_PREVIEW_SCAN && source.length > TOOL_PREVIEW_SCAN)) {
    return `${flat.slice(0, PREVIEW - 1)}…`;
  }
  return flat;
}
const preview = toolPreview;

/** One reader per event: rebuilding the transcript leaves an unchanged event's row unchanged. */
const outputs = new WeakMap<ActivityEvent, () => string>();
function outputOf(e: ActivityEvent): () => string {
  let read = outputs.get(e);
  if (!read) { read = () => e.detail ?? ""; outputs.set(e, read); }
  return read;
}

/** An operational event as an expanded tool row shows it; its output stays in the event. */
function toolEvent(e: ActivityEvent): ToolEvent {
  const process = typeof e.process_id === "string" && e.process_id ? e.process_id : undefined;
  return {
    label: `${eventTitle(e.type)}${e.tool ? ` / ${e.tool}` : ""}${childEvent(e) ? tr(" (subagent %1)", e.agent_id) : ""}`,
    at: timeText(e.at), size: (e.detail ?? "").length, process, detail: outputOf(e),
  };
}

/**
 * The request and reply the summary recorded, when the timeline has no events
 * (a journal that kept nothing of this conversation). As in Qt, they are shown
 * as preceding context, never as new messages.
 */
function recordedContext(summary: Record<string, unknown> | undefined): TranscriptItem[] {
  const prompt = typeof summary?.prompt === "string" ? summary.prompt.trim() : "";
  const answer = typeof summary?.last_message === "string" ? summary.last_message.trim() : "";
  if (!prompt && !answer) return [];
  const items: TranscriptItem[] = [{ id: "context", kind: "context", at: "", body: tr("Recorded context — outside the available timeline") }];
  // A recorded prompt that holds Codex replies reads as one (Rust's prompt_replies).
  const replies = Array.isArray(summary?.prompt_replies) && summary.prompt_replies.length ? summary.prompt_replies as Reply[] : undefined;
  if (prompt) items.push({ id: "prompt-snapshot", kind: "user", at: "", label: replies ? tr("You (recorded answer)") : tr("You (recorded request)"), body: prompt, replies });
  if (answer) items.push({ id: "answer-snapshot", kind: "agent", at: "", label: tr("Agent (recorded response)"), body: answer });
  return items;
}

// ---------------------------------------------------------------------------
// Local message rows (Qt's m_localMessages): what this app sent until the
// agent's own history shows it, and what failed (ActivityView.cpp:665-720,
// 1053-1068).

const PROMPTS = new Set(["UserPromptSubmit", "UserPromptQueued", "TurnStarted"]);
const OUTBOX_SOURCE = "zerus_outbox";
const LOCAL_STAMP: Record<LocalMessage["status"], string> = { sending: trNoop("Sending…"), error: trNoop("Not sent"), sent: trNoop("Submitted to terminal") };
const OPENCODE_SOURCE = "opencode_native";
const CLAUDE_OUTBOX_SOURCE = "zerus_claude_outbox";
const OPENCODE_SUBMITTED = trNoop("Submitted to OpenCode");
const agyOffer = (event: ActivityEvent) => event.source === "agy_acp" && event.type === "UserPromptSubmit";
const agyRequest = (event: ActivityEvent) => typeof event.request_id === "string" && event.request_id
  ? JSON.stringify([event.run_id, event.conversation_id, event.request_id]) : "";

/** Older workers recorded only the offer. An exact native reply supplies proof for that request. */
function agyDelivery(events: readonly ActivityEvent[]): readonly ActivityEvent[] {
  if (!events.some(agyOffer)) return events;
  const replies = new Set(events.filter(e => e.source === "agy_acp" && e.type === "AgentMessage").map(agyRequest));
  return events.map(event => {
    if (!agyOffer(event) || event.delivery_state !== undefined) return event;
    const request = agyRequest(event);
    return { ...event, delivery_state: request && replies.has(request) ? "submitted" : "unconfirmed" };
  });
}
/** How far a sent row's own prompt event may be from it, in seconds (ActivityView.cpp:684). */
const NEAREST = 10;

/** The text the agent received (the receipt's, else the row's), trimmed. */
const submittedOf = (row: LocalMessage) => (row.submitted_text ?? row.text).trim();

/**
 * Whether a local row's file can be opened: it has its receipt's request or
 * its cached copy (Qt's rule, ActivityView.cpp:765). Activity's journal cards
 * read a file by its request only (attachment-card.ts `cardTarget`).
 */
export const localFileAvailable = (file: Record<string, unknown>) =>
  (typeof file.request_id === "string" && file.request_id !== "") || (typeof file.local_path === "string" && file.local_path !== "");

/** A failed or pending row's stamp, error line and buttons (ActivityView.cpp:1053-1068). */
export function localItem(row: LocalMessage, summary?: Record<string, unknown>): LocalItem {
  const failed = row.status === "error";
  const actions: LocalItem["actions"] = [];
  if (failed && row.id) {
    // Uncertain delivery can be checked here. Allow retry stays in the composer.
    if (!row.uncertain) actions.push("retry");
    if (row.uncertain || earlierInputRefusal(row.error)) actions.push("inspect");
    actions.push("delete");
  }
  return {
    id: row.id,
    stamp: failed && row.uncertain ? tr("Delivery not confirmed")
      : tr(row.status === "sent" && row.source === OPENCODE_SOURCE ? OPENCODE_SUBMITTED : LOCAL_STAMP[row.status] ?? LOCAL_STAMP.sent),
    error: failed ? row.error : "",
    uncertain: failed && row.uncertain,
    actions,
    inspectLabel: summary?.backend === "agy" ? tr("Check Activity")
      : summary?.native_ui_available === true ? tr("Check native UI") : tr("Check Terminal"),
    available: row.attachments.map(localFileAvailable),
  };
}

/** A local row as a timeline event (Qt's `type: LocalMessage`). */
function localEvent(row: LocalMessage): DisplayEvent {
  return {
    type: "LocalMessage", detail: row.text, at: row.submitted_at, attachments: row.attachments as ActivityEvent["attachments"],
    source: row.source ?? undefined, voice: row.voice ?? undefined, message_id: row.message_id ?? undefined, local: row,
    [LOCAL_FILES]: row.attachments,
  };
}

/**
 * The events with the session's local rows merged in (ActivityView.cpp:665-720).
 * A voice row the journal already holds is left out. A sent row takes over its
 * own prompt event, which then shows its text and files: the one event with
 * exactly its text when it has files and no other sent row competes, else the
 * nearest prompt within 10 s that starts like it. Every other row is placed by
 * its time, after the events of the same time.
 */
export function mergeLocal(events: readonly ActivityEvent[], local: readonly LocalMessage[], bindingOnly: readonly LocalMessage[] = []): ActivityEvent[] {
  const source = events.slice();
  const appended: ActivityEvent[] = [];
  const claimed = new Set<number>();
  const candidates = [...local, ...bindingOnly];
  for (const row of candidates) {
    const append = local.includes(row);
    if (row.source === "hgs_voice") {
      if (append && !source.some((e) => e.source === "hgs_voice" && e.message_id === row.message_id)) appended.push(localEvent(row));
      continue;
    }
    const submitted = submittedOf(row);
    const sent = row.status !== "error";
    const fifo = row.source === OUTBOX_SOURCE;
    let match = -1;
    const exact: number[] = [];
    const eligible = (e: ActivityEvent, i: number) => !claimed.has(i) && !childEvent(e) && e.source !== "hgs_voice" && PROMPTS.has(e.type ?? "")
      && (!e.activity_display_key || e.activity_display_key === `local-${row.id}`);
    const agy = row.id && (row.status !== "error" || row.uncertain)
      ? source.findIndex((e, i) => agyOffer(e) && eligible(e, i) && e.request_id === row.id
        && (e.run_id === undefined || e.run_id === row.run_id)
        && (e.conversation_id === undefined || e.conversation_id === row.conversation_id)) : -1;
    if (agy >= 0) {
      const offer = source[agy];
      const submitted = offer.delivery_state === "submitted" || row.status === "sent";
      source[agy] = {
        ...offer, ...(submitted ? { detail: row.text, message_text: row.text, message_text_truncated: false, delivery_state: "submitted" }
          : localEvent(row)),
        activity_journal_key: eventKey(offer), activity_display_key: `local-${row.id}`,
      };
      claimed.add(agy);
      continue;
    }
    if (sent && row.source === OPENCODE_SOURCE) {
      match = source.findIndex((e, i) => {
        const native = opencodePromptId(e);
        return !!native && eligible(e, i)
          && (e.run_id === undefined || e.run_id === row.run_id)
          && (e.conversation_id === undefined || e.conversation_id === row.conversation_id)
          && (e.request_id === undefined || e.request_id === row.id)
          && (row.message_id && row.message_id !== row.id ? row.message_id === native : e.request_id === row.id);
      });
    }
    if (sent && row.source === CLAUDE_OUTBOX_SOURCE) {
      match = source.findIndex((e, i) => eligible(e, i)
        && claudePromptMatch(e, row.message_id, row.run_id, row.conversation_id));
    }
    const ordinary = (e: ActivityEvent, i: number) => row.source !== OPENCODE_SOURCE
      && row.source !== CLAUDE_OUTBOX_SOURCE && e.source !== OPENCODE_SOURCE && eligible(e, i) && !agyOffer(e);
    if (sent && fifo) match = source.findIndex((e, i) => ordinary(e, i)
      && outboxReceiptId(e, row) === row.id);
    if (sent && !fifo && match < 0) match = source.findIndex((e, i) => ordinary(e, i) && e.activity_display_key === `local-${row.id}`);
    if (sent && !fifo && match < 0) match = source.findIndex((e, i) => ordinary(e, i)
      && !!(e.message_id || e.attachment_request_id) && [row.id, row.message_id].some(id => !!id && (e.message_id === id || e.attachment_request_id === id)));
    if (sent && !fifo && match < 0 && row.attachments.length) {
      source.forEach((e, i) => {
        if (ordinary(e, i) && !ownMessageTruncated(e) && !e.attachments?.length
          && submitted && ownMessageText(e).trim() === submitted) exact.push(i);
      });
      const competing = candidates.filter((other) => other.status !== "error" && other.attachments.length && submittedOf(other) === submitted).length;
      // Qt also remembers which of several exact events a row was bound to; with one it is that one.
      if (competing === 1 && exact.length === 1) match = exact[0];
    }
    if (sent && !fifo && match < 0 && !exact.length) {
      const full = source.flatMap((e, i) => ordinary(e, i) && !ownMessageTruncated(e) && !e.attachments?.length
        && !e.message_id && !e.attachment_request_id && submitted && ownMessageText(e).trim() === submitted
        && Math.abs((e.at ?? NaN) - row.submitted_at) <= NEAREST ? [i] : []);
      if (full.length === 1) match = full[0];
    }
    if (match >= 0) {
      claimed.add(match);
      source[match] = {
        ...source[match], activity_journal_key: eventKey(source[match]), activity_key: `local-${row.id}`, activity_display_key: `local-${row.id}`, detail: row.text,
        message_text: row.text, message_text_truncated: false,
        attachments: source[match].attachments?.length ? source[match].attachments : row.attachments as ActivityEvent["attachments"], attachment_submitted_text: row.submitted_text,
        [LOCAL_FILES]: row.attachments,
      } as DisplayEvent;
      continue;
    }
    if (append) appended.push(localEvent(row));
  }
  // A native echo may precede both the CLI acknowledgement and its durable
  // receipt. Keep the client's row visible through that short gap; do not
  // invent an identity binding from text. Equal-text sends remain separate.
  const visible = source.filter((event, index) => claimed.has(index) || !candidates.some((row) => {
    if (row.source === OUTBOX_SOURCE || row.source === OPENCODE_SOURCE || row.source === CLAUDE_OUTBOX_SOURCE
      || event.source === OPENCODE_SOURCE
      || row.status === "error" || !row.attachments.length || !PROMPTS.has(event.type ?? "") || childEvent(event)
      || event.attachments?.length || Math.abs((event.at ?? NaN) - row.submitted_at) > NEAREST) return false;
    const raw = event.detail ?? "";
    if (!echoMatchesFiles(raw, row.attachments as ActivityEvent["attachments"])) return false;
    const canonical = raw.replace(/\[(Image|File) #([1-9][0-9]{0,8})\] ?\((?:Inspect the attached image at|Read the attached file at) (?:"(?:[^"\\]|\\.)*") ?\)/g, "[$1 #$2]");
    return canonical.trim() === row.text.trim();
  }));
  if (!appended.length) return visible;
  // Placed by time, after the events of the same time (Qt's stable sort by `at`).
  const placed = appended.map((e, i) => ({ e, i })).sort((a, b) => (a.e.at ?? 0) - (b.e.at ?? 0) || a.i - b.i).map((p) => p.e);
  const merged: ActivityEvent[] = [];
  let next = 0;
  for (const e of visible) {
    const at = typeof e.at === "number" ? e.at : 0;
    while (next < placed.length && at > 0 && (placed[next].at ?? 0) < at) merged.push(placed[next++]);
    merged.push(e);
  }
  while (next < placed.length) merged.push(placed[next++]);
  return merged;
}

const PROVIDER_SOURCES = new Set(["codex_transcript", "claude_transcript", "kimi_wire"]);
const MESSAGE_TYPES = new Set(["SessionCleared", "UserPromptSubmit", "UserPromptQueued", "UserMessage", "TurnStarted", "QuestionAnswered", "AgentMessage", "Stop"]);

/** The inspection keeps up to 100 main journal messages independently of its
 * tool-event tail. Merge that bounded window for display only; tool cursors and
 * older-page requests continue to use the source's original journal events. */
export function mergeMessages(given: readonly ActivityEvent[], summary?: Record<string, unknown>): readonly ActivityEvent[] {
  if (!Array.isArray(summary?.message_events) || !summary.message_events.length) return given;
  const events = [...given];
  const known = new Set(given.map(eventKey));
  for (const value of summary.message_events.slice(-100)) {
    if (!value || typeof value !== "object") continue;
    const message = value as ActivityEvent;
    if (!MESSAGE_TYPES.has(message.type ?? "") || childEvent(message)
      || typeof message.seq !== "number" || !Number.isSafeInteger(message.seq) || message.seq <= 0) continue;
    const key = eventKey(message);
    if (known.has(key)) continue;
    known.add(key);
    events.push(message);
  }
  if (events.length === given.length) return given;
  if (events.every(event => typeof event.seq === "number" && event.seq > 0)) events.sort((a, b) => a.seq! - b.seq!);
  else if (events.every(event => typeof event.at === "number" && Number.isFinite(event.at) && event.at > 0)) events.sort((a, b) => a.at! - b.at!);
  return events;
}
const isExcerpt = (excerpt: string, full: string) => {
  const prefix = excerpt.endsWith("…") ? excerpt.slice(0, -1) : excerpt;
  return !!prefix && full.startsWith(prefix);
};

const eventAt = (e: ActivityEvent) => {
  const value = e.at;
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
};

const NEARBY_STOP_SECONDS = 2;
const DELAYED_CLAUDE_STOP_SECONDS = 10;
type ClaudeReply = { event: ActivityEvent; insertedAt?: number };
const CLAUDE_TURN_BOUNDARIES = new Set([
  "SessionStart", "SessionEnd", "SessionCleared", "UserPromptQueued", "UserMessage", "TurnStarted",
  "QuestionAnswered", "AgentMessage", "StopFailure", "Interrupt",
]);
const ordinaryTurn = (e: ActivityEvent) =>
  !e.inherited_from && !e.origin && (!e.turn_origin || e.turn_origin === "user");
const completeBody = (e: ActivityEvent) =>
  !!e.detail?.trim() && !e.detail.endsWith("…") && e.message_text_truncated !== true;

// A Claude Stop is timed on journal arrival. Allow a brief delay only with a
// visible same-run prompt and a unique, complete final reply in that turn.
function delayedClaudeCopies(
  events: readonly ActivityEvent[], journalLength: number, replies: readonly ClaudeReply[], matchedStops: ReadonlySet<number>,
): Set<number> {
  const copies = new Set<number>();
  const pendingBodies = new Set(replies.filter(row => row.insertedAt !== undefined && completeBody(row.event)).map(row => row.event.detail));
  if (!pendingBodies.size || replies.some(row => eventAt(row.event) <= 0)) return copies;
  const conflicts = new Set<number>();
  let prompt: ActivityEvent | undefined;
  for (let i = 0; i < journalLength; i++) {
    const event = events[i];
    if (childEvent(event)) continue;
    if (event.type === "UserPromptSubmit") {
      prompt = ordinaryTurn(event) ? event : undefined;
    } else if (CLAUDE_TURN_BOUNDARIES.has(event.type ?? "")) {
      prompt = undefined;
    } else if (event.type === "Stop") {
      const start = prompt;
      prompt = undefined;
      if (!start || typeof start.run_id !== "string" || !start.run_id || start.run_id !== event.run_id || matchedStops.has(i)
        || !ordinaryTurn(event) || !pendingBodies.has(event.detail) || !completeBody(event)) continue;
      const from = eventAt(start), until = eventAt(event);
      if (from <= 0 || until <= from) continue;
      const inTurn = replies.filter(row => {
        const at = eventAt(row.event);
        return at > from && at <= until;
      });
      const [reply, other] = inTurn.filter(row => row.event.detail === event.detail);
      if (reply?.insertedAt === undefined || other) continue;
      const { run_id, message_phase } = reply.event;
      // Today's Claude transcript producer omits both fields. If supplied,
      // they must agree with the journal turn and its final reply.
      if (run_id !== undefined && run_id !== start.run_id) continue;
      if (message_phase !== undefined && message_phase !== "final" && message_phase !== "final_answer") continue;
      const when = eventAt(reply.event);
      const delay = until - when;
      if (delay <= NEARBY_STOP_SECONDS || delay > DELAYED_CLAUDE_STOP_SECONDS
        || !ordinaryTurn(reply.event) || !completeBody(reply.event)) continue;
      if (inTurn.some(row => row !== reply && eventAt(row.event) >= when)) continue;
      if (copies.has(reply.insertedAt)) conflicts.add(reply.insertedAt);
      else copies.add(reply.insertedAt);
    }
  }
  for (const index of conflicts) copies.delete(index);
  return copies;
}

/**
 * Qt's provider_messages merge: watch event frames also arrive here, folded
 * into the summary by the core. Keep journal cursors intact and project the
 * bounded provider window at render time, including progress between tools.
 */
export function mergeProvider(given: readonly ActivityEvent[], summary?: Record<string, unknown>): readonly ActivityEvent[] {
  const restored = restoreOwnMessages(given, summary);
  if (!Array.isArray(summary?.provider_messages) || !summary.provider_messages.length) return restored;
  let events = [...restored];
  const matchedStops = new Set<number>();
  const identities = new Set<string>();
  const seconds = new Map<number, number[]>();
  const nativeKey = (e: ActivityEvent) => e.message_id ? `${e.source}:${e.type}:${e.message_id}` : "";
  const claudeReplies: ClaudeReply[] = [];
  // Index only matching candidates once, rather than scanning every loaded
  // journal page for each provider row on every streaming rebuild.
  const index = (e: ActivityEvent, i: number) => {
    if (childEvent(e) || (e.type !== "Stop" && !PROVIDER_SOURCES.has(e.source ?? ""))) return;
    const key = nativeKey(e);
    if (key) identities.add(key);
    const second = Math.floor(eventAt(e));
    const bucket = seconds.get(second);
    if (bucket) bucket.push(i); else seconds.set(second, [i]);
  };
  events.forEach(index);
  for (const value of summary.provider_messages.slice(-100)) {
    if (!value || typeof value !== "object") continue;
    const message = value as ActivityEvent;
    if (!PROVIDER_SOURCES.has(message.source ?? "") || childEvent(message)
      || (message.type !== "AgentMessage" && !(message.type === "AgentThinking" && message.source === "claude_transcript"))) continue;
    // Even a provider copy already visible in the journal can be evidence
    // of a later reply. Record it before the display deduplication checks.
    const claudeReply: ClaudeReply | undefined = message.type === "AgentMessage" && message.source === "claude_transcript"
      ? { event: message } : undefined;
    if (claudeReply) claudeReplies.push(claudeReply);
    const key = nativeKey(message);
    if (key && identities.has(key)) continue;
    const when = eventAt(message);
    const nearby: number[] = [];
    for (let offset = -3; offset <= 3; offset++) {
      for (const i of seconds.get(Math.floor(when) + offset) ?? []) if (Math.abs(eventAt(events[i]) - when) <= 3) nearby.push(i);
    }
    nearby.sort((a, b) => a - b);
    const same = nearby.some((i) => {
      const e = events[i];
      return e.type === message.type && e.source === message.source
        && !(message.message_id && e.message_id && message.message_id !== e.message_id)
        && e.detail === message.detail && Math.abs(eventAt(e) - when) < 3;
    });
    if (same) continue;
    // The hook's final excerpt and the provider's full response are one row.
    // An empty Stop never absorbs commentary immediately before completion.
    const stop = message.type === "AgentMessage" ? nearby.find((i) => !matchedStops.has(i) && events[i].type === "Stop"
      && Math.abs(eventAt(events[i]) - when) <= NEARBY_STOP_SECONDS
      && (events[i].detail ? isExcerpt(events[i].detail!, message.detail ?? "") : message.message_phase !== "commentary")) : undefined;
    if (stop !== undefined) {
      matchedStops.add(stop);
      events[stop] = { ...events[stop], detail: message.detail, source: message.source, message_id: message.message_id };
      if (key) identities.add(key);
    } else {
      if (claudeReply) claudeReply.insertedAt = events.length;
      index(message, events.length);
      events.push(message);
    }
  }
  // Resolve all ordinary matches before considering any delayed Stop. For an
  // exact delayed match, keep the original journal row and drop its copy.
  const copies = delayedClaudeCopies(events, restored.length, claudeReplies, matchedStops);
  if (copies.size) events = events.filter((_, i) => !copies.has(i));
  // Preserve journal order when no row was inserted, or occurrence times are
  // incomplete. Fully timed rows use occurrence time with sequence tie breaks.
  if (events.length > restored.length && events.every((e) => eventAt(e) > 0)) {
    events.sort((a, b) => eventAt(a) - eventAt(b) || ((a.seq ?? 0) > 0 && (b.seq ?? 0) > 0 ? a.seq! - b.seq! : 0));
  }
  return events;
}

/**
 * The items of a transcript, oldest first; ids are stable while events only
 * append. `local` are the open session's local rows (the composer view's).
 */
/**
 * The service's submitted question routes of this session as timeline events
 * (zerus-sa9qn.33). A parent's own message of exactly the routed text folds
 * into its line, which keeps that text behind a disclosure; any other line is
 * placed by its time, after the events of the same time.
 */
export function mergeRoutes(events: readonly ActivityEvent[], routes: readonly RouteActivity[]): ActivityEvent[] {
  if (!routes.length) return events as ActivityEvent[];
  const out: DisplayEvent[] = events.slice();
  for (const route of [...routes].sort((a, b) => a.at - b.at)) {
    const line: RouteLine = { requestId: route.requestId, role: route.role, child: route.child.name, reference: route.child.reference,
      parent: route.parent.name, preview: route.question.preview, ...(route.outcome ? { outcome: route.outcome } : {}) };
    const event: DisplayEvent = { type: "QuestionRoute", at: route.at, activity_display_key: `route-${route.requestId}`, [ROUTE]: line };
    const text = route.text;
    const folds = text === undefined ? [] : out.flatMap((e, i) => !routeOf(e) && e.type !== "LocalMessage" && e.source !== "hgs_voice"
      && messageRole(e) === "user" && e.detail === text ? [i] : []);
    if (folds.length) {
      line.text = text;
      event.at = out[folds[0]].at;
      out[folds[0]] = event;
      for (const i of folds.slice(1).reverse()) out.splice(i, 1);
      continue;
    }
    let i = out.length;
    while (i > 0 && typeof out[i - 1].at === "number" && (out[i - 1].at as number) > route.at) i--;
    out.splice(i, 0, event);
  }
  return out;
}

export function buildTranscript(given: readonly ActivityEvent[], summary?: Record<string, unknown>, local: readonly LocalMessage[] = [], routes: readonly RouteActivity[] = []): TranscriptItem[] {
  const provider = agyDelivery(mergeAttachmentMessages(mergeProvider(mergeMessages(given, summary), summary), summary));
  if (!provider.length && !local.length && !routes.length) return recordedContext(summary);
  const events = mergeRoutes(mergeAttachmentMessages(local.length ? mergeLocal(provider, local) : provider, summary), routes);
  return transcriptItems(events, summary, !provider.length);
}

/** The index of the Stop that ends the keep-warm ping at `from`, or -1 while it runs or a person's prompt comes first. */
function pingStop(events: readonly ActivityEvent[], from: number): number {
  for (let k = from + 1; k < events.length; k++) {
    const event = events[k];
    if (event.turn_origin === "keep_warm") { if (event.type === "Stop") return k; continue; }
    if (messageRole(event) === "user") return -1;
  }
  return -1;
}

/** Journal identity stays intact in the store; only rendered rows adopt a send's key. */
function transcriptItems(events: readonly ActivityEvent[], summary?: Record<string, unknown>, context = false): TranscriptItem[] {
  const items: TranscriptItem[] = context ? recordedContext(summary) : [];
  // A fork's history from its sources (zerus-b4qk): each source's rows under
  // its own label, then a marker where this session's own activity begins.
  let source: string | undefined, marked = 0;
  const inherit = () => {
    if (source) for (const item of items.slice(marked)) if (item.kind !== "context") item.inherited = true;
    marked = items.length;
  };
  for (let i = 0; i < events.length;) {
    const e = events[i];
    const id = typeof e.activity_display_key === "string" ? e.activity_display_key : eventKey(e);
    if (e.inherited_from !== source) {
      inherit();
      if (e.inherited_from) items.push({ id: `inherited-${id}`, kind: "context", at: "", body: tr("Inherited from %1: the history this fork began from", e.inherited_from) });
      else if (source) items.push({ id: `forked-${id}`, kind: "context", at: "", body: tr("Forked here: this session's own activity continues below") });
      source = e.inherited_from;
      marked = items.length;
    }
    const role = messageRole(e);
    const at = timeText(e.at);
    const route = routeOf(e);
    if (route) {
      items.push({ id, kind: "receipt", at, sender: route.outcome === "not_sent" ? tr("Question not sent to %1", route.parent.split("/").at(-1) ?? route.parent) : route.outcome === "uncertain" ? tr("Question delivery to %1 uncertain", route.parent.split("/").at(-1) ?? route.parent) : route.role === "child" ? tr("Question sent to the parent session") : tr("Question from a child session"), route });
      i++; continue;
    }
    if (e.turn_origin === "keep_warm" || e.origin === "keep_warm") {
      // A keep-warm ping is not the person's message: its prompt, the reply
      // and its Stop are one dim row. An untagged provider copy of the reply
      // before the ping's Stop folds in too; a row without its prompt hides.
      const stop = pingStop(events, i);
      const lines: string[] = [];
      let j = i;
      for (; j < events.length; j++) {
        const n = events[j];
        const tagged = n.turn_origin === "keep_warm" || n.origin === "keep_warm";
        if (!tagged && !(stop >= 0 && j < stop && messageRole(n) === "assistant")) break;
        const body = (n.detail ?? "").trim();
        if (body && (n.origin === "keep_warm" || messageRole(n) === "assistant") && lines[lines.length - 1] !== body) lines.push(body);
        if (j === stop) { j++; break; }
      }
      if (e.origin === "keep_warm") items.push({ id, kind: "warm", at, title: tr("Cache kept warm"), lines });
      i = j; continue;
    }
    if (e.type === "SessionCleared") {
      items.push({ id, kind: "cleared", at, sender: tr("Session cleared"), body: tr("The agent no longer sees the activity above. New messages start with a fresh conversation context.") });
      i++; continue;
    }
    if (openCodeCompaction(e)) {
      items.push({ id, kind: "report", at, title: eventTitle(e.type), body: e.detail });
      i++; continue;
    }
    if (e.type === "AgentThinking") {
      if ((e.detail ?? "").trim()) items.push({ id, kind: "thinking", at, body: e.detail });
      i++; continue;
    }
    if (role === "notice") {
      const child = e.child_policy_advisory?.child?.name;
      if (e.type === "Notification" && e.source === "zerus_child_policy" && typeof child === "string" && child) {
        items.push({ id, kind: "system", at, sender: tr("Child provider refusal"),
          body: tr("The provider blocked a request in %1.", child) });
        i++; continue;
      }
      const report = typeof e.report === "string" ? e.report : "";
      if (e.origin === "subagent_report") items.push({ id, kind: "report", at, title: preview(e.detail) || tr("Subagent report"), body: report || e.detail, sender: e.sender || undefined });
      else items.push({ id, kind: "system", at, sender: e.origin === "peer_message" ? tr("Message from %1", e.sender ?? tr("another session")) : tr("Background task"), body: report || e.detail });
      i++; continue;
    }
    if (role === "user" && e.source === "hgs_voice") {
      items.push(media(e, id));
      i++; continue;
    }
    if (e.type === "LocalMessage" && role === "user") {
      // The stamp stands in for the time and the receipt (ActivityView.cpp:1053-1055).
      const row = e.local as LocalMessage;
      items.push({ id: `local-${row.id}`, kind: "user", at: "", body: row.text, attachments: attachments(e), local: localItem(row, summary) });
      i++; continue;
    }
    if (role) {
      let receipt: string | undefined;
      let receiptUnconfirmed = false;
      let inbox: InboxItem | undefined;
      let displayTime = at;
      // OpenCode reports its inbox: a waiting message is queued, and after
      // Stop it stays there, not delivered, until it is sent or removed
      // (ActivityView.cpp:1036-1038, 1072-1087).
      const delivery = role === "user" ? e.delivery_state : undefined;
      if (role === "user" && agyOffer(e)) {
        receiptUnconfirmed = delivery !== "submitted";
        receipt = receiptUnconfirmed ? tr("Delivery not confirmed") : tr("Sent");
      } else if (delivery === "queued" || delivery === "not_delivered") {
        receipt = delivery === "queued" ? tr("Queued") : tr("Not delivered");
        const messageId = typeof e.message_id === "string" ? e.message_id : "";
        const allowed = Array.isArray(summary?.allowed_actions) && summary.allowed_actions.includes("remove_queued");
        inbox = { state: delivery, messageId, run: String(summary?.run_id ?? ""), conversation: String(summary?.conversation_id ?? ""),
          removable: delivery === "not_delivered" && !!messageId && allowed };
        const stamp = e.queued_at;
        displayTime = e.source === "opencode_native" && /^msg_[a-zA-Z0-9_-]{1,124}$/.test(messageId)
          && !!inbox.conversation && !!inbox.run && typeof stamp === "number" && stamp > 0 && stamp <= 8.64e12
          ? tr("Queued at %1", timeText(stamp)) : tr("Queue time unavailable");
      } else if (role === "user") {
        let accepted = e.type !== "UserPromptQueued";
        for (let j = i + 1; !accepted && j < events.length; j++) {
          const next = events[j];
          if ((next.type === "UserPromptSubmit" || next.type === "TurnStarted") && next.detail === e.detail) accepted = true;
        }
        receipt = accepted ? tr("Sent") : tr("Queued at this time");
      }
      // An answer to a question card, or a message holding Codex replies (ActivityView.cpp:1036-1038).
      const replies = role === "user" && e.question_replies?.length ? e.question_replies : undefined;
      // Qt's order: a queued prompt first, then an answer (ActivityView.cpp:1036-1038).
      const label = role !== "user" ? undefined : e.type === "UserPromptQueued" ? tr("You (queued)")
        : e.type === "QuestionAnswered" || replies ? tr("You (answer)") : undefined;
      items.push({ id, kind: role === "user" ? "user" : "agent", at: displayTime, body: role === "user" ? ownMessageText(e) : e.detail ?? "",
        ...(role === "user" ? { bodyTruncated: ownMessageTruncated(e) } : {}),
        echoBody: typeof e.attachment_echo === "string" ? e.attachment_echo : undefined,
        attachments: attachments(e), receipt, label, replies, ...(receiptUnconfirmed ? { receiptUnconfirmed } : {}), ...(inbox ? { inbox } : {}) });
      i++; continue;
    }
    // Adjacent operational events form one group; messages and attention stay boundaries.
    const group = [e];
    i++;
    while (i < events.length && group.length < GROUP_LIMIT && events[i].type !== "AgentThinking" && events[i].type !== "SessionCleared" && !routeOf(events[i]) && !openCodeCompaction(events[i])
      && events[i].inherited_from === e.inherited_from
      && !messageRole(events[i]) && !attention(e) && !attention(events[i])) group.push(events[i++]);
    const tools: string[] = [];
    let calls = 0;
    for (const g of group) {
      if (g.tool && !tools.includes(g.tool)) tools.push(g.tool);
      if (g.type === "PreToolUse") calls++;
    }
    const last = group[group.length - 1];
    const toolEvents = group.map(toolEvent);
    const shownTools = tools.slice(0, 3).join(", ") + (tools.length > 3 ? " …" : "");
    if (group.length === 1) {
      items.push({ id, kind: "tool", at: timeText(last.at), title: `${eventTitle(e.type)}${e.tool ? ` / ${e.tool}` : ""}`, body: preview(e.detail), events: toolEvents, warn: attention(e) });
    } else {
      const running = group.filter(event => event.type === "ToolProgress").length;
      const count = calls === 1 ? tr("1 tool call") : tr("%1 tool calls", calls);
      const title = running > 0 && calls > 0 ? tr("%1, %2 running", count, running)
        : calls > 0 ? tr("%1, %2 events", count, group.length) : tr("%1 activity events", group.length);
      items.push({ id, kind: "tools", at: timeText(last.at), title, body: shownTools || preview(last.detail), events: toolEvents, warn: group.some(attention) });
    }
  }
  inherit();
  return items;
}

type OutboxIdentity = Pick<OutboxItem, "run_id" | "conversation_id">;

/** Only a bound receipt can take over a queued row's display identity. */
function outboxReceiptId(event: ActivityEvent, target?: OutboxIdentity): string | undefined {
  if (event.inherited_from || messageRole(event) !== "user" || !PROMPTS.has(event.type ?? "")) return undefined;
  if (target && ((event.run_id !== undefined && event.run_id !== target.run_id)
    || (event.conversation_id !== undefined && event.conversation_id !== target.conversation_id))) return undefined;
  const id = opencodePromptId(event) ? event.request_id
    : event.source === "hgs_delivery" ? event.message_id : event.attachment_request_id;
  return typeof id === "string" ? id : undefined;
}

/** One display target only. Remember at most 50 local receipts and 1000 small
 * journal/display-key bindings; never retain another session's transcript.
 * Rust may drop its local row before the corresponding activity batch arrives. */
export function createTranscriptProjection() {
  const recent = new Map<string, LocalMessage>(), bindings = new Map<string, string>(), outboxRequests = new Map<string, OutboxIdentity>();
  const trim = <T>(map: Map<string, T>, limit: number) => { while (map.size > limit) map.delete(map.keys().next().value!); };
  return {
    reset() { recent.clear(); bindings.clear(); outboxRequests.clear(); },
    build(given: readonly ActivityEvent[], summary?: Record<string, unknown>, local: readonly LocalMessage[] = [],
      routes: readonly RouteActivity[] = [], outbox: readonly OutboxItem[] = []): TranscriptItem[] {
      const sent = new Set(local.filter(row => row.status === "sent").map(row => row.id));
      const queued = outbox.filter(row => !sent.has(row.request_id));
      const queuedIds = new Set(queued.filter(row => !row.target_changed).map(row => row.request_id));
      for (const item of queued) if (queuedIds.has(item.request_id)) {
        outboxRequests.set(item.request_id, { run_id: item.run_id, conversation_id: item.conversation_id });
      }
      trim(outboxRequests, 50);
      const independent = new Set(local.filter(row => row.source === OPENCODE_SOURCE && row.status === "error")
        .map(row => `local-${row.id}`));
      // An early exact echo can share a pending row's key. A later failed or
      // uncertain local result needs its own row, while native history stays.
      if (independent.size) {
        for (const [key, alias] of bindings) if (independent.has(alias)) bindings.delete(key);
      }
      for (const row of local) { recent.set(row.id, row); }
      trim(recent, 50);
      const present = new Set(local.map(row => row.id));
      const bound = new Set(bindings.values());
      const missing = [...recent.values()].filter(row => !present.has(row.id) && row.status !== "error" && row.source !== "hgs_voice"
        // A retired in-flight row has no receipt identity. If Rust removed
        // it against native history, redisplaying it would duplicate that history.
        && (row.source !== OPENCODE_SOURCE || row.status === "sent"));
      // A confirmed local receipt can leave Rust's view one batch before its
      // journal event. Keep that one row through the handoff, without actions.
      const waiting = missing.filter(row => !bound.has(`local-${row.id}`));
      const bindingOnly = missing.filter(row => bound.has(`local-${row.id}`));
      const provider = agyDelivery(mergeAttachmentMessages(mergeProvider(mergeMessages(given, summary), summary), summary));
      const aliased: ActivityEvent[] = [];
      const seenOutbox = new Set<string>();
      for (const event of provider) {
        const request = outboxReceiptId(event);
        const key = eventKey(event);
        const target = request ? outboxRequests.get(request) : undefined;
        if (request && target && outboxReceiptId(event, target)) {
          // The queue remains the authority for its visible state until the
          // backing entry leaves. An echo or an early receipt never edits it.
          if (queuedIds.has(request) || seenOutbox.has(request)) continue;
          seenOutbox.add(request);
          bindings.set(key, `local-${request}`);
        }
        const id = bindings.get(key);
        aliased.push(id ? { ...event, activity_display_key: id } : event);
      }
      let events = mergeLocal(aliased, [...local, ...waiting], bindingOnly);
      events = events.map(event => {
        const key = typeof event.activity_journal_key === "string" ? event.activity_journal_key : eventKey(event);
        if (typeof event.activity_display_key === "string") bindings.set(key, event.activity_display_key);
        const id = bindings.get(key);
        return id ? { ...event, activity_display_key: id } : event;
      });
      trim(bindings, 1000);
      const items = transcriptItems(mergeRoutes(mergeAttachmentMessages(events, summary), routes), summary, !provider.length);
      // Waiting entries appear at the live end in FIFO order. Legacy
      // entries without a timestamp keep their place as well.
      for (const row of queued) items.push({ id: `local-${row.request_id}`, kind: "user", at: "", outbox: row });
      return items;
    },
  };
}

/**
 * The projections of one open Activity: the session's and its open child's.
 * Each starts over when its own display target changes, so a child shows no
 * other row's receipts and a brief visit to a child leaves its parent's handoff
 * in place. Showing another session drops both; the bound is each projection's.
 */
export function createActivityProjections() {
  const session = createTranscriptProjection(), child = createTranscriptProjection();
  let sessionTarget: string | undefined, childTarget: string | undefined;
  return (isChild: boolean, target: string | undefined, ...view: Parameters<typeof session.build>) => {
    if (isChild && target !== childTarget) { child.reset(); childTarget = target; }
    if (!isChild && target !== sessionTarget) { session.reset(); child.reset(); sessionTarget = target; }
    return (isChild ? child : session).build(...view);
  };
}
