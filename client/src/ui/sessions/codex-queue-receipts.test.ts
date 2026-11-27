import { describe, expect, it } from "vitest";
import { buildTranscript, createTranscriptProjection } from "./transcript";
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
    // The snapshot can beat the durable removal. Keep its pending state until removal commits.
    expect(p.build([], summary([receipt]), [], [], [waiting])).toMatchObject([{ id, outbox: waiting }]);
    const accepted = [{ id, kind: "receipt", label: "Accepted into the Codex queue", body: text, attachments: [{ request_id: request, index: 0 }] }];
    expect(p.build([], scope, [local])).toMatchObject(accepted);
    expect(p.build([], summary([receipt]), [])).toMatchObject(accepted);
    p.reset();
    expect(p.build([], summary([receipt]), [])).toMatchObject(accepted);
    expect(p.build([], summary([{ ...receipt, native_queue_state: "queued" }]), [])).toMatchObject([{ id, label: "Queued in Codex" }]);
    expect(p.build([], summary([{ ...receipt, native_queue_state: "consumed" }]), [])).toMatchObject([{ id, label: "Received by Codex" }]);
  });

  it("preserves equal accepted text-only messages and manual input independently, including old receipts without client IDs", () => {
    const repeated = { ...receipt, attachments: [], request_id: second, message_id: second, native_queued_message_id: client };
    const nativeEcho = { type: "UserPromptSubmit", seq: 3, at: 6, detail: text };
    const rows = buildTranscript([nativeEcho], summary([receipt, repeated]));
    expect(rows).toMatchObject([{ id, kind: "receipt", body: text }, { id: `local-${second}`, kind: "receipt", body: text }, { kind: "user", body: text }]);
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
