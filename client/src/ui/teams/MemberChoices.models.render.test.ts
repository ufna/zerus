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

type Reply = { account: string; revision: string; catalog: { models: { id: string }[] } };
type Replies = { freest: Record<string, Reply>; named: Record<string, Reply>; refusals: { no_account: string; too_many: string; changed: string } };
const f = vi.hoisted(() => ({ reads: [] as string[], fail: new Set<string>(), none: new Set<string>(), many: new Set<string>(), accounts: undefined as unknown,
  replies: {} as Replies }));
vi.mock("../../bridge", async (original) => ({ ...(await original<object>()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false, invoke: async (command: string, args: { agent: string; account: string }) => {
  if (command === "accounts_view") return f.accounts;
  if (command === "team_member_models") {
    f.reads.push(`${args.agent}/${args.account}`);
    // The command's replies for the freest account ("") per agent and for a
    // named account, and its own refusals (the fixture holds their text).
    if (!args.account && f.none.has(args.agent)) throw f.replies.refusals.no_account;
    if (!args.account && f.many.has(args.agent)) throw f.replies.refusals.too_many;
    const reply = args.account ? f.replies.named[args.account] : f.replies.freest[args.agent];
    if (f.fail.has(args.agent) || !reply) throw f.replies.refusals.changed;
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
  vi.resetModules(); f.reads = []; f.fail = new Set(); f.none = new Set(); f.many = new Set(); f.accounts = structuredClone(accounts); f.replies = structuredClone(replies) as unknown as Replies;
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

it("lists the models every account of the agent offers for the freest account, and no free text", async () => {
  const { host, sent } = addMember();
  await flush();
  expect(host.querySelector('input[aria-label="Model"]'), "no free-text model").toBeNull();
  expect(host.querySelector('input[aria-label="Effort"]'), "no free-text effort").toBeNull();
  // The role's agent is claude and its account the freest: the launch may
  // pick Work or Spare, so model-a (both) is a plain choice and model-b (Spare
  // only) comes with Spare.
  expect(f.reads).toEqual(["claude/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"], ["@claude-spare/model-b", "Second model (only on Spare)"]]);
  expect(host.textContent).toContain("Models every account of this agent offers. A model only some offer is listed with its account.");
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
  expect(host.textContent, "a named account needs no note").not.toContain("Models every account");
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"]]);
  await chooseNative(host, "Agent", "codex");
  // The agent change clears the account in the same change: no codex read of claude-work.
  expect(f.reads).toEqual(["claude/", "claude/claude-work", "codex/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"]]);
  expect(host.textContent, "one codex account: its own name").toContain("Models of codex-work.");
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
  // No codex account on this computer.
  f.none.add("codex");
  const { host, sent } = addMember();
  await flush();
  await chooseNative(host, "Agent", "codex");
  expect(f.reads).toEqual(["claude/", "codex/"]);
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"]]);
  expect(host.textContent).toContain("No account of this agent is on this computer.");
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
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"], ["@claude-spare/model-b", "Second model (only on Spare)"], ["retired-model", "retired-model (not in the current catalog)"]]);
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

it("uses the command's own refusal texts", async () => {
  const { NO_ACCOUNT, TOO_MANY } = await import("./member-edit");
  expect([NO_ACCOUNT, TOO_MANY]).toEqual([replies.refusals.no_account, replies.refusals.too_many]);
});

// #2533 review (P2 freest): a model only some accounts offer is chosen with
// one of them, named, so the launch uses an account that has it.
it("choosing a model only one account offers names that account and reads its catalog", async () => {
  const { host, sent } = addMember();
  await flush();
  await pick(host, "Model", "@claude-spare/model-b");
  expect(f.reads).toEqual(["claude/", "claude/claude-spare"]);
  expect(host.querySelector<HTMLSelectElement>('select[aria-label="Account"]')!.value).toBe("claude-spare");
  expect(combo(host, "Model")!.textContent).toContain("Second model");
  expect(await offered(host, "Effort")).toEqual([["", "Default effort"], ["low", "low"]]);
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "model-b", effort: "", account: { mode: "named", account: "claude-spare" } }]]);
});

it("says why the freest account cannot list models when the agent has too many accounts", async () => {
  f.many.add("kimi");
  const { host } = addMember();
  await flush();
  await chooseNative(host, "Agent", "kimi");
  expect(host.textContent).toContain("This agent has too many accounts to compare their models; choose an account.");
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"]]);
});

// #2533 review (P2 refresh; Review2533.models.test.ts adopted): an open
// chooser follows the Accounts revision. A newer projection reads the
// catalog again, also after a first read that failed.
it("an open chooser reads its catalog again when the Accounts revision moves", async () => {
  const { host } = addMember();
  await flush();
  expect(f.reads).toEqual(["claude/"]);
  const { loadAccounts } = await import("../../store/accounts");
  f.accounts = { ...(f.accounts as { revision?: number }), revision: 55 };
  const claude = f.replies.freest.claude;
  claude.revision = "r2";
  claude.catalog.models[0].id = "replacement-model";
  await loadAccounts(true);
  await flush();
  expect(f.reads).toEqual(["claude/", "claude/"]);
  expect((await offered(host, "Model")).map(([value]) => value)).toEqual(["", "replacement-model", "@claude-spare/model-b"]);
});

it("a first read that failed recovers in the same open dialog when the Accounts revision moves", async () => {
  f.fail.add("claude");
  const { host } = addMember();
  await flush();
  expect(host.textContent).toContain("The account's models could not be read: its defaults apply.");
  f.fail.clear();
  const { loadAccounts } = await import("../../store/accounts");
  f.accounts = { ...(f.accounts as { revision?: number }), revision: 56 };
  await loadAccounts(true); await flush();
  expect((await offered(host, "Model")).map(([value]) => value)).toEqual(["", "model-a", "@claude-spare/model-b"]);
});

// WebKitGTK reopened the list after every pick: a <label> around the
// Dropdown forwards the option's click to its control once the closing list
// has left the label. The fields name their Dropdowns with a sibling label.
it("closes the model list after a pick and names each list without wrapping it in a label", async () => {
  const { host } = addMember();
  await flush();
  for (const name of ["Model", "Effort"]) {
    const box = combo(host, name)!;
    expect(box.closest("label"), `${name} is not inside a label`).toBeNull();
    expect(host.querySelector<HTMLLabelElement>(`label[for="${box.id}"]`)?.textContent).toBe(name);
  }
  await pick(host, "Model", "model-a");
  expect(combo(host, "Model")!.getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector('[role="listbox"]')).toBeNull();
});

// #2533 delta review (P2-1; review2529-cx's MemberChoices.round3-review and
// delta-extra probes adopted). Only a stopped member's own saved model keeps
// a plain "not in the current catalog" option. An Add preset or a new pick
// that the freest account does not offer becomes its "(only on <account>)"
// choice when one account offers it, else the account's defaults, with a
// note; a catalog refresh never turns the person's pick into a plain option.
function addWith(model: string, effort = "") {
  const roles = view.member_roles!.map((role) => role.id === "developer" ? { ...role, model, effort } : role);
  const sent: unknown[] = [];
  const host = mount(AddMemberDialog, { view: { ...view, member_roles: roles }, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  return { host, sent };
}
const adjusted = (host: HTMLElement) => host.querySelector("[data-member-choice-adjusted]")?.textContent ?? "";

it("names the one account that offers a role's preset model instead of sending it with the freest account", async () => {
  const { host, sent } = addWith("model-b");
  await flush(); await flush();
  expect(f.reads).toEqual(["claude/", "claude/claude-spare"]);
  expect(host.querySelector<HTMLSelectElement>('select[aria-label="Account"]')!.value).toBe("claude-spare");
  expect(adjusted(host)).toBe("Second model is only on Spare, so that account is chosen.");
  const options = await offered(host, "Model");
  expect(options.some(([, label]) => label?.includes("not in the current catalog")), "no plain fallback").toBe(false);
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "model-b", effort: "", account: { mode: "named", account: "claude-spare" } }]]);
});

it("falls back to the account's defaults, saying so, for a preset model no account offers", async () => {
  const { host, sent } = addWith("gone-model", "high");
  await flush(); await flush();
  expect(adjusted(host)).toBe("gone-model is not offered here, so the account's defaults apply.");
  expect(combo(host, "Model")!.textContent).toContain("Account defaults");
  expect((await offered(host, "Model")).map(([value]) => value)).toEqual(["", "model-a", "@claude-spare/model-b"]);
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "", effort: "", account: { mode: "freest" } }]]);
});

it("does not treat a newly picked model as the stopped member's legacy model when a refresh removes it", async () => {
  const member = { ...view.members.find((m) => m.agent === "claude" && !m.lead)!, state: "paused", person_stopped: true, open_work: 0,
    model: "retired-model", effort: "", account_choice: { mode: "freest" } } as TeamMember;
  const sent: unknown[] = [];
  const host = mount(SwapMemberDialog, { member, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  await flush();
  await pick(host, "Model", "model-a");
  const { loadAccounts } = await import("../../store/accounts");
  f.accounts = { ...(f.accounts as { revision?: number }), revision: 57 };
  f.replies.freest.claude.revision = "r2";
  f.replies.freest.claude.catalog.models[0].id = "replacement";
  await loadAccounts(true); await flush(); await flush();
  const options = await offered(host, "Model");
  expect(options).toContainEqual(["retired-model", "retired-model (not in the current catalog)"]);
  expect(options.some(([value]) => value === "model-a"), "the new pick never becomes a plain option").toBe(false);
  expect(adjusted(host)).toBe("model-a is not offered here, so the account's defaults apply.");
  submit(host).click(); await flush();
  expect(sent).toEqual([["swap_member", { member: member.id, agent: "claude", model: "", effort: "", account: { mode: "freest" } }]]);
});

it("pins the one account that still offers a picked model after a refresh", async () => {
  const { host, sent } = addMember();
  await flush();
  await pick(host, "Model", "model-a");
  const { loadAccounts } = await import("../../store/accounts");
  f.accounts = { ...(f.accounts as { revision?: number }), revision: 58 };
  const claude = f.replies.freest.claude as unknown as { revision: string; catalog: { models: unknown[] }; on_some: { id: string; label: string; accounts: string[] }[] };
  claude.revision = "r3";
  claude.catalog.models = [];
  claude.on_some = [{ id: "model-a", label: "Example model", accounts: ["claude-work"] }];
  await loadAccounts(true); await flush(); await flush();
  expect(host.querySelector<HTMLSelectElement>('select[aria-label="Account"]')!.value).toBe("claude-work");
  expect(adjusted(host)).toBe("Example model is only on Work, so that account is chosen.");
  submit(host).click(); await flush();
  expect(sent).toEqual([["add_member", { role: "developer", agent: "claude", model: "model-a", effort: "", account: { mode: "named", account: "claude-work" } }]]);
});

// The round-1 exception stays exact: a stopped member's own saved model keeps
// its plain choice even when only some accounts list it, beside the
// "(only on <account>)" choice; it is not settled away, and Swap can send it.
it("keeps a stopped member's saved model that only some accounts list as its own choice", async () => {
  const member = { ...view.members.find((m) => m.agent === "claude" && !m.lead)!, state: "paused", person_stopped: true, open_work: 0,
    model: "model-b", effort: "", account_choice: { mode: "freest" } } as TeamMember;
  const sent: unknown[] = [];
  const host = mount(SwapMemberDialog, { member, busy: false, error: "", close: () => {}, submit: async (kind: string, input: unknown) => { sent.push([kind, input]); } });
  await flush(); await flush();
  expect(await offered(host, "Model")).toEqual([["", "Account defaults"], ["model-a", "Example model"], ["@claude-spare/model-b", "Second model (only on Spare)"], ["model-b", "model-b (not in the current catalog)"]]);
  expect(adjusted(host)).toBe("");
  expect(submit(host).disabled).toBe(false);
  submit(host).click(); await flush();
  expect(sent).toEqual([["swap_member", { member: member.id, agent: "claude", model: "model-b", effort: "", account: { mode: "freest" } }]]);
});
