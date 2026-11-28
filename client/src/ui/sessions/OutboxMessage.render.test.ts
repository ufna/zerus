// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { composerCheckDelivery } from "../../bridge";
import type { DeliveryObservation } from "../../types";
import { OutboxActions, OutboxScope } from "./OutboxMessage";
import type { OutboxItem } from "./types";

vi.mock("../../bridge", () => ({ composerCheckDelivery: vi.fn() }));
vi.mock("../../i18n", () => ({ tr: (source: string) => source }));
vi.mock("../../store/composer", () => ({ composer: {
  operationLocked: () => false, waitingMessages: () => [], removeWaiting: vi.fn(),
} }));
vi.mock("../../store/ui", () => ({ setDetailTab: vi.fn() }));

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); document.body.replaceChildren(); });

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
