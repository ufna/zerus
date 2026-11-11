// @vitest-environment happy-dom
// One create intent per dialog (finding 1 of the second review). The intent
// lives in the store, so closing the dialog keeps it; a reopened dialog adopts
// it reactively, also one that becomes unknown after the reopen; only the
// store settles it (done, a refusal, or a Dismiss the core took). A Dismiss
// that fails or is refused changes nothing, and two team ids are never alive.
// The real store runs here; only the Tauri invoke is a fake.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { TeamCall, TeamOutcome } from "../../store/teams";

const f = vi.hoisted(() => ({
  acts: [] as TeamCall[],
  again: [] as TeamCall[],
  dismissed: [] as TeamCall[],
  act: (_call: TeamCall): Promise<TeamOutcome> => Promise.resolve({ outcome: "uncertain", detail: "reply lost", request_id: "" }),
  actAgain: (call: TeamCall): Promise<TeamOutcome> => Promise.resolve({ outcome: "uncertain", detail: "still unknown", request_id: call.request_id }),
  dismiss: (_call: TeamCall): Promise<void> => Promise.resolve(),
}));
vi.mock("../../bridge", async (importOriginal) => ({ ...(await importOriginal<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: async (command: string, args: { call: TeamCall }) => {
    if (command === "team_action") { f.acts.push(args.call); return f.act(args.call); }
    if (command === "team_action_again") { f.again.push(args.call); return f.actAgain(args.call); }
    if (command === "team_intent_dismiss") { f.dismissed.push(args.call); return f.dismiss(args.call); }
    if (command === "teams_project") return { state: "ok", head: "0123456789abcdef0123456789abcdef01234567", branch: "main" };
    throw new Error(`unexpected ${command}`);
  },
}));
vi.mock("../../store/machines", async (importOriginal) => ({ ...(await importOriginal<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));

type Store = typeof import("../../store/teams");
let store: Store["teams"];
let NewTeam: typeof import("./NewTeam").NewTeam;
let catalog: typeof import("./fixtures").catalog;
const disposers: (() => void)[] = [];

beforeEach(async () => {
  vi.resetModules();
  ({ teams: store } = await import("../../store/teams"));
  ({ NewTeam } = await import("./NewTeam"));
  ({ catalog } = await import("./fixtures"));
  f.acts = []; f.again = []; f.dismissed = [];
  f.act = (call) => Promise.resolve({ outcome: "uncertain", detail: "reply lost", request_id: call.request_id });
  f.actAgain = (call) => Promise.resolve({ outcome: "uncertain", detail: "still unknown", request_id: call.request_id });
  f.dismiss = () => Promise.resolve();
});
afterEach(() => { disposers.splice(0).forEach((dispose) => dispose()); document.body.replaceChildren(); });

/** The form as the dialog wires it: Create runs the store's intent. */
function open(closed = vi.fn()) {
  const host = document.createElement("div");
  document.body.append(host);
  disposers.push(render(() => createComponent(NewTeam, {
    catalog, get busy() { return store.busy(); }, machineId: "00000000-0000-4000-8000-0000000000aa",
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: (path) => store.project(path),
    onCreate: (input) => store.run({ team: input.team, team_name: input.name, request_id: input.request_id, action: { kind: "create", name: input.name } }, "new-team"),
    onSaveTemplate: async () => {}, onClose: closed,
    get failure() { return store.failureFor("new-team")?.message; },
  }), host));
  return host;
}
async function fill(host: HTMLElement) {
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  (host.querySelector("[data-check-folder]") as HTMLButtonElement).click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
}
const button = (host: HTMLElement, label: string) => [...host.querySelectorAll("button")].find((candidate) => candidate.textContent === label) as HTMLButtonElement | undefined;
const settle = async () => { for (let n = 0; n < 6; n++) await Promise.resolve(); };

it("keeps one intent while the create is unknown and offers Check and Dismiss", async () => {
  const host = open();
  await fill(host);
  button(host, "Create team")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-create-uncertain]")).not.toBeNull());
  expect(button(host, "Create team")).toBeUndefined();
  expect((host.querySelector('input[placeholder="Fixes"]') as HTMLInputElement).disabled).toBe(true);
  button(host, "Check")!.click();
  await settle();
  expect(f.again.map((call) => call.request_id)).toEqual([f.acts[0].request_id]);
  expect(f.again[0].team).toBe(f.acts[0].team);
  expect(host.querySelector("[data-create-uncertain]")).not.toBeNull();
});

it.each(["pending", "uncertain"] as const)("a refused Dismiss keeps the %s create locked with its ids", async (state) => {
  f.act = (call) => Promise.resolve(state === "pending" ? { outcome: "pending", request_id: call.request_id } : { outcome: "uncertain", detail: "reply lost", request_id: call.request_id });
  f.dismiss = () => Promise.reject(state === "pending" ? "Only an uncertain action can be dismissed." : "The computer is offline.");
  const host = open();
  await fill(host);
  button(host, "Create team")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-create-uncertain]")).not.toBeNull());
  const first = store.createIntent();
  button(host, "Dismiss")!.click();
  await settle();
  expect(f.dismissed).toHaveLength(1);
  expect(store.createIntent()).toEqual(first);
  expect(store.unresolved()).toHaveLength(1);
  expect(button(host, "Create team")).toBeUndefined();
  expect(host.querySelector("[data-new-team-error]")?.textContent).toContain(state === "pending" ? "Only an uncertain action" : "offline");
  expect(f.acts).toHaveLength(1);
});

it("a dialog reopened before the reply adopts the create once it becomes unknown", async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  f.act = async (call) => { await gate; return { outcome: "uncertain", detail: "reply lost", request_id: call.request_id }; };
  const first = open();
  await fill(first);
  button(first, "Create team")!.click();
  await settle();
  expect(f.acts).toHaveLength(1);
  // Close while sending: the intent stays where it is.
  disposers.shift()!();
  first.remove();
  const second = open();
  expect(second.querySelector("[data-create-sending]"), "the reopened dialog shows the create in flight").not.toBeNull();
  expect((second.querySelector("[data-check-folder]") as HTMLButtonElement).disabled, "and is locked").toBe(true);
  expect(button(second, "Create team")).toBeUndefined();
  release();
  await vi.waitFor(() => expect(second.querySelector("[data-create-uncertain]")).not.toBeNull());
  expect(button(second, "Create team")).toBeUndefined();
  expect(f.acts).toHaveLength(1);
  expect(store.createIntent()?.request_id).toBe(f.acts[0].request_id);
});

it("a form whose busy flag says nothing still locks while its create is in flight", async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  f.act = async (call) => { await gate; return { outcome: "uncertain", detail: "reply lost", request_id: call.request_id }; };
  const first = open();
  await fill(first);
  button(first, "Create team")!.click();
  await settle();
  disposers.shift()!(); first.remove();
  const host = document.createElement("div");
  document.body.append(host);
  disposers.push(render(() => createComponent(NewTeam, {
    catalog, busy: false, machineId: "00000000-0000-4000-8000-0000000000aa", defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: (path) => store.project(path), onCreate: async () => { throw new Error("no second create"); }, onSaveTemplate: async () => {}, onClose: () => {},
  }), host));
  expect((host.querySelector("[data-check-folder]") as HTMLButtonElement).disabled).toBe(true);
  expect((host.querySelector('input[placeholder="Fixes"]') as HTMLInputElement).disabled).toBe(true);
  release();
  await vi.waitFor(() => expect(host.querySelector("[data-create-uncertain]")).not.toBeNull());
});

it("only a Dismiss the core took lets a new intent with new ids start", async () => {
  const host = open();
  await fill(host);
  button(host, "Create team")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-create-uncertain]")).not.toBeNull());
  button(host, "Dismiss")!.click();
  await vi.waitFor(() => expect(button(host, "Create team")).toBeDefined());
  expect(store.createIntent()).toBeNull();
  f.act = (call) => Promise.resolve({ outcome: "done", receipt: {}, request_id: call.request_id });
  button(host, "Create team")!.click();
  await settle();
  expect(f.acts).toHaveLength(2);
  expect(f.acts[1].team).not.toBe(f.acts[0].team);
  expect(f.acts[1].request_id).not.toBe(f.acts[0].request_id);
  expect(store.createIntent()).toBeNull();
});

it("a refusal ends the intent and shows its reason, also in a dialog reopened after it", async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  f.act = async (call) => { await gate; return { outcome: "failed", detail: "invalid_template: The template was refused.", code: "invalid_template", request_id: call.request_id }; };
  const first = open();
  await fill(first);
  button(first, "Create team")!.click();
  await settle();
  disposers.shift()!();
  first.remove();
  release();
  await vi.waitFor(() => expect(store.createIntent()).toBeNull());
  const second = open();
  expect(second.querySelector("[data-new-team-error]")?.textContent).toContain("The template was refused.");
  await fill(second);
  expect(button(second, "Create team")).toBeDefined();
});

it("a Check that finds the team created closes the dialog", async () => {
  const closed = vi.fn();
  const host = open(closed);
  await fill(host);
  button(host, "Create team")!.click();
  await vi.waitFor(() => expect(host.querySelector("[data-create-uncertain]")).not.toBeNull());
  f.actAgain = (call) => Promise.resolve({ outcome: "done", receipt: {}, request_id: call.request_id });
  button(host, "Check")!.click();
  await vi.waitFor(() => expect(closed).toHaveBeenCalled());
  expect(store.createIntent()).toBeNull();
});
