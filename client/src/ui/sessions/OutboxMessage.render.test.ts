// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { composer } from "../../store/composer";
import { composerCheckDelivery } from "../../bridge";
import type { DeliveryObservation, SkipWaiting } from "../../types";
import { OutboxActions, OutboxScope } from "./OutboxMessage";
import type { OutboxItem } from "./types";

vi.mock("../../bridge", () => ({ composerCheckDelivery: vi.fn() }));
vi.mock("../../i18n", () => ({ tr: (source: string) => source }));
vi.mock("../../store/composer", () => ({ composer: {
  operationLocked: () => false, waitingMessages: () => [], removeWaiting: vi.fn(), skipWaitingBinding: vi.fn(),
} }));
vi.mock("../../store/ui", () => ({ setDetailTab: vi.fn() }));

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); vi.restoreAllMocks(); vi.clearAllMocks(); });

it("checks only the original Unknown Activity head and discards an observation after a scope change", async () => {
  const head: OutboxItem = { handle: 7, request_id: "00000000-0000-4000-8000-000000000001",
    run_id: "run", conversation_id: "conversation", state: "unknown", preview: "same", can_check_delivery: true };
  const follower: OutboxItem = { ...head, request_id: "00000000-0000-4000-8000-000000000002",
    state: "waiting", can_check_delivery: false };
  const [rows, setRows] = createSignal([head, follower]);
  const done = vi.fn();
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(OutboxScope.Provider, {
    value: { rows, done },
    get children() { return [head, follower].map((item) => createComponent(OutboxActions, { item })); },
  }), host);
  const button = () => [...host.querySelectorAll("button")].find((b) => b.textContent === "Check delivery");
  const status = () => host.querySelector('[role="status"]')?.textContent;
  let reply!: (result: DeliveryObservation) => void;
  vi.mocked(composerCheckDelivery).mockImplementation(() => new Promise((resolve) => { reply = resolve; }));
  expect([...host.querySelectorAll("button")].filter((b) => b.textContent === "Check delivery")).toHaveLength(1);
  button()!.click();
  expect(button()?.disabled).toBe(true);
  button()!.click();
  expect(composerCheckDelivery).toHaveBeenCalledExactlyOnceWith(head.handle, head.request_id);
  reply("accepted");
  await vi.waitFor(() => expect(status()).toContain("does not confirm the model read"));
  for (const [result, text] of [
    ["no_input", "Confirmed: no input was offered for this message."],
    ["unresolved", "Delivery remains unconfirmed."],
  ] as const) {
    button()!.click();
    reply(result);
    await vi.waitFor(() => expect(status()).toBe(text));
  }
  expect(rows()).toEqual([head, follower]);
  expect(done).not.toHaveBeenCalled();

  button()!.click();
  setRows([{ ...head, conversation_id: "replacement", can_check_delivery: false }, follower]);
  reply("no_input");
  await Promise.resolve();
  expect(button()).toBeUndefined();
  expect(status()).toBeUndefined();
});


it("skips only the confirmed current Unknown blocking original, preserving cancellation and stale-scope guards", () => {
  const head: OutboxItem = { handle: 7, request_id: "00000000-0000-4000-8000-000000000001",
    run_id: "run", conversation_id: "conversation", state: "unknown", preview: "same" };
  const follower: OutboxItem = { ...head, request_id: "00000000-0000-4000-8000-000000000002", state: "waiting" };
  const [rows, setRows] = createSignal([head, follower]);
  vi.spyOn(composer, "waitingMessages").mockImplementation(rows);
  const binding: SkipWaiting = { handle: head.handle, key: "draft-key", target: { computer: "studio", name: "claude/sample-project/review", run_id: "run", conversation_id: "conversation" },
    selection_epoch: 4, route_revision: 2, queue_revision: 1, queue: [[head.request_id, "unknown"], [follower.request_id, "waiting"]] };
  const [nativeBinding, setNativeBinding] = createSignal<SkipWaiting | undefined>(binding);
  vi.mocked(composer.skipWaitingBinding).mockImplementation((handle, request) =>
    handle === head.handle && request === head.request_id ? nativeBinding() : undefined);
  const done = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(OutboxScope.Provider, {
    value: { rows, done },
    get children() { return [head, follower].map((item) => createComponent(OutboxActions, { item })); },
  }), host);
  const skip = () => host.querySelector<HTMLButtonElement>('[aria-label="Skip and continue…"]');
  const confirm = () => host.querySelector<HTMLButtonElement>('.act-outbox-confirm button:last-child');
  expect(skip()?.textContent).toBe("Skip and continue…");
  expect(host.querySelectorAll('[aria-label="Remove from Zerus queue…"]')).toHaveLength(1);
  skip()!.click();
  expect(host.querySelector('.act-outbox-confirm')?.textContent).toContain("without being sent again");
  expect(host.querySelector('.act-outbox-confirm')?.textContent).toContain("may already have reached the agent");
  expect(host.querySelector('.act-outbox-confirm')?.textContent).toContain("Later queued messages can then proceed");
  host.querySelector<HTMLButtonElement>('.act-outbox-confirm button')!.click();
  expect(confirm()).toBeNull();
  expect(composer.removeWaiting).not.toHaveBeenCalled();
  skip()!.click();
  host.querySelector('.act-outbox-confirm')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  expect(confirm()).toBeNull();
  for (const stale of [
    [{ ...head, conversation_id: "other" }, follower],
    [{ ...head, run_id: "new-run" }, follower],
    [{ ...head, handle: 8 }, { ...follower, handle: 8 }],
    [{ ...head, request_id: "00000000-0000-4000-8000-000000000003" }, follower],
    [{ ...head, target_changed: true }, follower],
    [],
    [follower, head],
    [{ ...head, state: "waiting" as const }, follower],
    [head],
  ]) {
    setRows([head, follower]); skip()!.click();
    const oldConfirm = confirm()!;
    setRows(stale); oldConfirm.click();
    expect(confirm()).toBeNull();
    expect(composer.removeWaiting).not.toHaveBeenCalled();
    expect(done).not.toHaveBeenCalled();
  }
  setRows([follower, head]);
  expect(skip()).toBeNull();
  expect(host.querySelectorAll('[aria-label="Remove from Zerus queue…"]')).toHaveLength(2);
  setRows([head, follower]);
  for (const staleBinding of [
    { ...binding, selection_epoch: 5 },
    { ...binding, queue_revision: 2 },
    { ...binding, route_revision: 3 },
    { ...binding, target: { ...binding.target, conversation_id: "changed" } },
  ]) {
    setNativeBinding(binding); skip()!.click(); const staleBindingConfirm = confirm()!;
    setNativeBinding(staleBinding); staleBindingConfirm.click();
    expect(confirm()).toBeNull();
    expect(composer.removeWaiting).not.toHaveBeenCalled();
  }
  setNativeBinding(binding);
  skip()!.click(); const staleQueueConfirm = confirm()!;
  setNativeBinding({ ...binding, queue: [[head.request_id, "waiting"], [follower.request_id, "waiting"]] }); staleQueueConfirm.click();
  expect(confirm()).toBeNull();
  expect(composer.removeWaiting).not.toHaveBeenCalled();
  setNativeBinding(binding);
  skip()!.click(); confirm()!.click();
  expect(composer.removeWaiting).toHaveBeenCalledExactlyOnceWith(head.handle, head.request_id,
    { kind: "skip_unknown", binding });
  expect(composerCheckDelivery).not.toHaveBeenCalled();
  expect(done).toHaveBeenCalledOnce();
  expect(rows()).toEqual([head, follower]);
});


it("keeps ordinary removal generic and never falls back after a refused Skip", async () => {
  const head: OutboxItem = { handle: 7, request_id: "original", run_id: "run", conversation_id: "conversation", state: "unknown", preview: "same" };
  const follower: OutboxItem = { ...head, request_id: "follower", state: "waiting" };
  const rows = () => [head, follower];
  const binding: SkipWaiting = { handle: 7, key: "key", target: { computer: "studio", name: "claude/sample-project/review", run_id: "run", conversation_id: "conversation" },
    selection_epoch: 1, route_revision: 1, queue_revision: 1, queue: [[head.request_id, "unknown"], [follower.request_id, "waiting"]] };
  vi.mocked(composer.skipWaitingBinding).mockImplementation((_, id) => id === head.request_id ? binding : undefined);
  const refused = vi.fn();
  vi.mocked(composer.removeWaiting).mockImplementation(async (_h, _id, intent) => { if (intent) refused(); });
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(OutboxScope.Provider, {
    value: { rows, done: vi.fn() },
    get children() { return [head, follower].map((item) => createComponent(OutboxActions, { item })); },
  }), host);
  host.querySelector<HTMLButtonElement>('[aria-label="Remove from Zerus queue…"]')!.click();
  host.querySelector<HTMLButtonElement>('.act-outbox-confirm button:last-child')!.click();
  expect(composer.removeWaiting).toHaveBeenCalledExactlyOnceWith(follower.handle, follower.request_id);
  vi.mocked(composer.removeWaiting).mockClear();
  host.querySelector<HTMLButtonElement>('[aria-label="Skip and continue…"]')!.click();
  host.querySelector<HTMLButtonElement>('.act-outbox-confirm button:last-child')!.click();
  await Promise.resolve();
  expect(refused).toHaveBeenCalledOnce();
  expect(composer.removeWaiting).toHaveBeenCalledExactlyOnceWith(head.handle, head.request_id, { kind: "skip_unknown", binding });
});
