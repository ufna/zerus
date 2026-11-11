// @vitest-environment happy-dom
// One create intent: its team and request ids stay until it ends as done, as a
// refusal, or by the person's Dismiss; while it is uncertain the form is locked
// and offers Check (the same ids) and Dismiss, never a second Create.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({
  setUnresolved: (_: unknown[]) => {}, runAgain: vi.fn(async (): Promise<{ outcome: string; detail?: string; receipt?: unknown; request_id: string }> => ({ outcome: "done", receipt: {}, request_id: "" })),
  dismiss: vi.fn(async () => {}), created: [] as { team: string; request_id: string }[],
}));
let next = 0;
vi.mock("../../store/teams", async () => {
  const { createSignal: signal } = await import("solid-js");
  const [unresolved, setUnresolved] = signal<unknown[]>([]);
  f.setUnresolved = setUnresolved as (value: unknown[]) => void;
  return {
    teams: { unresolved, runAgain: f.runAgain, dismiss: f.dismiss },
    newRequestId: () => `id-${++next}`,
  };
});
import { NewTeam } from "./NewTeam";
import { catalog } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; f.setUnresolved([]); f.created = []; f.runAgain.mockClear(); f.dismiss.mockClear(); document.body.replaceChildren(); });

function setup(outcome: (input: { team: string; request_id: string }) => unknown) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async () => ({ git: true, head: "0123456789abcdef0123456789abcdef01234567", branch: "main", reason: "" }),
    onCreate: async (input) => { f.created.push({ team: input.team, request_id: input.request_id }); return outcome(input) as never; },
    onSaveTemplate: async () => {}, onClose: () => {},
  }), host);
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  (host.querySelector("[data-check-folder]") as HTMLButtonElement).click();
  return host;
}
const create = (host: HTMLElement) => [...host.querySelectorAll("button")].find((button) => button.textContent === "Create team") as HTMLButtonElement | undefined;
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

it("keeps one intent while the create is uncertain and offers Check and Dismiss", async () => {
  const host = setup(() => ({ outcome: "uncertain", detail: "reply lost", request_id: "" }));
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  create(host)!.click();
  await settle();
  const first = f.created[0];
  expect(first.team).toMatch(/^id-/);
  // The store kept the unresolved call; the form locks and hides Create.
  f.setUnresolved([{ call: { team: first.team, request_id: first.request_id, action: { kind: "create" } }, detail: "reply lost", state: "uncertain" }]);
  await settle();
  expect(create(host)).toBeUndefined();
  const notice = host.querySelector("[data-create-uncertain]")!;
  expect(notice.textContent).toContain("unknown");
  expect((host.querySelector('input[placeholder="Fixes"]') as HTMLInputElement).disabled).toBe(true);
  // Check sends the same intent and keeps it when the answer is still unknown.
  f.runAgain.mockResolvedValueOnce({ outcome: "uncertain", detail: "still unknown", request_id: first.request_id });
  (notice.querySelector("button") as HTMLButtonElement).click();
  await settle();
  expect(f.runAgain).toHaveBeenCalledWith(expect.objectContaining({ request_id: first.request_id }));
  // Dismiss ends it.
  const dismiss = [...notice.querySelectorAll("button")].find((button) => button.textContent === "Dismiss") as HTMLButtonElement;
  dismiss.click();
  await settle();
  expect(f.dismiss).toHaveBeenCalledWith(expect.objectContaining({ call: expect.objectContaining({ request_id: first.request_id }) }));
  f.setUnresolved([]);
  await settle();
  expect(create(host)).toBeDefined();
});

it("only after Dismiss may a new intent with new ids start", async () => {
  const host = setup(() => ({ outcome: "uncertain", detail: "reply lost", request_id: "" }));
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  create(host)!.click();
  await settle();
  const first = f.created[0];
  f.setUnresolved([{ call: { team: first.team, request_id: first.request_id, action: { kind: "create" } }, detail: "reply lost", state: "uncertain" }]);
  await settle();
  ([...host.querySelectorAll("[data-create-uncertain] button")].find((button) => button.textContent === "Dismiss") as HTMLButtonElement).click();
  await settle();
  f.setUnresolved([]);
  await settle();
  create(host)!.click();
  await settle();
  expect(f.created).toHaveLength(2);
  expect(f.created[1].team).not.toBe(first.team);
  expect(f.created[1].request_id).not.toBe(first.request_id);
});

it("a refusal ends the intent and shows its reason in the dialog", async () => {
  const host = setup(() => ({ outcome: "failed", detail: "The template was refused.", code: "invalid", request_id: "" }));
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  create(host)!.click();
  await settle();
  expect(host.querySelector("[data-new-team-error]")?.textContent).toContain("The template was refused.");
  expect(create(host)).toBeDefined();
});
