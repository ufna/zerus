import { describe, expect, it } from "vitest";
import { buildTranscript, createTranscriptProjection, timeText } from "./transcript";
import type { ActivityEvent, LocalMessage } from "../../types";
import type { OutboxItem } from "./types";

const request = "00000001-2222-4444-8888-000000000001";
const native = "00000002-2222-4444-8888-000000000002";
const client = "00000003-2222-4444-8888-000000000003";
const second = "00000004-2222-4444-8888-000000000004";
const scope = { run_id: "r", conversation_id: "c" };
const file = { request_id: request, index: 0, name: "notes.md", mime: "text/plain", bytes: 4, reference: "[File #1]" };
const text = "What did the tests show?";
const receipt = { ...scope, request_id: request, native_queued_message_id: native, type: "CodexQueueReceipt", status: "submitted",
  transport: "codex_queue", source: "hgs_delivery", message_id: request, at: 5, detail: text, attachments: [file] };
const summary = (rows: readonly ActivityEvent[]) => ({ ...scope, codex_queued_messages: rows });
const id = `local-${request}`;
const local: LocalMessage = { ...scope, id: request, message_id: native, source: "zerus_codex_queue", text, submitted_at: 5,
  status: "sent", attachments: [file], uncertain: false, error: "", submitted_text: null, voice: null };
const waiting: OutboxItem = { ...scope, handle: 1, request_id: request, preview: text, attachments: [file], state: "reserved" };

describe("Codex queue acceptance in Activity", () => {
  it("keeps one keyed message from pending through the native acknowledgement, snapshot and restart", () => {
    const p = createTranscriptProjection();
    expect(p.build([], scope, [], [], [waiting])).toMatchObject([{ id, outbox: waiting }]);
    // The exact queue acknowledgement can arrive before durable outbox settlement.
    expect(p.build([], summary([receipt]), [], [], [waiting])).toMatchObject([{ id, label: "Accepted into the Codex queue" }]);
    const accepted = [{ id, kind: "receipt", label: "Accepted into the Codex queue", body: text, attachments: [{ request_id: request, index: 0 }] }];
    expect(p.build([], scope, [local])).toMatchObject(accepted);
    expect(p.build([], summary([receipt]), [])).toMatchObject(accepted);
    p.reset();
    expect(p.build([], summary([receipt]), [])).toMatchObject(accepted);
    expect(p.build([], summary([{ ...receipt, codex_client_id: client, native_queue_state: "queued" }]), [])).toMatchObject([{ id, label: "Queued in Codex" }]);
    expect(p.build([], summary([{ ...receipt, native_queue_state: "consumed" }]), [])).toMatchObject([{ id, label: "Received by Codex" }]);
  });

  it("preserves equal accepted text-only messages and manual input independently, including old receipts without client IDs", () => {
    const repeated = { ...receipt, attachments: [], request_id: second, message_id: second, native_queued_message_id: client };
    const nativeEcho = { type: "UserPromptSubmit", seq: 3, at: 6, detail: text };
    const rows = buildTranscript([nativeEcho], summary([receipt, repeated]));
    expect(rows).toMatchObject([{ id, kind: "receipt", body: text }, { id: `local-${second}`, kind: "receipt", body: text }, { kind: "user", body: text }]);
  });

  it("keeps the exact still-pending row at the live end, then preserves its native position after consumption and restart", () => {
    const equalClient = "00000005-2222-4444-8888-000000000005";
    const equalFile = { ...file, request_id: second, name: "equal.md" };
    const pending = { ...receipt, codex_client_id: client, native_queue_state: "queued" };
    const consumed = { ...receipt, request_id: second, message_id: second, native_queued_message_id: second,
      codex_client_id: equalClient, at: 6, native_queue_state: "consumed", attachments: [equalFile] };
    const body = (message_id: string, codex_client_id: string) => ({ ...scope, type: "UserMessageBody",
      source: "codex_transcript", message_id, codex_client_id, detail: text });
    const echo = (message_id: string, seq: number) => ({ ...scope, type: "UserPromptSubmit", source: "codex_transcript",
      message_id, seq, at: seq, detail: text });
    const unrelated = Array.from({ length: 16 }, (_, i) => ({ ...echo(`unrelated-${i}`, 10 + 2 * i), detail: `Unrelated ${i + 1}` }));
    const otherEcho = echo("other-original", 9);
    const pendingSummary = { ...summary([pending, consumed]), user_message_bodies: [body("other-original", equalClient)] };
    const displayFile = ({ name, request_id, index, reference }: typeof file) => ({ name, request_id, index, caption: `${reference} ${name}` });
    const p = createTranscriptProjection();
    const checkPending = (events: ActivityEvent[], snapshot: Record<string, unknown>) => {
      const held = p.build(events, snapshot);
      expect(held).toHaveLength(18);
      expect(held[0]).toMatchObject({ id: `local-${second}`, body: text, attachments: [displayFile(equalFile)] });
      expect(held.at(-1)).toMatchObject({ id, kind: "receipt", label: "Queued in Codex", body: text, attachments: [displayFile(file)] });
      expect(held.at(-2)).toMatchObject({ body: "Unrelated 16" });
    };
    checkPending([otherEcho, ...unrelated], pendingSummary);

    const history = [otherEcho, ...unrelated.slice(0, 8), echo("pending-original", 25), ...unrelated.slice(8)];
    const bodies = [...pendingSummary.user_message_bodies, body("pending-original", client)];
    checkPending(history, { ...summary([pending, pending, consumed]), user_message_bodies: bodies });
    const consumedSummary = { ...summary([{ ...pending, native_queue_state: "consumed" }, consumed]),
      user_message_bodies: bodies };
    const checkHistory = () => {
      const settled = p.build(history, consumedSummary);
      expect(settled).toHaveLength(18);
      expect(settled.slice(8, 11)).toMatchObject([{ body: "Unrelated 8" },
        { id, kind: "user", body: text, receipt: "Sent", attachments: [displayFile(file)] }, { body: "Unrelated 9" }]);
      expect(settled.filter(row => row.id === id)).toHaveLength(1);
      expect(settled.at(-1)).toMatchObject({ body: "Unrelated 16" });
    };
    checkHistory();
    p.reset();
    checkHistory();
  });

  it("keeps local-only, unproved and ambiguously owned receipts in their historical order", () => {
    const later = { type: "UserPromptSubmit", seq: 3, at: 8, detail: "Later" };
    const queuedMark = { ...receipt, native_queue_state: "queued" };
    const secondOwner = { ...queuedMark, request_id: second, message_id: second, codex_client_id: second, attachments: [] };
    for (const rows of [
      buildTranscript([later], scope, [local]),
      buildTranscript([later], summary([queuedMark])),
      buildTranscript([later], summary([{ ...queuedMark, codex_client_id: client }, secondOwner])),
      buildTranscript([later], summary([{ ...queuedMark, codex_client_id: client },
        { ...secondOwner, native_queued_message_id: second, codex_client_id: client }])),
    ]) {
      expect(rows[0]).toMatchObject({ id, kind: "receipt", label: "Accepted into the Codex queue" });
      expect(rows.at(-1)).toMatchObject({ body: "Later" });
      expect(rows.some(row => row.kind === "receipt" && row.label === "Queued in Codex")).toBe(false);
    }
  });

  it("places exact consumed bodies at their native times before the journal echoes arrive", () => {
    const otherClient = "00000005-2222-4444-8888-000000000005";
    const fileReceipt = { ...receipt, codex_client_id: client, native_queue_state: "consumed" };
    const otherReceipt = { ...fileReceipt, request_id: second, message_id: second, native_queued_message_id: second,
      codex_client_id: otherClient, at: 6, attachments: [] };
    const nativeBody = (codex_client_id: string, at: number) => ({ type: "UserMessageBody", source: "codex_transcript",
      agent_id: "", conversation_id: scope.conversation_id, message_id: `codex-client:${codex_client_id}`,
      codex_client_id, at, detail: "Native text including private attachment path" });
    const fileBody = nativeBody(client, 180), otherBody = nativeBody(otherClient, 240);
    const s = { ...summary([fileReceipt, otherReceipt]), user_message_bodies: [fileBody, otherBody] };
    const before = { type: "AgentMessage", seq: 1, at: 120, detail: "Earlier native reply" };
    const between = { type: "AgentMessage", seq: 3, at: 210, detail: "Intervening native reply" };
    const echo = { ...fileBody, ...scope, type: "UserPromptSubmit", seq: 4, at: 220 };
    const expected = [{ kind: "agent", body: before.detail },
      { id, kind: "user", at: timeText(fileBody.at), body: text, receipt: "Sent", attachments: [{ request_id: request, index: 0, name: file.name }] },
      { kind: "agent", body: between.detail }, { id: `local-${second}`, kind: "user", at: timeText(otherBody.at), body: text }];
    const p = createTranscriptProjection();
    expect(p.build([before, between], s)).toMatchObject(expected);
    expect(p.build([before, between, echo], s)).toMatchObject(expected);
    p.reset();
    expect(p.build([before, between], s)).toMatchObject(expected);
  });

  it("requires a unique timed own body and leaves contradictory echoes untouched", () => {
    const durable = { ...receipt, codex_client_id: client, native_queue_state: "consumed" };
    const body = { type: "UserMessageBody", source: "codex_transcript", conversation_id: scope.conversation_id,
      message_id: `codex-client:${client}`, codex_client_id: client, at: 20, detail: text };
    const s = { ...summary([durable]), user_message_bodies: [body] };
    const fallback = (snapshot: Record<string, unknown>, events: ActivityEvent[] = []) => {
      expect(buildTranscript(events, snapshot).filter(row => row.id === id)).toMatchObject([{ kind: "receipt" }]);
    };
    for (const changed of [
      { at: undefined }, { at: NaN }, { at: Infinity }, { at: 0 }, { message_id: "" },
      { source: "other" }, { codex_client_id: second }, { conversation_id: "other" }, { run_id: "other" },
      { agent_id: "child" }, { inherited_from: "old" }, { origin: "task_notification" }, { turn_origin: "keep_warm" },
    ]) fallback({ ...s, user_message_bodies: [{ ...body, ...changed }] });
    for (const duplicate of [body, { ...body, message_id: "another" }, { ...body, codex_client_id: second }]) {
      fallback({ ...s, user_message_bodies: [body, duplicate] });
    }
    for (const native_queue_state of ["queued", undefined]) {
      fallback({ ...s, codex_queued_messages: [{ ...durable, native_queue_state }] });
    }
    const echo = { ...scope, type: "UserPromptSubmit", source: "codex_transcript", seq: 1, at: 21, message_id: body.message_id,
      detail: text };
    for (const changed of [{ codex_client_id: second }, { run_id: "other" }, { conversation_id: "other" },
      { source: "other" }, { inherited_from: "old" }, { attachment_request_id: second }]) fallback(s, [{ ...echo, ...changed }]);
    fallback(s, [echo, { ...echo, seq: 2 }]);
    const unrelated = { ...echo, message_id: "another" };
    expect(buildTranscript([unrelated], s)).toMatchObject([{ id, kind: "user", body: text }, { id: "s1", kind: "user", body: text }]);
  });

  it("transfers a receipt and files to one exactly bound turn with the same display key", () => {
    const durable = { ...receipt, codex_client_id: client };
    const body = { type: "UserMessageBody", source: "codex_transcript", conversation_id: "c",
      message_id: "item-id", codex_client_id: client, detail: text };
    const echo = { ...scope, type: "UserPromptSubmit", seq: 4, at: 7, source: "codex_transcript", message_id: "item-id", detail: "raw native text" };
    const unrelated = { ...echo, seq: 3, at: 6, message_id: "another", detail: text };
    const s = { ...summary([durable]), user_message_bodies: [body] };
    expect(buildTranscript([unrelated, echo], s)).toMatchObject([{ kind: "user", body: text }, { id, kind: "user", body: text, receipt: "Sent", attachments: [{ name: file.name }] }]);
    for (const wrong of [{ ...echo, conversation_id: "other" }, { ...echo, inherited_from: "other" }, { ...echo, source: "other" }]) {
      expect(buildTranscript([wrong], s).filter(i => i.kind === "receipt")).toMatchObject([{ id, body: text }]);
    }
    const ambiguous = { ...s, user_message_bodies: [body, { ...body, message_id: "another" }] };
    expect(buildTranscript([unrelated, echo], ambiguous).filter(i => i.kind === "receipt")).toHaveLength(1);
  });

  it("rejects invalid proofs, retains tool boundaries and keeps legacy file matching off these receipts", () => {
    const bad = [{ ...receipt, source: "other" }, { ...receipt, run_id: "other" }, { ...receipt, request_id: native },
      { ...receipt, native_queued_message_id: "" }, { ...receipt, attachments: [{ ...file, index: 1 }] }];
    expect(buildTranscript([], summary(bad))).toEqual([]);
    const tool = { type: "PostToolUse", seq: 1, at: 4 };
    expect(buildTranscript([tool, { ...tool, seq: 2, at: 7 }], summary([receipt])).map(i => i.kind)).toEqual(["tool", "receipt", "tool"]);
    const raw = '[File #1] (Read the attached file at "/tmp/attachments/message-test/01-notes.md")';
    const fileReceipt = { ...receipt, codex_outbox: true, submitted_text: raw };
    expect(buildTranscript([{ type: "UserPromptSubmit", detail: raw }], { ...summary([receipt]), attachment_messages: [fileReceipt] })
      .find(i => i.kind === "user")).toMatchObject({ body: raw, attachments: undefined });
  });
});
