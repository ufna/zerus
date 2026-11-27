// @vitest-environment happy-dom
import { readFileSync, writeFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";

const transport = vi.hoisted(() => ({
  enablesTeams: false,
  language: "en" as "en" | "ru",
  composer: undefined as ((command: string, args?: Record<string, unknown>) => unknown) | undefined,
  parent: vi.fn(async (..._args: unknown[]) => ({} as Record<string, unknown>)),
}));
vi.mock("../../i18n", async (original) => {
  const i18n = await original<typeof import("../../i18n")>();
  const ru = (await import("../../i18n.ru.json")).default;
  return { ...i18n, uiLanguage: () => transport.language, tr: (source: string, ...args: (string | number)[]) =>
    transport.language === "ru" ? i18n.translateWithCatalog(ru, "ru", source, ...args) : i18n.tr(source, ...args) };
});
vi.mock("@tauri-apps/api/core", async (original) => {
  const core = await original<typeof import("@tauri-apps/api/core")>();
  return { ...core, invoke: (command: string, args?: Record<string, unknown>) => {
    if (transport.composer) return Promise.resolve(transport.composer(command, args));
    if (transport.enablesTeams && (command === "teams_hidden" || command === "teams_follow")) return Promise.resolve(undefined);
    return core.invoke(command, args);
  } };
});

vi.mock("../../bridge", async (original) => {
  const bridge = await original<typeof import("../../bridge")>();
  // Run the real command wrappers through the recorded Tauri IPC, while
  // keeping unrelated replay-mode subscriptions on their regular test path.
  const withDesktop = <A extends unknown[], R>(invoke: (...args: A) => R) => (...args: A): R => {
    if (!transport.composer) return invoke(...args);
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    try { return invoke(...args); }
    finally { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); }
  };
  return {
    ...bridge,
    composerEdit: withDesktop(bridge.composerEdit), sendMessage: withDesktop(bridge.sendMessage),
    composerRemoveWaiting: withDesktop(bridge.composerRemoveWaiting),
    runAction: withDesktop(bridge.runAction),
    queueSendNow: withDesktop(bridge.queueSendNow), localRetry: withDesktop(bridge.localRetry),
    localInspect: withDesktop(bridge.localInspect), sendAnswer: withDesktop(bridge.sendAnswer), composerAllowRetry: withDesktop(bridge.composerAllowRetry),
    inTauri: () => transport.enablesTeams, setParent: transport.parent,
    activityPort: { follow: vi.fn(async () => {}), unfollow: vi.fn(async () => {}) },
    terminalStatus: vi.fn(async () => ({ supported: true, available: false, reason: "Private fixture", idle: "Fixture terminal", connecting: "Connecting", disconnected: "Disconnected" })),
    inspectSession: vi.fn(async () => null), composerOpen: vi.fn(async () => null),
    sessionFolderStatus: vi.fn(async () => undefined), followTasks: vi.fn(async () => {}),
    reportDetailInvariant: vi.fn(async () => {}),
    archivePage: vi.fn(async () => ({ rows: [], next_cursor: "", previous_cursor: null, total: 0, position: 0 })),
    archivedActivity: vi.fn(async () => ({ events: [], replace_events: true, conversation_id: "archived-conversation", run_id: "archived-run", cleared_conversations: [], cursor: 0, summary: {} })),
  };
});
// The actual TerminalTab and its store still mount. Only xterm's canvas/OS
// renderer is inert in happy-dom; no terminal is connected or receives input.
vi.mock("@xterm/xterm", () => ({ Terminal: class {
  cols = 80; rows = 24; options: Record<string, unknown>; textarea?: HTMLTextAreaElement;
  constructor(options: Record<string, unknown>) { this.options = options; }
  loadAddon() {} onData() {} onBinary() {} onResize() {} attachCustomKeyEventHandler() {} write(_bytes: unknown, done?: () => void) { done?.(); }
  open(host: HTMLElement) { this.textarea = document.createElement("textarea"); host.append(this.textarea); }
  focus() { this.textarea?.focus(); } dispose() { this.textarea?.remove(); } reset() {} paste() {}
} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));

import * as bridge from "../../bridge";
import { App } from "../../App";
import { receive, activity } from "../../store/session";
import { selected, setSelectedKey, setDetailTab, detailTab, setSubagentOpen, setMenu, menu, setStateFilter, setSearch } from "../../store/ui";
import { inspectorTab, type InspectorTab } from "../../store/inspector";
import { settingsReady, setSetting, restoreSettings } from "../../store/settings";
import { openCleanup, closeCleanup } from "../../store/terminationCleanup";
import { machineMeta } from "../../store/testing";
import { listRows } from "./model";
import { archive } from "../../store/archive";
import { terminal } from "../../store/terminal";
import { clearSearchPreview, setSearchPreview } from "../../store/searchSelection";
import type { SearchHit } from "../../store/content-search-model";
import { emptyBatch, type ComposerView, type LocalMessage, type ViewRow } from "../../types";
import { draftKey } from "../../store/draftKeys";
import contact from "../../../core/src/teams/fixtures/person-lead-contact.json";

const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const machine = "11111111-1111-4111-8111-111111111111";
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
let serial = 0, dispose: (() => void) | undefined;
let host: HTMLDivElement;

beforeEach(async () => {
  await settingsReady;
  vi.clearAllMocks();
  vi.mocked(bridge.inspectSession).mockResolvedValue(null);
  vi.mocked(bridge.composerOpen).mockResolvedValue(null);
  vi.mocked(bridge.sessionFolderStatus).mockResolvedValue(undefined as never);
  await setSetting("processes/enabled", true, { persist: false });
  await setSetting("workspace/inspectorOpen", true, { persist: false });
  setStateFilter("all"); setSubagentOpen(undefined);
  host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(App, { mode: () => "replay" }), host);
});
afterEach(() => {
  closeCleanup(); setMenu(undefined);
  host.querySelector<HTMLDialogElement>("dialog.dlg")?.dispatchEvent(new Event("close"));
  dispose?.(); dispose = undefined; document.body.replaceChildren();
  setSelectedKey(undefined); setSubagentOpen(undefined); setStateFilter("all");
  receive({ ...emptyBatch(), replaces: [{ computer: "", rows: [] }] });
  restoreSettings();
  transport.composer = undefined;
  transport.language = "en";
});

function fixtureRow(title = "Fixture"): ViewRow {
  const n = ++serial;
  return { name: `codex/example/fixture-${n}`, title, project: "Example", agent: "codex", status: "ready", caption: "Ready", state: "running", tracked: true,
    run_id: `original-${n}`, conversation_id: `conversation-${n}`, session_id: uuid(n), cwd: "/srv/example/fixture" };
}
function put(row: ViewRow) {
  const key = `run:${row.run_id}`;
  receive({ ...emptyBatch(), computers: [{ computer: "", status: "live", attempt: 0 }],
    upserts: [{ computer: "", key, row }] });
  return `\n${key}`;
}
function remove(key: string) {
  receive({ ...emptyBatch(), removes: [{ computer: "", key: key.slice(1) }],
    activity: [{ computer: "", key: key.slice(1), kind: "ended", payload: {} }] });
}
async function open(row: ViewRow, tab: "activity" | "processes" | "terminal" = "activity", side: InspectorTab = "plan") {
  const key = put(row); setSelectedKey(key); await flush();
  await setSetting("workspace/inspectorTab", side, { persist: false });
  setDetailTab(tab); await vi.dynamicImportSettled(); await flush();
  expect(detailTab()).toBe(tab); expect(inspectorTab()).toBe(side);
  expect(host.querySelector(`.${tab === "activity" ? "act-scroll" : tab === "terminal" ? "term-tab" : "proc-tab"}`)).not.toBeNull();
  expect(host.querySelector(".detail-head")).not.toBeNull();
  expect(host.querySelector(".inspector")).not.toBeNull();
  activity.applyActivity([{ computer: "", key: key.slice(1), kind: "state", payload: { rev: 1, value: { run_id: row.run_id, conversation_id: row.conversation_id, activity: "idle", phase: "idle", activity_summary: "Ready" } } }]);
  return key;
}

it("ignores an outdated OpenCode terminal status for the same selected row", async () => {
  const row = { ...fixtureRow(), name: "opencode/example/terminal-status", agent: "opencode" };
  const key = put(row);
  const status = { supported: true, available: false, reason: "Current fixture", idle: "Fixture terminal", connecting: "Connecting", disconnected: "Disconnected" };
  let resolveOld!: (result: typeof status) => void;
  vi.mocked(bridge.terminalStatus)
    .mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }))
    .mockResolvedValueOnce(status);
  const setTarget = vi.spyOn(terminal, "setTarget");
  onTestFinished(() => setTarget.mockRestore());

  setSelectedKey(key);
  await flush();
  expect(resolveOld).toBeTypeOf("function");
  expect(put({ ...row, conversation_id: "new-conversation" })).toBe(key);
  await flush();
  expect(setTarget).toHaveBeenLastCalledWith(expect.objectContaining({ conversation_id: "new-conversation" }), status, "opencode");
  const targetCalls = setTarget.mock.calls.length;

  resolveOld({ ...status, reason: "Outdated fixture" });
  await flush();
  expect(setTarget).toHaveBeenCalledTimes(targetCalls);
});

const queueDraftLabels = {
  en: { send: "Send", queue: "Send queued messages now", retry: "Retry this message", notSent: "Not sent",
    check: "Check Terminal", allow: "Allow retry", notConfirmed: "Delivery not confirmed",
    wait: "Your draft is kept. Wait for the agent to accept the earlier message, or check Terminal.",
    cleared: "The earlier input is no longer pending. Your draft is kept.",
    unobserved: "Your draft is kept. Check Terminal. Send and Retry stay paused until this host confirms the earlier input cleared. Update hgs on older hosts.",
    confirmSend: "Send anyway", confirmCompact: "Compact and send",
    origin: "Sender shown by agent: kimi/example/helper on fixture" },
  ru: { send: "Отправить", queue: "Отправить сообщения из очереди сейчас",
    retry: "Повторить отправку сообщения", notSent: "Не отправлено", check: "Проверить терминал",
    allow: "Разрешить повторную попытку", notConfirmed: "Доставка не подтверждена",
    wait: "Черновик сохранён. Дождитесь, пока агент примет предыдущее сообщение, или проверьте терминал.",
    cleared: "Предыдущий ввод больше не ожидает подтверждения. Черновик сохранён.",
    unobserved: "Черновик сохранён. Проверьте терминал. Отправка и повтор недоступны, пока компьютер не подтвердит, что предыдущий ввод больше не ожидает подтверждения. На старых компьютерах обновите hgs.",
    confirmSend: "Всё равно отправить", confirmCompact: "Сжать и отправить",
    origin: "Агент показывает отправителя: kimi/example/helper на fixture" },
};

type SubmissionWire = {
  name: string;
  run_id: string;
  conversation_id: string;
  input_queue: Pick<NonNullable<ComposerView["queue"]>, "id" | "text" | "hint" | "can_send_now">;
  pending_zerus_submission: { request_id: string; run_id: string; conversation_id: string; submitted_at: number; sender: string };
};
const wirePath = process.env.HGS_TEST_SUBMISSION_WIRE;
const nativeWire = wirePath ? JSON.parse(readFileSync(wirePath, "utf8")) as SubmissionWire : undefined;

it.each(["en", "ru"] as const)("keeps a synthetic manager's native queue and refused draft independent in %s", async (lang) => {
  transport.language = lang;
  // Happy DOM has no layout. Give the real Activity virtualizer a viewport.
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(700);
  onTestFinished(() => { height.mockRestore(); width.mockRestore(); });
  const labels = queueDraftLabels[lang];
  const row = fixtureRow("Synthetic manager");
  const selectedRow = { ...row, agent: "kimi", name: nativeWire?.name ?? "kimi/example/manager",
    run_id: nativeWire?.run_id ?? row.run_id, status: "working", caption: "Busy" };
  const key = draftKey({ computer: "", key: `run:${selectedRow.run_id}`, name: selectedRow.name,
    session_id: selectedRow.session_id, archive_id: null });
  // The selected catalog lies one conversation behind the Activity target.
  const actualConversation = nativeWire?.conversation_id ?? "fixture-current-conversation";
  const queueId = "synthetic-native-queue", failedId = uuid(99901);
  const helperBody = "Synthetic helper report.";
  const draft = "Please summarize the synthetic findings.";
  const queue: ComposerView["queue"] = {
    id: queueId, hint: "Steer the agent with the queued messages now", label: "Send now", enabled: true, can_send_now: true,
    run_id: selectedRow.run_id!, conversation_id: actualConversation,
    text: '❯ <zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?machine=11111111-1111-4111-8111-111111111111&amp;host=fixture&amp;name=kimi%2Fexample%2Fhelper&amp;run=fixture-run">\n    '
      + helperBody + '\n    </zerus-message>',
  };
  const detail = "Message was not sent. hgs refused before typing: previous input has not been confirmed by the agent; a busy agent may keep it queued until it accepts it. Send again once it does, or inspect Terminal";
  const failed: LocalMessage = { id: failedId, text: draft, status: "error", attachments: [], run_id: selectedRow.run_id!,
    conversation_id: actualConversation, submitted_at: 1, error: detail, uncertain: false,
    message_id: null, submitted_text: null, source: null, voice: null };
  const initial: ComposerView = {
    handle: 117, key, loaded: true, rev: 0, text_epoch: 1, text: "", position: 0, anchor: 0, attachments: [], removed: null,
    attaching: false, sending: false, waiting: null, uncertain: false, status: "", status_error: false, can_send: false,
    send_label: { source: "Send" }, available: true, unavailable_reason: "", block_reason: null, attachments_available: true,
    attachments_reason: "", session_status: "", session_status_error: false, session_blocks_input: false, inspect_error: false,
    cache: { visible: false, tone: "quiet", label: "", short_label: "" }, cache_warning: "", cache_details: "",
    context: { visible: false, text: "", tooltip: "", warning: false, tone: "quiet", percent: null, accessible_name: "" },
    local: [], compacting: false, cancel_send: false, input_visible: true, question: null, queue,
    clear: { enabled: false, dismiss: false, title: "", text: "", confirm: "", run: "", conversation: "",
      cache_enabled: false, running: false, reason: "", cache_reason: "" },
    previous_message_pending: false,
  };
  vi.mocked(bridge.composerOpen).mockImplementation(async (s) => s?.key === `run:${selectedRow.run_id}` ? initial : null);
  const commands: Array<[string, Record<string, unknown> | undefined]> = [];
  let cold = false;
  transport.composer = (name, args) => {
    commands.push([name, args]);
    if (name === "send_message" || name === "local_retry") {
      if (!cold) return { kind: "sending" };
      return { kind: "cold", token: 55, prompt: { title: "Fixture cache", text: "", informative: "", choices: [
        { action: "send", label: labels.confirmSend, enabled: true, tooltip: "" },
        { action: "compact", label: labels.confirmCompact, enabled: true, tooltip: "" },
      ] } };
    }
    if (name === "send_answer") return { kind: "sending" };
    if (name === "queue_send_now") return true;
    if (name === "local_inspect") return { kind: "terminal" };
    return undefined;
  };
  const sentBy = (name: string) => commands.filter(([command]) => command === name).map(([, args]) => args);
  const button = (text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")]
    .find((b) => b.textContent?.trim() === text);
  const draftSend = () => host.querySelector<HTMLButtonElement>(".cmp-actions .primary")!;
  const draftState = () => host.querySelector(".cmp-draft-status")?.textContent;
  const localActions = () => host.querySelector(".act-local-actions");
  const publish = (update: Partial<ComposerView>) => receive({ ...emptyBatch(), composer: { ...initial, ...update } });
  const activityKey = await open(selectedRow);
  activity.applyActivity([{ computer: "", key: activityKey.slice(1), kind: "state",
    payload: { rev: 2, value: { run_id: selectedRow.run_id, conversation_id: actualConversation,
      activity: "busy", phase: "working" } } }]);
  expect(selected()?.conversationId).toBe(selectedRow.conversation_id);
  const editor = () => host.querySelector<HTMLTextAreaElement>(".cmp-editor")!;
  await vi.waitFor(() => expect(editor().disabled).toBe(false));
  editor().value = draft;
  editor().setSelectionRange(draft.length, draft.length);
  editor().dispatchEvent(new Event("input", { bubbles: true }));
  expect(draftSend().disabled).toBe(false);
  draftSend().click();
  await vi.waitFor(() => expect(sentBy("send_message")).toHaveLength(1));
  expect(sentBy("send_message")[0]).toMatchObject({ handle: 117, text: draft });
  await flush();

  const refusal = { text: draft, text_epoch: 2, status: detail, status_error: true, local: [failed], earlier_input_held: true };
  publish({ ...refusal, previous_message_pending: true });
  await vi.waitFor(() => expect(localActions()).not.toBeNull());
  expect(host.querySelector(".act-queue-source")?.textContent).toBe(labels.origin);
  expect(host.querySelector(".act-queue-text")?.textContent).toContain(helperBody);
  expect(host.querySelector(".act-queue-text")?.textContent).not.toContain("zerus://");
  const unavailable = lang === "ru" ? "Время постановки в очередь неизвестно" : "Queue time unavailable";
  expect(host.querySelector(".act-queue-time")?.textContent).toBe(unavailable);
  const legacyQueue = { ...queue,
    text: '❯ <zerus-message from="kimi/example/helper" host="fixture" reference="zerus://session?…\n❯ Another item',
    entry_times: [{ row: 0, queued_at: 1792271374 }] };
  publish({ ...refusal, previous_message_pending: true, queue: legacyQueue });
  expect([...host.querySelectorAll(".act-queue-time")].map(node => node.textContent)).toEqual([unavailable, unavailable]);
  expect(host.querySelector(".act-queue-source")?.textContent).toBe(labels.origin);
  publish({ ...refusal, previous_message_pending: true });
  expect(host.textContent).toContain(labels.notSent);
  expect(draftState()).toBe(labels.wait);
  expect(editor().value).toBe(draft);
  expect(editor().readOnly).toBe(false);
  expect(draftSend().getAttribute("aria-label")).toBe(labels.send);
  expect(draftSend().disabled).toBe(true);
  expect(button(labels.retry)?.getAttribute("aria-disabled")).toBe("true");
  expect(button(labels.check)).toBeDefined();
  expect(button(labels.allow)).toBeUndefined();
  publish({ ...refusal, previous_message_pending: true, queue: { ...queue, can_send_now: false, enabled: false } });
  expect(button(labels.queue)?.disabled).toBe(true);
  expect(button(labels.queue)?.title).not.toContain(queue.hint);
  button(labels.queue)!.click();
  expect(sentBy("queue_send_now")).toEqual([]);
  publish({ ...refusal, previous_message_pending: true });
  button(labels.retry)!.click();
  editor().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  button(labels.queue)!.click();
  await vi.waitFor(() => expect(sentBy("queue_send_now")).toEqual([{ queue_id: queueId }]));
  expect(sentBy("send_message")).toHaveLength(1);
  expect(sentBy("local_retry")).toEqual([]);
  expect(editor().value).toBe(draft);

  publish({ ...refusal, previous_message_pending: true, queue: null });
  expect(button(labels.queue)).toBeUndefined();
  expect(draftSend().disabled).toBe(true);
  expect(draftState()).toBe(labels.wait);
  publish({ ...refusal, previous_message_pending: undefined, queue: null });
  expect(draftState()).toBe(labels.unobserved);
  expect(draftSend().disabled).toBe(true);
  expect(button(labels.retry)?.getAttribute("aria-disabled")).toBe("true");
  expect(editor().readOnly).toBe(false);
  draftSend().click();
  button(labels.retry)!.click();
  editor().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flush();
  expect(sentBy("send_message")).toHaveLength(1);
  expect(sentBy("local_retry")).toEqual([]);

  const released = { ...refusal, earlier_input_held: false, previous_message_pending: false, queue: null };
  publish(released);
  expect(draftState()).toBe(labels.cleared);
  expect(draftSend().disabled).toBe(false);
  expect(button(labels.retry)?.getAttribute("aria-disabled")).toBeNull();
  expect(sentBy("local_retry")).toEqual([]);
  expect(sentBy("send_message")).toHaveLength(1);
  cold = true;
  if (lang === "en") {
    button(labels.retry)!.click();
    expect(draftSend().disabled).toBe(true);
    draftSend().click();
    await vi.waitFor(() => expect(sentBy("local_retry")).toEqual([{ handle: 117, id: failedId }]));
    expect(sentBy("send_message")).toHaveLength(1);
  } else {
    draftSend().click();
    expect(button(labels.retry)?.getAttribute("aria-disabled")).toBe("true");
    button(labels.retry)!.click();
    await vi.waitFor(() => expect(sentBy("send_message")).toHaveLength(2));
    expect(sentBy("send_message")[1]).toMatchObject({ handle: 117, text: draft });
    expect(sentBy("local_retry")).toEqual([]);
  }

  await vi.waitFor(() => expect(button(labels.confirmSend)?.disabled).toBe(false));
  publish({ ...refusal, previous_message_pending: undefined, queue: null });
  expect(button(labels.confirmSend)?.disabled).toBe(true);
  expect(button(labels.confirmCompact)?.disabled).toBe(true);
  button(labels.confirmSend)!.click();
  button(labels.confirmCompact)!.click();
  await flush();
  expect(sentBy("send_answer")).toEqual([]);
  publish(released);
  expect(button(labels.confirmSend)?.disabled).toBe(false);
  button(labels.confirmSend)!.click();
  await vi.waitFor(() => expect(sentBy("send_answer")).toEqual([{ token: 55, answer: { kind: "cold", action: "send" } }]));

  const unknown = { ...failed, id: uuid(99902), uncertain: true, error: "Delivery not confirmed" };
  publish({ ...released, text_epoch: 3, uncertain: true,
    status: unknown.error, local: [unknown] });
  await vi.waitFor(() => expect(button(labels.allow)).toBeDefined());
  expect(host.textContent).toContain(labels.notConfirmed);
  expect(button(labels.retry)).toBeUndefined();
  expect(draftSend().disabled).toBe(true);
  button(labels.check)!.click();
  await vi.waitFor(() => expect(sentBy("local_inspect")).toEqual([{ handle: 117, id: unknown.id }]));
  expect(sentBy("composer_allow_retry")).toEqual([]);
  await vi.waitFor(() => expect(detailTab()).toBe("terminal"));
  setDetailTab("activity");
  await flush();
  expect(editor().value).toBe(draft);

  const submitted = nativeWire?.pending_zerus_submission ?? {
    request_id: uuid(99903), run_id: selectedRow.run_id, conversation_id: actualConversation,
    submitted_at: +new Date(2026, 10, 26, 9, 7) / 1000, sender: "codex/example/helper",
  };
  const clipped = nativeWire?.input_queue ?? { ...queue, text:
    '❯ <zerus-message from="codex/example/helper" host="fixture" reference="zerus://session?…' };
  const showState = (rev: number, submission?: typeof submitted, pending = true) => activity.applyActivity([{
    computer: "", key: activityKey.slice(1), kind: "state",
    payload: { rev, value: { run_id: selectedRow.run_id, conversation_id: actualConversation,
      activity: "busy", phase: "working", previous_message_pending: pending, pending_zerus_submission: submission } },
  }]);
  const submissionTime = () => host.querySelector<HTMLTimeElement>(".act-zerus-submission time");
  const surface = () => ({
    submission: host.querySelector(".act-zerus-submission")?.textContent ?? null,
    submitted_at: submissionTime()?.dateTime ?? null,
    queue: host.querySelector(".act-queue")?.textContent ?? null,
    native_timestamp_count: host.querySelectorAll(".act-queue time").length,
  });
  publish({ ...refusal, previous_message_pending: true, queue: {
    ...queue, ...clipped, enabled: clipped.can_send_now, label: "Send now",
  } });
  showState(3);
  expect(submissionTime()).toBeNull(); // older host with the same queue
  expect(host.querySelector(".act-queue-text")?.textContent).not.toContain("zerus://");
  const before = surface();
  showState(4, submitted);
  expect(submissionTime()).toBeNull(); // Activity and catalog disagree for this run
  put({ ...selectedRow, conversation_id: actualConversation });
  await flush();
  publish({ ...refusal, previous_message_pending: true, queue: {
    ...queue, ...clipped, enabled: clipped.can_send_now, label: "Send now",
  } });
  expect(surface().queue).toBe(before.queue);
  const iso = new Date(submitted.submitted_at * 1000).toISOString();
  expect(submissionTime()?.dateTime).toBe(iso);
  expect(submissionTime()?.textContent).toMatch(lang === "ru" ? /^Отправка Zerus записана / : /^Zerus submission recorded at /);
  expect(host.querySelector(".act-zerus-submission")?.textContent).toContain(submitted.sender);
  expect(host.querySelector(".act-queue time")).toBeNull();
  if (nativeWire && process.env.HGS_TEST_SUBMISSION_RENDER) {
    writeFileSync(`${process.env.HGS_TEST_SUBMISSION_RENDER}-${lang}.json`,
      JSON.stringify({ before, after: surface() }, null, 2) + "\n");
  }
  publish({ ...refusal, previous_message_pending: true, queue: null });
  expect(submissionTime()?.dateTime).toBe(iso);
  put({ ...selectedRow, conversation_id: "new-catalog-conversation" });
  expect(submissionTime()).toBeNull();
  put({ ...selectedRow, conversation_id: actualConversation });
  showState(5, { ...submitted, conversation_id: "switched-away" });
  expect(submissionTime()).toBeNull();
  showState(6, submitted, false);
  expect(submissionTime()).toBeNull();
  showState(7);
  put(selectedRow);
  publish({ ...refusal, previous_message_pending: true });
});

it.each(["en", "ru"] as const)("uses the same single Send for click and Enter in %s", async (lang) => {
  transport.language = lang;
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(700);
  onTestFinished(() => { height.mockRestore(); width.mockRestore(); });
  const row = { ...fixtureRow("Busy fixture"), status: "working", caption: "Busy" };
  const key = draftKey({ computer: "", key: `run:${row.run_id}`, name: row.name,
    session_id: row.session_id, archive_id: null });
  const initial: ComposerView = {
    handle: 219, key, loaded: true, rev: 0, text_epoch: 1, text: "", position: 0, anchor: 0, attachments: [], removed: null,
    attaching: false, sending: false, waiting: null, uncertain: false, status: "", status_error: false, can_send: false,
    send_label: { source: "Send" }, available: true, unavailable_reason: "", block_reason: null, attachments_available: true,
    attachments_reason: "", session_status: "", session_status_error: false, session_blocks_input: false, inspect_error: false,
    cache: { visible: false, tone: "quiet", label: "", short_label: "" }, cache_warning: "", cache_details: "",
    context: { visible: false, text: "", tooltip: "", warning: false, tone: "quiet", percent: null, accessible_name: "" },
    local: [], compacting: false, cancel_send: false, input_visible: true, question: null, queue: null,
    clear: { enabled: false, dismiss: false, title: "", text: "", confirm: "", run: "", conversation: "",
      cache_enabled: false, running: false, reason: "", cache_reason: "" },
    offers_outbox: true, previous_message_pending: true,
  };
  vi.mocked(bridge.composerOpen).mockImplementation(async (s) => s?.key === `run:${row.run_id}` ? initial : null);
  const commands: string[] = [];
  const removals: Array<Record<string, unknown> | undefined> = [];
  let removedView: ComposerView | undefined;
  const localQueue: NonNullable<ComposerView["waiting_in_zerus"]> = [];
  transport.composer = (name, args) => {
    commands.push(name);
    if (name === "composer_remove_waiting") {
      removals.push(args);
      return removedView;
    }
    if (name !== "send_message") return undefined;
    const index = localQueue.length;
    localQueue.push({ request_id: uuid(99000 + index), preview: String(args?.text), state: "waiting",
      run_id: row.run_id!, conversation_id: row.conversation_id!,
      queued_at: index < 2 ? new Date(2026, 10, 26, 14, 8, 9 + index).getTime() / 1000 : undefined,
      attachments: index === 0 ? [{ name: "queue-fixture.txt", mime: "text/plain", reference: "", bytes: 9 }] : [] });
    return { kind: "accepted", waiting: true, view: { ...initial,
      rev: Number(args?.rev), text_epoch: Number(args?.epoch) + 1, waiting_in_zerus: [...localQueue] } };
  };
  await open(row);
  const editor = () => host.querySelector<HTMLTextAreaElement>(".cmp-editor")!;
  const draftSend = () => host.querySelector<HTMLButtonElement>(".cmp-actions .primary")!;
  const labels = queueDraftLabels[lang];
  await vi.waitFor(() => expect(editor().readOnly).toBe(false));
  const oldHost = "Update hgs on this computer to queue messages. The draft stays in the editor.";
  const oldHostLabel = lang === "ru"
    ? "Обновите hgs на этом компьютере, чтобы ставить сообщения в очередь. Черновик остался в редакторе."
    : oldHost;
  receive({ ...emptyBatch(), composer: { ...initial, offers_outbox: false, ordinary_send_block: oldHost } });
  editor().value = "kept on the older host";
  editor().dispatchEvent(new Event("input", { bubbles: true }));
  expect(draftSend().disabled).toBe(true);
  expect(draftSend().title).toBe(oldHostLabel);
  expect(host.querySelector(".cmp-status")?.textContent).toBe(oldHostLabel);
  editor().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flush();
  expect(commands.filter(name => name === "send_message")).toHaveLength(0);
  expect(editor().value).toBe("kept on the older host");
  receive({ ...emptyBatch(), composer: initial });
  for (const [index, text] of ["same", "same", "third"].entries()) {
    editor().value = text;
    editor().dispatchEvent(new Event("input", { bubbles: true }));
    expect(draftSend().textContent).toBe(labels.send);
    expect(draftSend().disabled).toBe(false);
    expect(host.querySelectorAll(".cmp-actions .primary")).toHaveLength(1);
    expect([...host.querySelectorAll("button")].some(button => /Queue draft|Поставить черновик в очередь/.test(button.textContent ?? ""))).toBe(false);
    if (index === 1) editor().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    else draftSend().click();
    await vi.waitFor(() => expect(editor().value).toBe(""));
  }
  expect(localQueue.map(entry => entry.preview)).toEqual(["same", "same", "third"]);
  expect(commands.filter(name => name === "send_message")).toHaveLength(3);
  expect(commands).not.toContain("composer_enqueue");
  expect(commands).not.toContain("queue_send_now");
  const waitingRows = () => [...host.querySelectorAll(".act-scroll article.act-outbox")];
  const visibleOrder = () => waitingRows().map(item => item.id);
  await vi.waitFor(() => expect(waitingRows()).toHaveLength(3));
  expect(visibleOrder()).toEqual(localQueue.map(item => `item-local-${item.request_id}`));
  expect(waitingRows().map(item => item.querySelector(".act-outbox-preview")?.textContent)).toEqual(["same", "same", "third"]);
  expect(waitingRows()[0].querySelector(".act-outbox-files")?.textContent).toContain("queue-fixture.txt");
  const firstTime = waitingRows()[0].querySelector("time")!;
  expect(firstTime.dateTime).toBe(new Date(localQueue[0].queued_at! * 1000).toISOString());
  expect(firstTime.textContent).toContain("14:08:09");
  expect(waitingRows()[1].querySelector("time")?.textContent).toContain("14:08:10");
  expect(waitingRows()[2].querySelector("time")).toBeNull();
  expect(waitingRows()[0].querySelector(".act-stamp")?.textContent?.trim()).toBe(lang === "ru" ? "В очереди Zerus" : "Queued in Zerus");
  expect(host.querySelector(".cmp-draft-status summary")).toBeNull();
  expect(host.querySelector(".cmp-hint")?.textContent).toBe(lang === "ru" ? "Поставлено в очередь Zerus." : "Queued in Zerus.");

  editor().value = "fourth draft still editable";
  editor().dispatchEvent(new Event("input", { bubbles: true }));
  const confirmed: LocalMessage = {
    id: localQueue[0].request_id, text: "same", status: "sent", attachments: localQueue[0].attachments ?? [],
    run_id: row.run_id!, conversation_id: row.conversation_id!, submitted_at: localQueue[0].queued_at! + 5,
    error: "", uncertain: false, message_id: localQueue[0].request_id, submitted_text: "same", source: "zerus_outbox", voice: null,
  };
  const firstId = waitingRows()[0].id;
  const staged = { ...initial, text_epoch: 4, waiting_in_zerus: localQueue.slice(1), local: [confirmed] };
  receive({ ...emptyBatch(), composer: staged });
  await vi.waitFor(() => expect(waitingRows()).toHaveLength(2));
  const sent = [...host.querySelectorAll(".act-scroll article")].filter(item => item.id === firstId);
  expect(sent).toHaveLength(1);
  expect(sent[0].classList.contains("act-outbox")).toBe(false);
  expect(sent[0].textContent).toContain("queue-fixture.txt");
  expect(editor().value).toBe("fourth draft still editable");
  expect(editor().readOnly).toBe(false);

  const held = { ...staged, waiting_in_zerus: [
    { ...localQueue[1], state: "unknown" as const, can_check_terminal: true }, localQueue[2],
  ] };
  receive({ ...emptyBatch(), composer: held });
  const removeButton = () => waitingRows().at(-1)?.querySelector<HTMLButtonElement>(".act-outbox-actions .icon-btn");
  const confirm = () => host.querySelector<HTMLButtonElement>(".act-outbox-confirm button:last-child");
  removeButton()!.click();
  const staleConfirmation = confirm()!;
  expect(staleConfirmation).not.toBeNull();

  const other = fixtureRow("Other session");
  const otherKey = draftKey({ computer: "", key: `run:${other.run_id}`, name: other.name,
    session_id: other.session_id, archive_id: null });
  const reopened = { ...held, handle: held.handle + 2, text: "fourth draft still editable" };
  vi.mocked(bridge.composerOpen).mockImplementation(async (selection) =>
    selection?.key === `run:${row.run_id}` ? reopened
      : selection?.key === `run:${other.run_id}` ? { ...initial, handle: initial.handle + 1, key: otherKey } : null);
  await open(other);
  expect(waitingRows()).toHaveLength(0);
  expect(confirm()).toBeNull();
  staleConfirmation.click();
  expect(removals).toHaveLength(0);

  await open(row);
  await vi.waitFor(() => expect(waitingRows()).toHaveLength(2));
  expect(confirm()).toBeNull();
  expect(visibleOrder()).toEqual(localQueue.slice(1).map(item => `item-local-${item.request_id}`));
  removeButton()!.click();
  expect(confirm()).not.toBeNull();
  removedView = { ...reopened, waiting_in_zerus: [reopened.waiting_in_zerus[0]] };
  confirm()!.click();
  await vi.waitFor(() => expect(waitingRows()).toHaveLength(1));
  expect(removals).toEqual([{ handle: reopened.handle, request_id: localQueue[2].request_id }]);
  expect(editor().value).toBe("fourth draft still editable");
  const checkTerminal = () => [...host.querySelectorAll<HTMLButtonElement>(".act-scroll button")]
    .find(button => button.textContent?.trim() === labels.check);
  put({ ...row, conversation_id: "successor-conversation" });
  await vi.waitFor(() => expect(checkTerminal()).toBeUndefined());
  await open(row);
  await vi.waitFor(() => expect(checkTerminal()).toBeDefined());
  const check = checkTerminal();
  expect(check).toBeDefined();
  expect(waitingRows()[0].querySelector(".act-stamp")?.textContent?.trim()).toBe(labels.notConfirmed);
  check!.click();
  await vi.waitFor(() => expect(detailTab()).toBe("terminal"));
  expect(commands.filter(name => name === "send_message")).toHaveLength(3);

});

const submissionLifecycleCases = [
  { lang: "en", phase: "ended" }, { lang: "ru", phase: "ended" },
  { lang: "en", phase: "disconnected" }, { lang: "ru", phase: "disconnected" },
  { lang: "en", phase: "reconnected" }, { lang: "ru", phase: "reconnected" },
] as const;

it.each(submissionLifecycleCases)("requires current Activity state for a pending submission after $phase in $lang", async ({ lang, phase }) => {
  transport.language = lang;
  const row = fixtureRow("Submission lifecycle");
  const key = (await open(row)).slice(1);
  const { run_id, conversation_id } = row;
  const history = { seq: 1, type: "prompt", detail: "Synthetic retained transcript" };
  const submitted = { request_id: uuid(90001), run_id, conversation_id, submitted_at: 1795684020, sender: "codex/example/helper" };
  const summary = { run_id, conversation_id, previous_message_pending: true, pending_zerus_submission: submitted };
  const state = (rev: number, value = summary, target = key) => ({ computer: "", key: target, kind: "state", payload: { rev, value } });
  const connection = (ready: boolean) => ({ computer: "", key, kind: "connection", payload: { ready } });
  const assertTranscript = () => {
    expect(activity.events()).toEqual([history]);
    expect(activity.transcriptSummary()?.run_id).toBe(run_id);
  };
  const card = () => host.querySelector<HTMLElement>(".act-zerus-submission");
  const stamp = () => card()?.querySelector("time")?.dateTime;
  const expected = new Date(submitted.submitted_at * 1000).toISOString();
  const name = lang === "ru" ? "Отправка Zerus ожидает подтверждения" : "Pending Zerus submission";
  receive({ ...emptyBatch(), activity: [
    { computer: "", key, kind: "replace", payload: { events: [history] } },
    state(2),
  ] });
  expect(card()?.getAttribute("aria-label")).toBe(name);
  expect(stamp()).toBe(expected);

  const ended = { computer: "", key, kind: "ended", payload: null };
  const lifecycle = phase === "ended" ? [ended] : phase === "disconnected" ? [connection(false)] : [connection(false), connection(true)];
  receive({ ...emptyBatch(), activity: lifecycle });
  expect(card()).toBeNull();
  assertTranscript();

  if (phase !== "reconnected") {
    // A delayed state with this key while the old follow is retired is still stale.
    const stale = { ...summary, pending_zerus_submission: { ...submitted, sender: "codex/example/old-follow" } };
    receive({ ...emptyBatch(), activity: [state(3, stale)] });
    expect(card()).toBeNull();
    expect(activity.transcriptSummary()).toMatchObject(summary);
    assertTranscript();
  }
  if (phase === "ended") {
    const next = fixtureRow("Replacement follow");
    const nextKey = (await open(next)).slice(1);
    receive({ ...emptyBatch(), activity: [state(4)] });
    expect(card()).toBeNull();
    const nextReceipt = { ...submitted, run_id: next.run_id, conversation_id: next.conversation_id, request_id: uuid(90002) };
    receive({ ...emptyBatch(), activity: [state(2, { run_id: next.run_id, conversation_id: next.conversation_id,
      previous_message_pending: true, pending_zerus_submission: nextReceipt }, nextKey)] });
    expect(stamp()).toBe(expected);
    return;
  }
  if (phase === "disconnected") {
    receive({ ...emptyBatch(), activity: [connection(true)] });
    expect(card()).toBeNull();
    assertTranscript();
  }
  // The replacement connection can start its revision stream over.
  receive({ ...emptyBatch(), activity: [state(1)] });
  expect(card()?.getAttribute("aria-label")).toBe(name);
  expect(stamp()).toBe(expected);
  assertTranscript();
});

it.each(["en", "ru"] as const)("renders only exact OpenCode inbox times in %s", async (lang) => {
  transport.language = lang;
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  onTestFinished(() => height.mockRestore());
  const row = { ...fixtureRow("Private OpenCode inbox"), agent: "opencode", name: "opencode/example/inbox" };
  const key = (await open(row)).slice(1);
  const at = (minute: number) => +new Date(2026, 10, 3, 2, minute) / 1000;
  const inbox = (id: string, minute: number, timed = true) => ({
    message_id: id, source: "opencode_native", type: "UserPromptSubmit", delivery_state: "queued",
    at: at(minute), queued_at: timed ? at(minute) : undefined, detail: "Identical private preview",
  });
  const update = (events: ReturnType<typeof inbox>[]) => activity.applyActivity([
    { computer: "", key, kind: "replace", payload: { events } },
  ]);
  const shown = () => Array.from(host.querySelectorAll(".act-msg.k-user .act-time"), node => node.textContent);
  const expected = (minute: number) => lang === "ru" ? `В очереди с 3 ноября 02:0${minute}` : `Queued at Nov 3 02:0${minute}`;
  const unavailable = lang === "ru" ? "Время постановки в очередь неизвестно" : "Queue time unavailable";
  update([inbox("msg_old", 3), inbox("msg_same", 7), inbox("msg_legacy", 8, false)]);
  await vi.waitFor(() => expect(shown()).toEqual([expected(3), expected(7), unavailable]));
  update([inbox("msg_same", 7), inbox("msg_legacy", 8, false), inbox("msg_new", 9)]);
  await vi.waitFor(() => expect(shown()).toEqual([expected(7), unavailable, expected(9)]));
});

it.each(["en", "ru"] as const)("opens ordered OpenCode compaction reports in %s", async (lang) => {
  transport.language = lang;
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  onTestFinished(() => height.mockRestore());
  await (await import("./markdown")).loadMarkdown();
  const row = { ...fixtureRow("Private OpenCode reports"), agent: "opencode", name: "opencode/example/reports" };
  const key = (await open(row)).slice(1);
  const native = (type: string, id: string, detail = "", tool = "") => ({
    type, source: "opencode_native", activity_key: `opencode:${id}`, message_id: id,
    at: 1793000000, detail, tool,
  });
  const tool = native("PreToolUse", "msg_tool:call", "printf output", "shell");
  const running = native("ToolProgress", "msg_tool:call:result", "", "shell");
  const summary = "## Objective\nSynthetic work\n\n### Next steps\n- Verify the fixture";
  const update = (...events: ReturnType<typeof native>[]) => activity.applyActivity([
    { computer: "", key, kind: "replace", payload: { events } },
  ]);
  const reports = () => [...host.querySelectorAll<HTMLElement>(".act-notice")];
  const button = (index: number) => reports()[index].querySelector("button")!;
  update(tool, running, native("PreCompact", "msg_compact1"));
  await vi.waitFor(() => expect(reports()).toHaveLength(1));
  expect(reports()[0].textContent).toContain(lang === "ru" ? "Сжатие контекста" : "Compacting context");
  expect(button(0)).toBeNull();
  const first = reports()[0].id;
  const toolTitle = lang === "ru" ? "1 вызов инструмента, выполняется: 1" : "1 tool call, 1 running";
  expect(host.querySelector(".act-tools-head")?.textContent).toContain(toolTitle);
  const finished = native("PostToolUse", "msg_tool:call:result", "public result", "shell");
  const completed = native("PostCompact", "msg_compact1", summary);
  const second = native("PostCompact", "msg_compact2", summary);
  update(tool, finished, completed, native("AgentMessage", "msg_reply", "After the first report"), second);
  await vi.waitFor(() => expect(reports()).toHaveLength(2));
  expect(reports().map(report => report.id)).toEqual([first, "item-kopencode:msg_compact2"]);
  expect(reports()[0].textContent).toContain(lang === "ru" ? "Контекст сжат" : "Context compacted");
  expect(reports()[0].querySelector("h2")).toBeNull();
  button(0).click();
  await vi.waitFor(() => expect(button(0).getAttribute("aria-expanded"), reports()[0].outerHTML).toBe("true"));
  await vi.waitFor(() => expect(reports()[0].querySelector("h2")?.textContent, reports()[0].outerHTML).toBe("Objective"));
  expect(reports()[0].querySelector("h3")?.textContent).toBe("Next steps");
  expect(reports()[0].querySelector("li")?.textContent).toBe("Verify the fixture");
  expect(button(0).getAttribute("aria-expanded")).toBe("true");
  expect(button(1).getAttribute("aria-expanded")).toBe("false");
  update(tool, finished, completed, native("AgentMessage", "msg_reply", "After the first report"), second);
  expect(button(0).getAttribute("aria-expanded")).toBe("true");
});

function expectEmpty() {
  expect(selected()).toBeUndefined();
  expect(host.querySelector(".detail-empty")).not.toBeNull();
  for (const selector of [".detail-head", ".detail-tabs", ".inspector"]) expect(host.querySelector(selector), selector).toBeNull();
  expect(bridge.reportDetailInvariant).not.toHaveBeenCalled();
}

it("offers a keyboard-accessible Show in session list button immediately after the conversation title", async () => {
  await open(fixtureRow());
  const title = host.querySelector(".detail-head h1")!;
  const button = title.nextElementSibling as HTMLButtonElement | null;
  expect(button?.tagName).toBe("BUTTON");
  expect(button?.getAttribute("aria-label")).toBe("Show in session list");
  expect(button?.getAttribute("data-tip")).toBe("Show in session list");
  expect(button?.disabled).toBe(false);
  expect(button?.tabIndex).toBe(0);
});

it("shows a new-name error in Rename and keeps valid inner spaces", async () => {
  await open(fixtureRow());
  const { actionEntries } = await import("./ActionDialogs");
  actionEntries(selected()!).rename.action(); await flush();
  const input = host.querySelector<HTMLInputElement>("#rename-tag")!;
  const submit = host.querySelector<HTMLButtonElement>("dialog form button[type=submit]")!;
  input.value = "bad$tag"; input.dispatchEvent(new Event("input", { bubbles: true })); await flush();
  expect(host.querySelector(".dlg-error")?.textContent).toBe("Names cannot contain shell syntax or invisible formatting characters.");
  expect(submit.disabled).toBe(true);
  input.value = "safe name"; input.dispatchEvent(new Event("input", { bubbles: true })); await flush();
  expect(host.querySelector(".dlg-error")).toBeNull();
  expect(submit.disabled).toBe(false);
});

const routes = (["activity", "processes", "terminal"] as const).flatMap((tab) =>
  (["plan", "details", "worktrees", "tasks"] as const).map((side) => ({ tab, side })));
it.each(routes)("retires the complete $tab / $side frame together when the catalog row disappears", async ({ tab, side }) => {
  const key = await open(fixtureRow(), tab, side);
  const parts = [".detail-head", ".detail-tabs", ".inspector"].map((selector) => host.querySelector(selector)!);
  const removed: Node[] = [];
  const observer = new MutationObserver((records) => records.forEach((record) => removed.push(...record.removedNodes)));
  observer.observe(host, { subtree: true, childList: true });
  remove(key); await flush(); observer.disconnect(); expectEmpty();
  // This is the new defensive ownership invariant, not a reproduction of the
  // historical screenshot: one removed DOM ancestor contains every old part.
  expect(removed.some((node) => parts.every((part) => node.contains(part)))).toBe(true);
});

it("keeps header, transcript, focus and inspector continuity on ordinary metadata updates", async () => {
  const row = fixtureRow(); await open(row);
  const head = host.querySelector(".detail-head"), scroll = host.querySelector<HTMLElement>(".act-scroll")!, inspector = host.querySelector(".inspector");
  scroll.focus(); put({ ...row, title: "Updated title", summary: "Updated metadata" }); await flush();
  expect(host.querySelector(".detail-head")).toBe(head);
  expect(host.querySelector(".act-scroll")).toBe(scroll);
  expect(host.querySelector(".inspector")).toBe(inspector);
  expect(host.querySelector(".detail-head h1")!.textContent).toContain("Updated title");
  expect(document.activeElement).toBe(scroll);
  expect(bridge.reportDetailInvariant).not.toHaveBeenCalled();
});

it.each(["terminate dialog", "cleanup panel", "pending replies"])("clears the detail with %s still outstanding", async (stage) => {
  const replies: (() => void)[] = [];
  vi.mocked(bridge.sessionFolderStatus).mockImplementation(() => new Promise((resolve) => replies.push(() => resolve(undefined as never))));
  vi.mocked(bridge.inspectSession).mockImplementation(() => new Promise((resolve) => replies.push(() => resolve(null))));
  const key = await open(fixtureRow());
  if (stage === "terminate dialog") {
    host.querySelector<HTMLButtonElement>('.detail-head button[data-tip="Session actions"]')!.click(); await flush();
    const archive = [...host.querySelectorAll<HTMLButtonElement>(".menu-item")].find((button) => button.textContent === "Stop and archive…")!;
    expect(archive.classList.contains("danger")).toBe(true);
    expect(menu()!.items).toContainEqual(expect.objectContaining({ label: "Stop and archive…", icon: "archive" }));
    archive.click(); await flush();
    expect(host.querySelector("#action-dlg-title")!.textContent).toBe("Terminate session?");
    expect(host.querySelector('dialog[aria-labelledby="action-dlg-title"] button.solid-danger')?.textContent).toBe("Stop and archive");
  } else if (stage === "cleanup panel") {
    openCleanup(); await vi.dynamicImportSettled(); await flush();
    expect(host.querySelector(".worktree-cleanup[open]")).not.toBeNull();
  }
  expect(replies.length).toBeGreaterThan(0);
  remove(key); await flush(); replies.forEach((reply) => reply()); await flush(); expectEmpty();
});

it.each([false, true])("removes the selected parent or preserves its selected child (child selected %s)", async (childSelected) => {
  const parent = fixtureRow("Parent"), child = fixtureRow("Child");
  child.parent = { machine, session_id: parent.session_id!, name: parent.name!, conversation_id: parent.conversation_id!, run_id: parent.run_id!, state: "live" };
  const parentKey = put(parent), childKey = put(child);
  receive({ ...emptyBatch(), machines: [{ computer: "", machine: machineMeta("fixture", {}, { machine_id: machine }) }],
    grouping: { loaded: true, default_project: "ungrouped", headers: [{ id: "example", name: "Example", color: null, accessible: true, collapsed: false, is_default: false, palette: 0 }], ready: true, reason: null, busy: false, status: "", status_error: false, conflicts: 0 },
    groups: [{ computer: "", key: parentKey.slice(1), group: "example", rank: 1 }, { computer: "", key: childKey.slice(1), group: "example", rank: 2 }] });
  setSelectedKey(childSelected ? childKey : parentKey); setDetailTab("activity"); await flush();
  expect(listRows().find((r) => r.type === "card" && r.session.key === childKey)).toMatchObject({ depth: 1 });
  remove(parentKey); await flush();
  if (childSelected) {
    expect(selected()?.key).toBe(childKey);
    expect(host.querySelector(".detail-head h1")!.textContent).toContain("Child");
    expect(host.querySelector(".detail-empty")).toBeNull();
    expect(listRows().find((r) => r.type === "card" && r.session.key === childKey)).not.toMatchObject({ depth: 1 });
  } else expectEmpty();
  remove(childKey); await flush(); expectEmpty();
});

it("keeps normal recorded archive browsing outside the live catalog", async () => {
  const liveKey = await open(fixtureRow());
  const originalFrame = host.querySelector(".detail-head")!.parentElement;
  const id = uuid(++serial), key = `\narchive:${id}`;
  vi.mocked(bridge.archivePage).mockResolvedValue({ rows: [{ group: "example", rank: 1, name: "codex/example/archived", agent: "codex", state: "archived", status: "ended", archive_id: id, archived_at: 100, run_id: "archived-run", conversation_id: "archived-conversation" }], next_cursor: "", previous_cursor: null, total: 1, position: 0 } as never);
  setStateFilter("archived"); archive.refresh(); await flush();
  setSelectedKey(key); setDetailTab("activity"); await vi.dynamicImportSettled(); await flush();
  expect(selected()?.archiveId).toBe(id);
  expect(host.querySelector(".detail-head h1")!.textContent).toContain("archived");
  expect(host.querySelector(".inspector")).not.toBeNull();
  expect(host.querySelector(".detail-empty")).toBeNull();
  expect(host.querySelector(".detail-head")!.parentElement).toBe(originalFrame);
  remove(liveKey); await flush();
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  expect(bridge.reportDetailInvariant).not.toHaveBeenCalled();
  const unrelated = fixtureRow("Unrelated"), unrelatedKey = put(unrelated); remove(unrelatedKey); await flush();
  expect(selected()?.key).toBe(key);
  expect(host.querySelector(".detail-head h1")!.textContent).toContain("archived");
  expect(bridge.reportDetailInvariant).not.toHaveBeenCalled();
});


it("keeps the moved row actions in the real session header menu", async () => {
  await open(fixtureRow());
  const button = host.querySelector<HTMLButtonElement>('button[data-tip="Session actions"]')!;
  button.click(); await flush();
  const labels = menu()!.items.flatMap((item) => "label" in item ? [item.label] : []);
  for (const label of ["Expand all collapsed", "Open folder", "Open shell in session folder", "Pause session", "Fork session…", "Set parent…", "Copy session name", "Copy session reference", "Copy folder path"]) {
    expect(labels, label).toContain(label);
  }
});

describe("Set parent across projects", () => {
  const computer = "00000000-0000-4000-8000-000000000099";

  async function openChoices() {
    const child = fixtureRow("Child"), parent = fixtureRow("Actual supervisor");
    const childKey = await open(child), parentKey = put(parent);
    const header = (id: string, name: string) => ({ id, name, color: null, accessible: true, collapsed: false, is_default: id === "alpha", palette: 0 });
    receive({ ...emptyBatch(), machines: [{ computer: "", machine: { machine_id: computer } as never }],
      grouping: { loaded: true, default_project: "alpha", ready: true, reason: null, busy: false, status: "", status_error: false, conflicts: 0,
        headers: [header("alpha", "Child project"), header("beta", "Parent project")] },
      groups: [{ computer: "", key: childKey.slice(1), group: "alpha", rank: 0 }, { computer: "", key: parentKey.slice(1), group: "beta", rank: 0 }] });
    host.querySelector<HTMLButtonElement>('button[data-tip="Session actions"]')!.click(); await flush();
    [...host.querySelectorAll<HTMLButtonElement>(".menu-item")].find((button) => button.textContent === "Set parent…")!.click();
    await vi.dynamicImportSettled(); await flush();
    const dialog = host.querySelector<HTMLDialogElement>('dialog[aria-labelledby="parent-dlg-title"]')!;
    [...dialog.querySelectorAll<HTMLButtonElement>('[role="option"]')].find((button) => button.textContent!.includes(parent.title!))!.click();
    return { child, parent, childKey, dialog };
  }

  const projectOption = (dialog: HTMLElement, text: string) => [...dialog.querySelectorAll("label")].find((label) => label.textContent!.includes(text))?.querySelector<HTMLInputElement>("input");

  beforeEach(() => {
    transport.parent.mockReset();
    transport.parent.mockImplementation(async (...args) => ({
      project_status: (args[1] as { project?: { choice?: string } }).project?.choice === "move" ? "moved" : "kept",
    }));
  });

  it.each(["Keep current project", "Move to parent’s project"])("writes the explicit %s choice once", async (choice) => {
    const { child, parent, dialog } = await openChoices();
    const submit = dialog.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(submit.disabled).toBe(true);
    projectOption(dialog, choice)!.click();
    expect(dialog.textContent).toContain("Child project");
    expect(dialog.textContent).toContain("Parent project");
    expect(submit.disabled).toBe(false);
    const form = dialog.querySelector("form")!;
    form.dispatchEvent(new Event("submit", { cancelable: true }));
    form.dispatchEvent(new Event("submit", { cancelable: true }));
    await flush();
    expect(transport.parent).toHaveBeenCalledExactlyOnceWith(
      { computer: "", name: child.name, run_id: child.run_id, conversation_id: child.conversation_id },
      expect.objectContaining({ kind: "set", name: parent.name, session_id: parent.session_id,
        project: expect.objectContaining({ choice: choice.startsWith("Move") ? "move" : "keep",
          child_project: "alpha", parent_project: "beta", child_id: child.session_id, machine_id: computer }) }),
    );
    expect(dialog.open).toBe(false);
  });

  it("cancels with zero writes even with Move selected", async () => {
    const { dialog } = await openChoices();
    projectOption(dialog, "Move to parent’s project")!.click();
    [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Cancel")!.click();
    await flush();
    expect(dialog.open).toBe(false);
    expect(transport.parent).not.toHaveBeenCalled();
  });

  it("never drops a confirmed move choice when the displayed projects change", async () => {
    const { childKey, dialog } = await openChoices();
    projectOption(dialog, "Move to parent’s project")!.click();
    receive({ ...emptyBatch(), groups: [{ computer: "", key: childKey.slice(1), group: "beta", rank: 0 }] });
    expect(dialog.textContent).toContain("The sessions or projects changed.");
    expect(dialog.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true);
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    expect(transport.parent).not.toHaveBeenCalled();
  });

  it.each(["project write unconfirmed", "parent reply lost"])("closes without replay when the %s", async (failure) => {
    const partial = failure === "project write unconfirmed";
    if (partial) transport.parent.mockResolvedValueOnce({ project_status: "uncertain" });
    else transport.parent.mockRejectedValueOnce("The parent session change was not confirmed.");
    const { dialog } = await openChoices();
    projectOption(dialog, "Move to parent’s project")!.click();
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await flush();
    expect(transport.parent).toHaveBeenCalledTimes(1);
    expect(dialog.open).toBe(false);
    expect(host.textContent).toContain(partial ? "The parent was set. Check the session’s project"
      : "The parent session change was not confirmed.");
  });


  it("waits for memberships before classifying an uninitialized project view", async () => {
    const { dialog } = await openChoices();
    receive({ ...emptyBatch(), grouping: { loaded: false, ready: false, reason: "Project sync is starting.",
      default_project: "ungrouped", headers: [], busy: false, status: "", status_error: false, conflicts: 0 } });
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await flush();
    expect(dialog.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true);
    expect(transport.parent).not.toHaveBeenCalled();
  });

  it("does not submit apparently equal memberships from an unready project view", async () => {
    const { childKey, dialog } = await openChoices();
    receive({ ...emptyBatch(), groups: [{ computer: "", key: childKey.slice(1), group: "beta", rank: 0 }] });
    const { grouping } = await import("../../store/organization");
    receive({ ...emptyBatch(), grouping: { ...grouping(), ready: false, reason: "Project sync unavailable" } });
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await flush();
    expect(dialog.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true);
    expect(transport.parent).not.toHaveBeenCalled();
  });

  it("binds a verified same-project confirmation without showing a move choice", async () => {
    const { childKey, dialog } = await openChoices();
    receive({ ...emptyBatch(), groups: [{ computer: "", key: childKey.slice(1), group: "beta", rank: 0 }] });
    expect(projectOption(dialog, "Move to parent’s project")).toBeUndefined();
    expect(dialog.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(false);
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await flush();
    expect(transport.parent).toHaveBeenCalledExactlyOnceWith(expect.anything(), expect.objectContaining({
      project: expect.objectContaining({ choice: "keep", child_project: "beta", parent_project: "beta" }),
    }));
  });

});

it("shows a neutral Archive beside destructive Forget in a stopped session's header", async () => {
  const row = { ...fixtureRow(), status: "paused", state: "stopped", allowed_actions: ["archive", "forget"] };
  await open(row);
  host.querySelector<HTMLButtonElement>('.detail-head button[data-tip="Session actions"]')!.click(); await flush();
  const entries = [...host.querySelectorAll<HTMLButtonElement>(".menu-item")];
  const archive = entries.find((button) => button.textContent === "Archive");
  const forget = entries.find((button) => button.textContent === "Forget saved session…");
  expect(archive?.classList.contains("danger")).toBe(false);
  expect(forget?.classList.contains("danger")).toBe(true);
  expect(menu()!.items).toContainEqual(expect.objectContaining({ label: "Archive", icon: "archive" }));
  expect(menu()!.items).toContainEqual(expect.objectContaining({ label: "Forget saved session…", icon: "trash" }));
});

describe("an archive search keeps the archive status and paging (zerus-5wmh.13.17 (4))", () => {
  afterEach(() => setSearch(""));

  it("shows the window, Newer and Older for the query's page, and a capped total as N+", async () => {
    vi.mocked(bridge.archivePage).mockResolvedValue({ rows: [], next_cursor: "c", previous_cursor: "p", total: 10_000, total_capped: true, position: 50 } as never);
    put(fixtureRow());
    setStateFilter("archived"); await flush();
    setSearch("needle"); await flush();
    await vi.waitFor(() => expect(vi.mocked(bridge.archivePage).mock.calls.some((call) => call[4] === "needle")).toBe(true), { timeout: 3000 });
    await vi.waitFor(() => expect(host.querySelector(".archive-status")?.textContent).toBe("0 shown of 10000+"), { timeout: 3000 });
    expect([...host.querySelectorAll(".archive-nav")].map((b) => b.getAttribute("aria-label"))).toEqual(["Newer sessions", "Older sessions"]);
    // The results list still shows the search (messages and the page's archive rows).
    expect(host.querySelector(".search-results, [role=listbox]")).not.toBeNull();
  });

  it("never says no matches while the archive scan is still partial (zerus-080n8)", async () => {
    vi.mocked(bridge.archivePage).mockResolvedValue({ rows: [], next_cursor: "c", previous_cursor: null, total: 0, total_capped: true, position: 0, partial: true, searched: 25_000, bounded: true } as never);
    put(fixtureRow());
    setStateFilter("archived"); await flush();
    setSearch("needle"); await flush();
    await vi.waitFor(() => expect(host.querySelector(".archive-status")?.textContent).toBe("0 shown, 25000 searched so far"), { timeout: 3000 });
    await vi.waitFor(() => expect(host.querySelector(".content-search .empty h3")?.textContent).toBe("0 shown, 25000 searched so far"), { timeout: 3000 });
    expect(host.querySelector(".content-search .empty p")?.textContent).toBe("Older continues the search.");
    expect(host.textContent).not.toContain("No matching sessions");
    expect(host.textContent).not.toContain("0+");
  });
});

describe("a shown tool search hit (zerus-tso9j, zerus-2r73y)", () => {
  const toolHit = (row: ViewRow, detail: string): SearchHit => ({ name: row.name!, run_id: row.run_id!, conversation_id: row.conversation_id!, archive_id: "", message_id: "m",
    at: 1_793_000_000, role: "tool", snippet: "", event: { seq: 1, type: "PostToolUse", tool: "Bash", at: 1_793_000_000, detail }, content_truncated: false, session: null, group: "" });
  const show = async (detail: string) => {
    const row = fixtureRow(); const key = await open(row);
    setSearchPreview({ key, run: row.run_id!, conversation: row.conversation_id!, query: "needle", hit: toolHit(row, detail) }); await flush();
    return () => host.querySelector(".search-preview .act-tool-detail");
  };
  afterEach(() => clearSearchPreview());

  it("Show more reveals a match past the first 2,000 characters", async () => {
    const detail = await show(`${"x".repeat(2_200)}needle${"y".repeat(1_000)}`);
    expect(detail()!.textContent).toHaveLength(2_000);
    [...host.querySelectorAll<HTMLButtonElement>(".search-preview button")].find((b) => b.textContent === "Show more")!.click(); await flush();
    expect(detail()!.textContent).toHaveLength(3_206);
    expect([...detail()!.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["needle"]);
  });

  it("marks the new query when another hit of the same session shows (zerus-2r73y.1)", async () => {
    const row = fixtureRow(); const key = await open(row);
    const marks = () => [...host.querySelectorAll(".search-preview mark")].map((m) => m.textContent);
    const showHit = async (query: string, hit: SearchHit) => { setSearchPreview({ key, run: row.run_id!, conversation: row.conversation_id!, query, hit }); await flush(); };
    await showHit("needle", toolHit(row, "needle and prefix"));
    expect(marks()).toEqual(["needle"]);
    await showHit("prefix", { ...toolHit(row, "prefix only here"), message_id: "m2", event: { seq: 2, type: "PostToolUse", tool: "Bash", at: 1_793_000_100, detail: "prefix only here" } });
    expect(marks()).toEqual(["prefix"]);
    // A reply hit (Markdown) follows the query the same way.
    const reply = (n: number, detail: string): SearchHit => ({ ...toolHit(row, detail), message_id: `r${n}`, role: "assistant", event: { seq: 10 + n, type: "AgentMessage", at: 1_793_000_200 + n, detail } });
    await showHit("alpha", reply(1, "alpha beta"));
    await vi.waitFor(() => expect(marks()).toEqual(["alpha"]));
    await showHit("beta", reply(2, "beta gamma"));
    await vi.waitFor(() => expect(marks()).toEqual(["beta"]));
  });

  it("marks the query in a tool hit's output, and only inside the hit", async () => {
    const detail = await show("a needle tool output with NEEDLE twice");
    expect([...detail()!.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["needle", "NEEDLE"]);
    clearSearchPreview(); await flush();
    expect(host.querySelector(".search-preview")).toBeNull();
    expect(host.querySelector(".act-scroll mark")).toBeNull();
  });
});

describe("the header's keep-warm mark (zerus-yacns)", () => {
  const warmRow = (keep_warm: Record<string, unknown> | undefined) => ({ ...fixtureRow("Warm"), keep_warm }) as ViewRow;
  const now = () => Date.now() / 1000;
  const on = (until: number | null, revision = 2) => ({ enabled: true, until, revision, state: "waiting", next_at: null, reason: "", turn_active: false });
  const mark = () => host.querySelector<HTMLButtonElement>(".detail-head .dh-row2 .dh-warm");

  it("shows the bolt and the time left, or an infinity sign for Always, directly after the model and effort", async () => {
    const row = { ...warmRow(on(now() + 2 * 3600 - 30)), model: "claude-opus-5-5", effort: "xhigh" }; await open(row);
    const shown = mark()!;
    expect(shown.textContent).toBe("1 h 59 m");
    expect(shown.getAttribute("aria-label")).toBe("Keep cache warm: 1 h 59 m left");
    expect(shown.title.split("\n")[0]).toBe("Keep cache warm: 1 h 59 m left");
    expect(shown.previousElementSibling).toBe(host.querySelector(".detail-head .dh-model"));
    expect(shown.previousElementSibling?.textContent).toBe("claude-opus-5-5 xhigh");
    put({ ...row, keep_warm: on(null, 3) }); await flush();
    expect(mark()).toBe(shown);
    expect([mark()!.textContent, mark()!.getAttribute("aria-label")]).toEqual(["∞", "Keep cache warm: Always"]);
  });

  it("keeps the remote session's mark directly after its model and effort (zerus-63zvg)", async () => {
    const row = { ...warmRow(on(null)), model: "claude-opus-5-5", effort: "xhigh", machine: "Remote fixture" };
    const computer = "warm-remote", key = `run:${row.run_id}`;
    receive({ ...emptyBatch(), computers: [{ computer, status: "live", attempt: 0 }], upserts: [{ computer, key, row }] });
    setSelectedKey(`${computer}\n${key}`); await vi.dynamicImportSettled(); await flush();
    expect(selected()?.computer).toBe(computer);
    const model = host.querySelector(".detail-head .dh-model")!;
    expect(model.textContent).toBe("claude-opus-5-5 xhigh");
    expect(model.nextElementSibling).toBe(mark());
    expect(mark()?.getAttribute("aria-label")).toBe("Keep cache warm: Always");
    receive({ ...emptyBatch(), replaces: [{ computer, rows: [] }] });
  });

  it("shows nothing while keep-warm is off or not reported", async () => {
    const row = warmRow({ ...on(null), enabled: false, state: "off" }); await open(row);
    expect(mark()).toBeNull();
    put({ ...row, keep_warm: undefined }); await flush();
    expect(mark()).toBeNull();
  });

  it("keeps the same mark node through refreshes and clock ticks", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    try {
      const row = warmRow(on(now() + 3 * 3600)); await open(row);
      const shown = mark()!, removed: Node[] = [];
      expect(shown).not.toBeNull();
      const observer = new MutationObserver((records) => records.forEach((record) => removed.push(...record.removedNodes)));
      observer.observe(host, { subtree: true, childList: true });
      for (let tick = 0; tick < 3; tick++) {
        vi.advanceTimersByTime(30_000);
        put({ ...row, summary: `Refresh ${tick}`, keep_warm: on(now() + 3 * 3600 - tick, 2 + tick) }); await flush();
      }
      observer.disconnect();
      expect(mark()).toBe(shown);
      expect(removed.filter((node) => node === shown || node.contains(shown))).toEqual([]);
    } finally { vi.useRealTimers(); }
  });

  it("opens Details at the Keep cache warm control when clicked", async () => {
    await setSetting("workspace/inspectorOpen", false, { persist: false });
    await setSetting("workspace/inspectorClosed", JSON.stringify(["details-session"]), { persist: false });
    vi.mocked(bridge.inspectSession).mockResolvedValue(null);
    const key = put(warmRow(on(null))); setSelectedKey(key); setDetailTab("activity"); await flush();
    mark()!.click(); await vi.dynamicImportSettled(); await flush();
    await vi.waitFor(() => expect(host.querySelector(".kw-row button[role=switch]")).not.toBeNull());
    expect(inspectorTab()).toBe("details");
    await vi.waitFor(() => expect(document.activeElement).toBe(host.querySelector(".kw-row button[role=switch]")));
  });
});


it("shows the bound Team tab's unread count from the producer's list while Activity is selected", async () => {
  const { teams } = await import("../../store/teams");
  transport.enablesTeams = true;
  try { await teams.recheck(); } finally { transport.enablesTeams = false; }
  const lead = contact.list.teams[0].lead_session;
  receive({ ...emptyBatch(),
    machines: [{ computer: "", machine: machineMeta("fixture", {}, { machine_id: lead.machine }) }],
    teams: [{ computer: "", key: "list", kind: "list", payload: contact.list }],
  });
  await open({ ...fixtureRow(), launch_id: lead.session_id.slice("launch_id:".length) });
  const badge = host.querySelector('[data-tab="team"] [data-team-tab-notices]');
  expect(badge?.textContent).toBe("1");
  expect(host.querySelector('[data-tab="team"]')?.getAttribute("aria-selected")).toBe("false");
});

function antigravityComposer(row: ViewRow): ComposerView {
  return {
    handle: 118, key: draftKey({ computer: "", key: `run:${row.run_id}`, name: row.name, session_id: row.session_id, archive_id: null }),
    loaded: true, rev: 1, text_epoch: 1, text: "Private Antigravity draft", position: 0, anchor: 0,
    attachments: [], removed: null, attaching: false, sending: false, waiting: null, uncertain: false,
    status: "", status_error: false, can_send: false, send_label: { source: "Send" }, available: false,
    unavailable_reason: "On this computer, run agy acp --login, then refresh the session. Your draft stays here.",
    block_reason: null, attachments_available: false, attachments_reason: "", session_status: "", session_status_error: false,
    session_blocks_input: false, inspect_error: false, local: [], compacting: false, cancel_send: false, input_visible: true,
    question: null, queue: null, cache: { visible: false, tone: "quiet", label: "", short_label: "" },
    cache_warning: "", cache_details: "",
    context: { visible: false, tone: "quiet", text: "", tooltip: "", warning: false, percent: null, accessible_name: "" },
    clear: { enabled: false, dismiss: false, title: "", text: "", confirm: "", run: "", conversation: "", cache_enabled: false, running: false, reason: "", cache_reason: "" },
  };
}

it.each(["en", "ru"] as const)("changes Antigravity model and thinking in the shared composer slot in %s", async (lang) => {
  transport.language = lang;
  const label = lang === "ru" ? {
    settings: "Настройки модели Antigravity", apply: "Применить", quota: "Квота аккаунта", context: "Использование контекста",
    check: "Проверить активность", unknown: "Изменение настроек Antigravity не подтверждено",
    old: "Этот процесс Antigravity не сообщает текущие настройки модели",
  } : {
    settings: "Antigravity model settings", apply: "Apply", quota: "Account quota", context: "Context usage",
    check: "Check Activity", unknown: "Antigravity settings change is unconfirmed",
    old: "This Antigravity worker does not report its current model settings",
  };
  const row: ViewRow = { ...fixtureRow(), name: "agy/example/settings", agent: "agy", run_id: uuid(801), conversation_id: uuid(802),
    process_state: "running", model: "Stale catalog choice", terminal_supported: false };
  const before = uuid(810), after = uuid(811), refreshed = uuid(812);
  const draft = { ...antigravityComposer(row), available: true, can_send: true, unavailable_reason: "" };
  vi.mocked(bridge.composerOpen).mockImplementation(async session => session?.name === row.name ? draft : null);
  const key = (await open(row)).slice(1);
  const settings = (current: string, effort: string, revision: string) => ({
    settings_model: current, effort, agy_settings: { status: "confirmed", revision, mode: { id: "default", label: "Review changes" } },
    settings_change_supported: true, model_change_supported: true, effort_change_supported: true, settings_apply_when: "now",
    model_options: [
      { id: "first", label: "Fixture Flash", ...(current === "first" ? { effort_options: ["mid", "high"], effort_labels: { mid: "Medium", high: "High" } } : {}) },
      { id: "second", label: "Fixture Pro", ...(current === "second" ? { effort_options: ["mid", "max"], effort_labels: { mid: "Medium", max: "Maximum" } } : {}) },
    ],
    session_usage: { context: { used: 180, limit: 512, used_source: "agy_acp", limit_source: "agy_acp" } },
    agy_quota: { status: "unavailable", reason: "provider_interface_unavailable" },
  });
  const state = (rev: number, value: Record<string, unknown>) => receive({ ...emptyBatch(), activity: [{
    computer: "", key, kind: "state", payload: { rev, value: { name: row.name, session_id: row.session_id, run_id: row.run_id,
      conversation_id: row.conversation_id, agent: "agy", process_state: "running", activity: "idle", phase: "idle", ...value } },
  }] });
  const calls: unknown[] = [], otherWrites: string[] = [];
  transport.composer = (command, args) => {
    if (command === "run_action") {
      calls.push(args);
      return calls.length === 1 ? { outcome: "done", receipt: { model: "receipt is not the displayed model" }, cleanup: null } : { outcome: "uncertain", detail: "Private fixture" };
    }
    if (["send_message", "local_retry", "composer_allow_retry"].includes(command)) otherWrites.push(command);
    return undefined;
  };
  const selector = `[aria-label="${label.settings}"]`;
  const button = host.querySelector<HTMLButtonElement>(`.cmp-actions ${selector}`)!;
  expect(button).not.toBeNull();
  expect(host.querySelectorAll(selector)).toHaveLength(1);
  expect(host.querySelector(`.detail-tabs ${selector}`)).toBeNull();
  expect(button.compareDocumentPosition(host.querySelector(".cmp-actions .primary")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const draftText = () => host.querySelector<HTMLTextAreaElement>(".cmp-editor")?.value;
  expect(draftText()).toBe(draft.text);
  const dialog = () => host.querySelector<HTMLDialogElement>('dialog[aria-labelledby="agy-settings-title"]')!;
  const field = (selector: string) => dialog().querySelector<HTMLButtonElement>(selector)!;
  const action = (caption: string) => [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === caption)!;
  const choose = async (selector: string, value: string) => {
    field(selector).click(); await flush();
    dialog().querySelector<HTMLButtonElement>(`[role="option"][data-value="${value}"]`)!.click();
  };
  state(2, { agy_settings: { status: "unavailable", reason: "legacy_worker" }, resumable: true,
    settings_model: "stale legacy cache", settings_change_supported: true });
  button.click(); await flush();
  expect(dialog().textContent).toContain(label.old);
  expect(host.querySelector(".detail-head")?.textContent).not.toContain("Stale catalog choice");
  expect(dialog().textContent).not.toContain("stale legacy cache");
  expect(draftText()).toBe(draft.text);
  expect(calls).toEqual([]);
  expect(action(label.apply).disabled).toBe(true);
  state(3, settings("first", "mid", before));
  expect(button.textContent).toContain("Fixture Flash");
  expect(dialog().textContent).toContain(label.quota);
  expect(dialog().textContent).toContain(label.context);
  expect(dialog().querySelector(`[role="group"][aria-label^="${label.quota}"]`)).not.toBeNull();
  expect(dialog().querySelector(`[role="group"][aria-label^="${label.context}"]`)?.getAttribute("aria-label")).toContain("180");
  expect(dialog().textContent).toContain("180");
  expect(dialog().textContent).toContain("/usage");
  field("#agy-model").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", ctrlKey: true, bubbles: true, cancelable: true }));
  await flush();
  expect(calls).toEqual([]);
  expect(otherWrites).toEqual([]);
  await choose("#agy-model", "second");
  expect(field("#agy-effort").disabled).toBe(true);
  const connection = (ready: boolean) => receive({ ...emptyBatch(), activity: [{
    computer: "", key, kind: "connection", payload: { ready },
  }] });
  connection(false);
  expect(action(label.apply).disabled).toBe(true);
  connection(true);
  expect(activity.currentSummary()).toBeUndefined();
  expect(action(label.apply).disabled, "Apply needs fresh provider settings after reconnect").toBe(true);
  action(label.apply).click();
  expect(calls).toEqual([]);
  state(1, settings("first", "mid", before));
  expect(action(label.apply).disabled).toBe(false);
  action(label.apply).click(); await flush();
  const target = { computer: "", name: row.name, run_id: row.run_id, conversation_id: row.conversation_id };
  const change = { kind: "antigravity_settings", model: "second", expected_session_id: row.session_id };
  expect(calls).toEqual([{ target, action: { ...change, expected_config_revision: before }, delay: undefined }]);
  expect(button.textContent).toContain("Fixture Flash");
  expect(action(label.apply).disabled).toBe(true);
  state(2, settings("second", "mid", after));
  expect(button.textContent).toContain("Fixture Pro");
  await choose("#agy-effort", "max");
  action(label.apply).click(); await flush();
  expect(calls).toEqual([
    { target, action: { ...change, expected_config_revision: before }, delay: undefined },
    { target, action: { ...change, effort: "max", expected_config_revision: after }, delay: undefined },
  ]);
  expect(dialog().textContent).toContain(label.unknown);
  expect(action(label.apply).disabled).toBe(true);
  action(label.check).click(); await flush();
  state(1, settings("second", "mid", after));
  expect(field("#agy-effort").disabled).toBe(true);
  state(2, { ...settings("first", "high", refreshed), session_id: uuid(899) });
  expect(button.textContent).not.toContain("Fixture Flash");
  expect(action(label.apply).disabled).toBe(true);
  state(3, settings("second", "max", refreshed));
  expect(button.textContent).toContain("Maximum");
  expect(field("#agy-effort").disabled).toBe(false);
  expect(draftText()).toBe(draft.text);
  expect(calls).toHaveLength(2);
  expect(otherWrites).toEqual([]);
});

it.each(["en", "ru"] as const)("uses Antigravity Activity and guarded lifecycle through the ordinary menu in %s", async (lang) => {
  transport.language = lang;
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(700);
  onTestFinished(() => { height.mockRestore(); width.mockRestore(); });
  const labels = lang === "ru"
    ? { menu: "Действия сессии", stop: "Остановить сессию…", confirm: "Остановить сессию", resume: "Возобновить сессию", retry: "Повторить запуск", cancel: "Отмена" }
    : { menu: "Session actions", stop: "Stop session…", confirm: "Stop session", resume: "Resume session", retry: "Retry start", cancel: "Cancel" };
  const fixture = fixtureRow("Private Antigravity");
  const row: ViewRow = { ...fixture, name: "agy/example/fixture", agent: "agy", terminal_supported: false };
  const view = antigravityComposer(row);
  vi.mocked(bridge.composerOpen).mockImplementation(async s => s?.name === row.name ? view : null);
  const key = (await open(row)).slice(1);
  const lifecycle: unknown[] = [], sendCalls: unknown[] = [], inspections: unknown[] = [];
  transport.composer = (command, args) => {
    if (command === "local_inspect") {
      inspections.push(args);
      return { kind: "activity", run_id: row.run_id, conversation_id: row.conversation_id };
    }
    if (command === "send_message") {
      sendCalls.push(args);
      return { kind: "sending" };
    }
    if (command === "run_action") {
      lifecycle.push(args);
      return { outcome: "done", receipt: {}, cleanup: null };
    }
    return undefined;
  };
  const menus = () => [...host.querySelectorAll<HTMLButtonElement>(".menu-item")];
  const openMenu = async (label: string) => {
    setMenu(undefined);
    host.querySelector<HTMLButtonElement>(`.detail-head button[data-tip="${labels.menu}"]`)!.click();
    await flush();
    return menus().find((button) => button.textContent?.trim() === label);
  };
  const state = (rev: number, extra: Record<string, unknown>) => receive({ ...emptyBatch(), activity: [{
    computer: "", key, kind: "state", payload: { rev, value: { backend: "agy", run_id: row.run_id,
      conversation_id: row.conversation_id, terminal_supported: false, ...extra } },
  }] });
  const confirmButton = (label: string) => [...host.querySelectorAll<HTMLButtonElement>('dialog[aria-labelledby="action-dlg-title"] button')]
    .find(b => b.textContent?.trim() === label);
  expect(selected()?.agent).toBe("agy");
  expect(host.querySelector(".detail-head .agent-badge")?.getAttribute("title")).toBe("Google Antigravity");
  state(2, { activity_supported: false, auth_required: true });
  expect(host.querySelector(".qcard[role=status]")?.textContent).toContain("agy acp --login");
  const sendDraft = () => host.querySelector<HTMLButtonElement>(".cmp-actions .primary")!;
  expect(sendDraft().disabled).toBe(true);
  sendDraft().click();
  expect(sendCalls).toEqual([]);
  expect(host.querySelector<HTMLTextAreaElement>(".cmp-editor")?.value).toBe(view.text);
  expect((await openMenu(labels.stop))?.getAttribute("aria-disabled")).toBe("true");
  expect(lifecycle).toEqual([]);

  state(3, { activity_supported: true, send_supported: true, send_available: true,
    allowed_actions: ["pause"], activity: "idle", phase: "idle", runtime_state: "live" });
  receive({ ...emptyBatch(), composer: { ...view, available: true, can_send: true, unavailable_reason: "" } });
  expect(host.querySelector(".qcard[role=status]")).toBeNull();
  expect(sendDraft().disabled).toBe(false);
  sendDraft().click();
  await vi.waitFor(() => expect(sendCalls).toHaveLength(1));
  expect(sendCalls[0]).toMatchObject({ handle: view.handle, text: view.text });
  const reply = { seq: 2, type: "AgentMessage", source: "agy_acp", detail: "Synthetic Antigravity reply" };
  receive({ ...emptyBatch(), activity: [{ computer: "", key, kind: "replace", payload: { events: [reply], cursor: 2 } }] });
  expect(activity.events()).toEqual([reply]);
  const unknownId = uuid(99801);
  const unknown: LocalMessage = {
    id: unknownId, text: view.text, status: "error", attachments: [], run_id: row.run_id ?? "",
    conversation_id: row.conversation_id ?? "", submitted_at: 1, error: "Delivery is not confirmed. Review Activity before retrying.",
    uncertain: true, message_id: null, submitted_text: null, source: null, voice: null,
  };
  const earlier = { seq: 1, type: "UserPromptSubmit", source: "agy_acp", detail: "Earlier Antigravity prompt", delivery_state: "submitted" };
  const offer = { seq: 3, type: "UserPromptSubmit", source: "agy_acp", detail: unknown.text, request_id: unknownId,
    run_id: row.run_id, conversation_id: row.conversation_id, delivery_state: "unconfirmed" };
  receive({ ...emptyBatch(), activity: [{ computer: "", key, kind: "replace", payload: { events: [earlier, reply, offer], cursor: 3 } }] });
  receive({ ...emptyBatch(), composer: { ...view, rev: 2, available: true, uncertain: true, local: [unknown],
    status: unknown.error, status_error: true } });
  const localButtons = () => [...host.querySelectorAll<HTMLButtonElement>(".act-local-actions button")];
  const checkLabel = lang === "ru" ? "Проверить активность" : "Check Activity";
  await vi.waitFor(() => expect(localButtons().map(button => button.textContent)).toContain(checkLabel));
  expect(localButtons()).toHaveLength(2); // Inspect and Delete, with no ordinary Retry
  const userRows = () => [...host.querySelectorAll(".act-msg.k-user")];
  expect(userRows()).toHaveLength(2);
  expect(userRows()[0].querySelector(".act-receipt")?.textContent).toBe(lang === "ru" ? "Отправлено" : "Sent");
  expect(userRows()[1].querySelector(".act-stamp")?.textContent).toBe(lang === "ru" ? "Доставка не подтверждена" : "Delivery not confirmed");
  expect(userRows()[1].querySelector(".act-receipt")).toBeNull();
  const guidance = lang === "ru" ? "Доставка не подтверждена. Перед повторной попыткой проверьте активность." : unknown.error;
  expect(userRows()[1].querySelector(".act-error")?.textContent).toBe(guidance);
  expect(host.querySelector(".cmp-status")?.getAttribute("aria-label")).toBe(guidance);
  expect(sendDraft().disabled).toBe(true);
  sendDraft().click();
  host.querySelector(".cmp-editor")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(sendCalls).toHaveLength(1);
  vi.mocked(bridge.activityPort.follow).mockClear();
  localButtons().find(button => button.textContent === checkLabel)!.click();
  await vi.waitFor(() => expect(inspections).toEqual([{ handle: view.handle, id: unknownId }]));
  await vi.waitFor(() => expect(bridge.activityPort.follow).toHaveBeenCalledExactlyOnceWith({
    computer: "", key, name: row.name, run_id: row.run_id, conversation_id: row.conversation_id, after: undefined, tail: 100,
  }));
  expect(activity.events()).toEqual([earlier, reply, offer]);
  expect(sendCalls).toHaveLength(1);
  expect(detailTab()).toBe("activity");
  expect(host.querySelector<HTMLTextAreaElement>(".cmp-editor")?.value).toBe(view.text);
  const stop = await openMenu(labels.stop);
  expect(stop?.getAttribute("aria-disabled")).toBe("false");
  stop!.click();
  await flush();
  expect(lifecycle).toEqual([]);
  confirmButton(labels.cancel)!.click();
  expect(lifecycle).toEqual([]);

  (await openMenu(labels.stop))!.click();
  await flush();
  confirmButton(labels.confirm)!.click();
  const target = { name: row.name, run_id: row.run_id, conversation_id: row.conversation_id, computer: "" };
  await vi.waitFor(() => expect(lifecycle).toEqual([{ target, action: { kind: "pause" }, delay: undefined }]));

  put({ ...row, state: "paused", status: "paused", resumable: true, allowed_actions: ["resume"] });
  state(4, { activity_supported: true, allowed_actions: ["resume"], runtime_state: "stopped" });
  await flush();
  expect(selected()).toMatchObject({ state: "paused", cmd: "agy" });
  const resume = await openMenu(labels.resume);
  expect(resume?.getAttribute("aria-disabled"), menus().map(b => b.textContent).join(", ")).toBe("false");
  resume!.click();
  await vi.waitFor(() => expect(lifecycle).toEqual([
    { target, action: { kind: "pause" }, delay: undefined },
    { target, action: { kind: "resume" }, delay: undefined },
  ]));

  const signedOut = { ...fixtureRow("First-time sign-in"), name: "agy/example/sign-in", agent: "agy",
    conversation_id: "", state: "stopped", status: "paused", resumable: true, allowed_actions: ["resume"] };
  const authKey = (await open(signedOut)).slice(1);
  const auth = (rev: number, retry?: boolean) => receive({ ...emptyBatch(), activity: [{
    computer: "", key: authKey, kind: "state", payload: { rev, value: {
      run_id: signedOut.run_id, conversation_id: "", allowed_actions: ["resume"], auth_required: true, startup_retry_available: retry,
    } },
  }] });
  auth(2);
  expect((await openMenu(labels.resume))?.getAttribute("aria-disabled")).toBe("true");
  expect(lifecycle).toHaveLength(2);
  auth(3, true);
  expect(host.querySelector(".qcard[role=status]")?.textContent).toContain(labels.retry);
  const retry = await openMenu(labels.retry);
  expect(retry?.getAttribute("aria-disabled")).toBe("false");
  retry!.click();
  await vi.waitFor(() => expect(lifecycle).toHaveLength(3));
  expect(lifecycle[2]).toMatchObject({
    target: { computer: "", name: signedOut.name, run_id: signedOut.run_id, conversation_id: "" },
    action: { kind: "resume" },
  });
});
