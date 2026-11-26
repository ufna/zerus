import { describe, expect, it } from "vitest";
import type { LocalMessage, QueueView } from "../../types";
import type { Session } from "./types";
import { cardTarget } from "./attachment-card";
import { queueEntries, queuePresentation, queuedMessages, sanitizedQueueText } from "./queue-message";

const text = "**Review** [Image #1]";
const submitted = '**Review** [Image #1] (Inspect the attached image at "/synthetic/attachments/message-one/01-shot.png")';
const file = { request_id: "receipt-one", index: 0, name: "shot.png", mime: "image/png", bytes: 1536, reference: "[Image #1]" };
const queue = (patch: Partial<QueueView> = {}): QueueView => ({ id: "native-screen-id", text: `❯ ${submitted}`,
  hint: "send now", enabled: true, can_send_now: true, label: "Send now", run_id: "run", conversation_id: "conversation", ...patch });
const local = (patch: Partial<LocalMessage> = {}): LocalMessage => ({ id: "receipt-one", text, status: "sent", attachments: [file],
  run_id: "run", conversation_id: "conversation", submitted_at: 1, error: "", uncertain: false,
  message_id: "receipt-one", submitted_text: submitted, source: null, voice: null, ...patch });
const summary = (patch: Record<string, unknown> = {}) => ({ run_id: "run", conversation_id: "conversation",
  attachment_messages: [{ source: "hgs_delivery", agent_id: "", message_id: "receipt-one", detail: text, submitted_text: submitted, attachments: [file] }], ...patch });

describe("native queued outgoing presentation", () => {
  it("removes native chrome while preserving Markdown and separate queued entries", () => {
    expect(queueEntries("❯ **one**\n  - item\n❯ `two`")).toEqual([
      { text: "**one**\n- item", exact: true }, { text: "`two`", exact: true },
    ]);
    expect(queueEntries("\u2022 Messages to be submitted after next tool call (press esc to\n  interrupt and send immediately)\n  ↳ **one**\n    continuation\n  ↳ two\nshift+← edit last queued message")).toEqual([
      { text: "**one**\ncontinuation", exact: true }, { text: "two", exact: true },
    ]);
    expect(queueEntries("↳ one\n  ↳ two").map(e => e.text)).toEqual(["one", "two"]);
    expect(queueEntries("\u001b[32m❯ **green**\u001b[0m")[0].text).toBe("**green**");
    expect(queueEntries("foreign first line\nsecond line")).toEqual([{ text: "foreign first line\nsecond line", exact: false }]);
  });

  it("previews full and clipped helper envelopes without adopting authority or disclosing reference attributes", () => {
    const prefix = '<zerus-message from="kimi/example/helper&amp;test" host="fixture" reference="zerus://session?';
    const text = `❯ ${prefix}machine=11111111-1111-4111-8111-111111111111&amp;host=fixture\n  &amp;name=kimi%2Fexample%2Fhelper%26test&amp;run=fixture-run">\n  Report &amp; review.\n  </zerus-message>`;
    const [item] = queuedMessages(queue({ text }), []);
    expect(item).toMatchObject({ sender: "kimi/example/helper&test on fixture", body: "Report & review." });
    expect(item.attachments).toBeUndefined();
    expect(item.local).toBeUndefined();
    expect(JSON.stringify(item)).not.toContain("zerus://");
    const [clipped] = queuedMessages(queue({ text: `❯ ${prefix}…` }), []);
    expect(clipped).toMatchObject({ sender: item.sender, body: "Message preview unavailable in Terminal" });
    const [unknown] = queuedMessages(queue({ text: '❯ <zerus-message from="kimi/example/hel…' }), []);
    expect(unknown).toMatchObject({ body: "Message preview unavailable in Terminal" });
    expect(unknown.sender).toBeUndefined();
  });

  it("uses an exact local send receipt for the original body and guarded attachment card", () => {
    const [item] = queuedMessages(queue(), [local()]);
    expect(item.body).toBe(text);
    expect(item.kind).toBe("user");
    expect(item.local).toBeUndefined(); // no sent/retry/delete actions for a native pending message
    expect(item.attachments?.[0]).toMatchObject({ name: "shot.png", request_id: "receipt-one", index: 0, image: true });
    expect(cardTarget(item.attachments![0], { computer: "", name: "codex/example/queue", conversation_id: "conversation", archive_id: null })?.ref.request_id).toBe("receipt-one");
    expect(JSON.stringify(item)).not.toContain("/synthetic/");
    expect(queue().id).toBe("native-screen-id"); // display reconciliation does not replace the Send now identity
  });

  it("keeps display identity independent of the screen hash while Send now retains its native ID", () => {
    const before = queue(), after = queue({ id: "screen-after-an-unrelated-native-spinner" });
    expect(queuedMessages(before, [local()])).toEqual(queuedMessages(after, [local()]));
    expect(before.id).not.toBe(after.id);
  });

  it("uses an exact host receipt and deduplicates the same receipt during local-to-host overlap", () => {
    expect(queuedMessages(queue(), [], summary())[0].body).toBe(text);
    expect(queuedMessages(queue(), [local()], summary())[0].attachments?.[0].request_id).toBe("receipt-one");
    const duplicate = summary();
    duplicate.attachment_messages.push({ ...duplicate.attachment_messages[0], message_id: "receipt-two", attachments: [{ ...file, request_id: "receipt-two" }] });
    expect(queuedMessages(queue(), [local()], duplicate)[0].attachments).toBeUndefined();
  });

  it("never matches a prefix, shortened entry, other run, uncertain send or duplicate queue body", () => {
    for (const [q, rows] of [
      [queue({ text: `❯ ${submitted.slice(0, 25)}` }), [local()]],
      [queue({ text: `❯ ${submitted}…` }), [local({ submitted_text: submitted + "…" })]],
      [queue(), [local({ run_id: "old" })]],
      [queue(), [local({ conversation_id: "old" })]],
      [queue(), [local({ uncertain: true })]],
      [queue(), [local({ status: "sending" })]],
      [queue({ text: `❯ ${submitted}\n❯ ${submitted}` }), [local()]],
      [queue({ text: `❯ ${submitted}\n  ❯ continuation resembling a prompt` }), [local()]],
    ] as const) {
      expect(queuedMessages(q, rows).every(item => item.attachments === undefined)).toBe(true);
    }
    expect(queuedMessages(queue(), [], summary({ conversation_id: "old" }))[0].attachments).toBeUndefined();
    expect(queuedMessages(queue({ run_id: undefined }), [local()])[0].attachments).toBeUndefined();
  });

  it("sanitizes unmatched attachment instructions without adopting or reading the host path", () => {
    const [item] = queuedMessages(queue(), []);
    expect(item.body).toBe(text);
    expect(item.attachments).toBeUndefined();
    expect(sanitizedQueueText('See [File #1] (Read the attached file at "/synthetic/note.txt")')).toBe("See [File #1]");
    expect(sanitizedQueueText('[File #1] Read the attached file at "/synthetic/note.txt"')).toBe("[File #1]");
    expect(sanitizedQueueText('Inspect the attached image at "/synthetic/shot.png"')).toBe("[Image]");
    expect(sanitizedQueueText('[Image #1] (Inspect the attached image at /synthetic/shot.png)')).toBe("[Image #1]");
  });

  it("refuses malformed or conflicting public file metadata", () => {
    for (const attachments of [[{ ...file, request_id: "other" }], [{ ...file, index: 4 }], [{ ...file, bytes: NaN }]]) {
      expect(queuedMessages(queue(), [local({ attachments })])[0].attachments).toBeUndefined();
    }
    const conflict = summary(); conflict.attachment_messages[0].attachments = [{ ...file, bytes: file.bytes + 1 }];
    expect(queuedMessages(queue(), [local()], conflict)[0].attachments).toBeUndefined();
  });
});

describe("native queue receipt source fences", () => {
  const session = { key: "\nrun:run", catalogKey: "run:run", computer: "", name: "codex/example/queue", runId: "run", conversationId: "conversation" } as Session;
  const view = { key: "\ncodex/example/queue", local: [local()] };
  const target = { id: session.key, key: session.catalogKey, computer: "", name: session.name };
  it("binds the main selected composer and its current Activity before supplying attachment scope", () => {
    const shown = queuePresentation(queue(), session, view, target, summary(), false);
    expect(shown.items[0].attachments?.[0].request_id).toBe("receipt-one");
    expect(shown.scope).toEqual({ computer: "", name: session.name, conversation_id: "conversation", archive_id: null });
    for (const [row, composer, child] of [
      [{ ...session, runId: "old" }, view, false],
      [{ ...session, conversationId: "old" }, view, false],
      [{ ...session, computer: "peer" }, view, false],
      [session, { ...view, key: "another" }, false],
      [session, { ...view, subagent: { agent_id: "child", send_supported: true, hint: "" } }, false],
      [session, view, true],
    ] as const) {
      const shown = queuePresentation(queue(), row, composer, target, summary(), child);
      expect(shown.scope).toBeUndefined();
      expect(shown.items[0].attachments).toBeUndefined();
    }
  });
  it("does not use a late host summary from a different follow, while local receipts remain independent", () => {
    const stale = { ...target, id: "other" };
    expect(queuePresentation(queue(), session, { ...view, local: [] }, stale, summary(), false).items[0].attachments).toBeUndefined();
    expect(queuePresentation(queue(), session, view, stale, summary(), false).items[0].attachments?.[0].request_id).toBe("receipt-one");
  });
});


it("keeps an unattributed native queue preview plain", () => {
  const [message] = queuedMessages(queue({ text: "❯ A plain queued message" }), []);
  expect(message.body).toBe("A plain queued message");
  expect(message.sender).toBeUndefined();
  expect(message.attachments).toBeUndefined();
});

it.each([
  ["different sender", "fixture", "kimi%2Fexample%2Fother"],
  ["different host", "other-host", "kimi%2Fexample%2Fhelper"],
  ["invalid encoded identity", "fixture", "kimi%2Fexample%2Fhelper%FF"],
  ["duplicate decoded identity", "fixture", "kimi%2Fexample%2Fhelper&amp;%6Eame=kimi%2Fexample%2Fhelper"],
])("drops attribution from a complete envelope with %s", (_reason, host, name) => {
  const ref = `zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=${host}&amp;name=${name}&amp;conversation=22222222-2222-4222-8222-222222222222`;
  const [message] = queuedMessages(queue({
    text: `❯ <zerus-message from="kimi/example/helper" host="fixture" reference="${ref}">\n  Text.\n  </zerus-message>`,
  }), []);
  expect(message).toMatchObject({ body: "Text.", sender: undefined, attachments: undefined });
  expect(message.body).not.toContain("zerus://");
});


it("checks a complete contradicting reference even when the outer host wrapped", () => {
  const ref = 'zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=another&amp;name=kimi%2Fexample%2Fhelper&amp;run=fixture';
  const [message] = queuedMessages(queue({
    text: `❯ <zerus-message from="kimi/example/helper" host="fix\n  ture" reference="${ref}">\n  Text.\n  </zerus-message>`,
  }), []);
  expect(message).toMatchObject({ body: "Text.", sender: undefined });
});


it.each([
  ["complete without a reference", '<zerus-message from="kimi/example/helper" host="fixture">'],
  ["contradicting reference after an extra attribute", '<zerus-message from="kimi/example/helper" host="fixture" extra="x" reference="zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=fixture&amp;name=kimi%2Fexample%2Fother&amp;run=fixture">'],
  ["agreeing reference followed by an extra attribute", '<zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=fixture&amp;name=kimi%2Fexample%2Fhelper&amp;run=fixture" extra="x">'],
])("leaves a malformed full opening unattributed and display-only (%s)", (_label, header) => {
  const native = queue({ text: `❯ ${header}\n  Text.\n  </zerus-message>` });
  const [message] = queuedMessages(native, []);
  expect(message).toMatchObject({ body: "Text.", sender: undefined, attachments: undefined });
  expect(message.local).toBeUndefined();
  expect(native.id).toBe("native-screen-id");
});

it.each([
  ['<zerus-message from="kimi/example/helper" ho…', "kimi/example/helper"],
  ['<zerus-message from="kimi/example/helper" host="fixture" refe…', "kimi/example/helper on fixture"],
  ['<zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=fixture&amp;name=kimi%2Fexample%2Fhelper&amp;run=fixture"…', "kimi/example/helper on fixture"],
  ['<zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?machine=11111&am…', "kimi/example/helper on fixture"],
  ['<zerus-message from="kimi/example/helper" host="fixture" extra="unfinished…', undefined],
  ['<zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?unexpected=…', undefined],
  ['<zerus-message from="kimi/example/helper" host="fixture" reference="unexpected input…', undefined],
])("attributes only an incomplete recognized opening (%s)", (text, sender) => {
  const [message] = queuedMessages(queue({ text: `❯ ${text}` }), []);
  expect(message).toMatchObject({ body: "Message preview unavailable in Terminal", sender, attachments: undefined });
});
