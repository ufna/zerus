// @vitest-environment happy-dom
// G16 (zerus-efeo3): Add member and Swap choose model and effort from the
// Accounts model catalog, as Accounts > Default model and effort does, never
// as free text. Inputs are product output: the accounts rows are client
// core's projection (views::accounts::tests::the_member_accounts_fixture_is_this_projection),
// the catalogs are the team_member_models command's own replies
// (screens::member_models::tests::the_member_models_fixture_is_this_command_reply),
// and the team view is the Rust writer's (src/teams/cli/tests/lanes.rs).
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import accounts from "./fixtures/member-accounts.json";
import replies from "./fixtures/member-models.json";
import lanes from "./fixtures/lanes-view.json";
import type { TeamMember, TeamView } from "../../store/teams";

type Reply = { account: string };
const f = vi.hoisted(() => ({ reads: [] as string[], fail: new Set<string>(), unsigned: new Set<string>(), accounts: undefined as unknown,
  replies: {} as { claude: Reply; kimi: Reply; codex: Reply; refusals: { no_signed_in: string; changed: string } } }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false, invoke: async (command: string, args: { agent: string; account: string }) => {
  if (command === "accounts_view") return f.accounts;
  if (command === "team_member_models") {
    f.reads.push(`${args.agent}/${args.account}`);
    // The command's replies for the freest account ("") and a named one, and
    // its own refusals (the fixture holds their exact text).
    const reply = (f.replies as unknown as Record<string, Reply>)[args.agent];
    if (!args.account && f.unsigned.has(args.agent)) throw f.replies.refusals.no_signed_in;
    if (f.fail.has(args.agent) || !reply || (args.account && reply.account !== args.account)) throw f.replies.refusals.changed;
    return reply;
  }
  throw new Error(`Unexpected invoke ${command}`);
} }));

const view = (lanes as unknown as { after: TeamView }).after;
const disposers: (() => void)[] = [];
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
let AddMemberDialog: typeof import("./AddMemberDialog").AddMemberDialog;
let SwapMemberDialog: typeof import("./SwapMemberDialog").SwapMemberDialog;
beforeEach(async () => {
  // The bridge's own calls ask whether this is the desktop window.
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  vi.resetModules(); f.reads = []; f.fail = new Set(); f.unsigned = new Set(); f.accounts = structuredClone(accounts); f.replies = structuredClone(replies) as typeof f.replies;
  ({ AddMemberDialog } = await import("./AddMemberDialog")); ({ SwapMemberDialog } = await import("./SwapMemberDialog"));
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });
function mount<T>(component: (props: T) => unknown, props: T) {
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => createComponent(component as never, props as never), host)); return host;
}
function addMember() {
  const sent: unknown[] = [];
  const host = mount(AddMemberDialog, { view, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  return { host, sent };
}
const combo = (host: HTMLElement, name: string) => host.querySelector<HTMLButtonElement>(`[role="combobox"][aria-label="${name}"]`);
const offered = async (host: HTMLElement, name: string) => {
  combo(host, name)!.click(); await flush();
  const values = [...document.querySelectorAll('[role="option"]')].map((o) => [o.getAttribute("data-value"), o.textContent]);
  combo(host, name)!.click(); await flush(); return values;
};
async function pick(host: HTMLElement, name: string, value: string) {
  combo(host, name)!.click(); await flush();
  document.querySelector<HTMLButtonElement>(`[role="option"][data-value="${value}"]`)!.click(); await flush();
}
const chooseNative = async (host: HTMLElement, name: string, value: string) => {
  const select = host.querySelector<HTMLSelectElement>(`select[aria-label="${name}"]`)!;
  select.value = value; select.dispatchEvent(new Event("change", { bubbles: true })); await flush();
};
const submit = (host: HTMLElement) => host.querySelector<HTMLButtonElement>('button[type="submit"]')!;

it("lists the freest account's models, read by the native side from its stand-in account, and no free text", async () => {
  const { host, sent } = addMember();
  await flush();
  expect(host.querySelector('input[aria-label="Model"]'), "no free-text model").toBeNull();
  expect(host.querySelector('input[aria-label="Effort"]'), "no free-text effort").toBeNull();
  // The role's agent is claude and its account the freest: the command picks
  // the stand-in (claude-work, signed in when last inspected) and names it.
  expect(f.reads).toEqual(["claude/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"]]);
  expect(host.textContent).toContain("Models of Work.");
  await pick(host, "Model", "model-a");
  expect(await offered(host, "Effort")).toEqual([["", "Default effort"], ["low", "low"], ["high", "high"]]);
  await pick(host, "Effort", "high");
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "model-a", effort: "high", account: { mode: "freest" } }]]);
});

it("reads a named account's own catalog, and the freest again after the agent changes", async () => {
  const { host } = addMember();
  await flush();
  await chooseNative(host, "Account", "claude-work");
  expect(f.reads).toEqual(["claude/", "claude/claude-work"]);
  expect(host.textContent, "a named account needs no stand-in note").not.toContain("Models of");
  await chooseNative(host, "Agent", "codex");
  // The agent change clears the account in the same change: no codex read of claude-work.
  expect(f.reads).toEqual(["claude/", "claude/claude-work", "codex/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"]]);
});

it("hides effort for Kimi, whose catalog has no effort list", async () => {
  const { host, sent } = addMember();
  await flush();
  await chooseNative(host, "Agent", "kimi");
  expect(f.reads).toContain("kimi/");
  expect(combo(host, "Model")).not.toBeNull();
  expect(combo(host, "Effort"), "no effort for Kimi").toBeNull();
  await pick(host, "Model", "model-a");
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "kimi", model: "model-a", effort: "", account: { mode: "freest" } }]]);
});

it("falls back to the account's defaults with a hint when nothing can be read", async () => {
  // No local codex account was signed in when last inspected.
  f.unsigned.add("codex");
  const { host, sent } = addMember();
  await flush();
  await chooseNative(host, "Agent", "codex");
  expect(f.reads).toEqual(["claude/", "codex/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"]]);
  expect(host.textContent).toContain("No signed-in account of this agent here: its account's defaults apply.");
  // A read that fails says so and keeps the defaults.
  f.fail.add("claude");
  await chooseNative(host, "Agent", "claude");
  await flush();
  expect(host.textContent).toContain("The account's models could not be read: its defaults apply.");
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"]]);
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "", effort: "", account: { mode: "freest" } }]]);
});

it("keeps Swap's current model and effort selectable and marked when the catalog lacks them", async () => {
  const member = { ...view.members.find((m) => m.agent === "claude" && !m.lead)!, state: "paused", person_stopped: true, open_work: 0,
    model: "retired-model", effort: "max", account_choice: { mode: "freest" } } as TeamMember;
  const sent: unknown[] = [];
  const host = mount(SwapMemberDialog, { member, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  await flush();
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"], ["retired-model", "retired-model (not in the current catalog)"]]);
  expect(combo(host, "Model")!.textContent).toContain("retired-model (not in the current catalog)");
  expect(await offered(host, "Effort")).toEqual([["", "Default effort"], ["max", "max (not in the current catalog)"]]);
  expect(submit(host).disabled).toBe(false);
  // A model from the catalog drops the old effort it does not offer.
  await pick(host, "Model", "model-a");
  expect(await offered(host, "Effort")).toEqual([["", "Default effort"], ["low", "low"], ["high", "high"]]);
  submit(host).click(); await flush();
  expect(sent).toEqual([["swap_member", { member: member.id, agent: "claude", model: "model-a", effort: "", account: { mode: "freest" } }]]);
});

it("keeps DeepSeek's model native and offers no effort", async () => {
  const { host } = addMember();
  await flush();
  await chooseNative(host, "Agent", "dsh");
  expect(combo(host, "Model")).toBeNull();
  expect(combo(host, "Effort")).toBeNull();
  expect(host.textContent).toContain("DeepSeek model is managed in its native account.");
  expect(f.reads).toEqual(["claude/"]);
});

it("says why a role's effort without a model cannot be added until a model is chosen", async () => {
  // A catalog role may name an effort and no model; Catalog::validate refuses that.
  const roles = view.member_roles!.map((role) => role.id === "developer" ? { ...role, effort: "high" } : role);
  const sent: unknown[] = [];
  const host = mount(AddMemberDialog, { view: { ...view, member_roles: roles }, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  await flush();
  expect(host.querySelector("[data-member-choice-problem]")?.textContent).toBe("Choose a model before setting effort.");
  expect(submit(host).disabled).toBe(true);
  submit(host).click(); await flush();
  expect(sent).toEqual([]);
  // A model that offers the role's effort keeps it.
  await pick(host, "Model", "model-a");
  expect(host.querySelector("[data-member-choice-problem]")).toBeNull();
  expect(submit(host).disabled).toBe(false);
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "model-a", effort: "high", account: { mode: "freest" } }]]);
});

it("uses the command's own refusal text for no signed-in account", async () => {
  const { NO_SIGNED_IN } = await import("./member-edit");
  expect(NO_SIGNED_IN).toBe(replies.refusals.no_signed_in);
});
