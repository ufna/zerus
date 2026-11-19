// Field layout and dependent-choice resets adapted from WhiteWh's add/swap
// dialogs. Our launch writer decides the account; this form grants no effects.
// Model and effort are chosen from the Accounts model catalog, the one
// Accounts > Default model and effort reads (zerus-efeo3 G16), never typed.
import { For, Show, batch, createEffect, createMemo, createResource, createSignal, createUniqueId, onMount } from "solid-js";
import { teamMemberModels } from "../../bridge";
import { accountsPage, loadAccounts } from "../../store/accounts";
import { tr } from "../../i18n";
import { Dropdown } from "../lib/Dropdown";
import { MEMBER_AGENTS, NO_ACCOUNT, TOO_MANY, memberAccounts, memberModelView, type MemberChoice } from "./member-edit";
export function MemberChoices(props: { initial: MemberChoice; disabled: boolean; onChange: (choice: MemberChoice) => void; onValid?: (valid: boolean) => void }) {
  const [agent, setAgent] = createSignal(props.initial.agent);
  const [model, setModel] = createSignal(props.initial.model);
  const [effort, setEffort] = createSignal(props.initial.effort);
  const [account, setAccount] = createSignal(props.initial.account.mode === "named" ? props.initial.account.account : "");
  const id = createUniqueId();
  const accounts = createMemo(() => memberAccounts(accountsPage(), agent()));
  // The chosen account's catalog lists the models. For the freest account
  // ("") the native side lists the models every account of the agent offers,
  // since the launch may pick any of them; a model only some offer comes with
  // those accounts (#2533 review). DeepSeek keeps its model native.
  // The Accounts revision is part of the key (#2533 review): an open dialog
  // reads again when the accounts change, also after a read that failed.
  const [read] = createResource(() => agent() === "dsh" ? false : `${agent()}\n${account()}\n${accountsPage().revision ?? 0}`, async (key) => {
    const [forAgent, forAccount] = key.split("\n");
    try { return { reply: await teamMemberModels(forAgent, forAccount) }; } catch (error) { return { error: String(error) }; }
  });
  const answer = () => read.loading ? undefined : read();
  const reply = () => { const got = answer()?.reply; return got && got.account === account() ? got : undefined; };
  const catalog = () => reply()?.catalog;
  const view = createMemo(() => memberModelView(reply(), { agent: agent(), model: model(), effort: effort() }, props.initial));
  // Accounts the command listed: an account a model option named keeps its
  // label even before the window's own Accounts list has it.
  const listed = createMemo<Record<string, string>>((seen) => ({ ...seen, ...Object.fromEntries((reply()?.accounts ?? []).map((a) => [a.id, a.label])) }), {});
  const changed = () => props.onChange({ agent: agent(), model: model(), effort: effort(), account: account() ? { mode: "named", account: account() } : { mode: "freest" } });
  const chooseModel = (value: string) => {
    // A model only some accounts offer is chosen with its account, named.
    const named = view().onSome.find((option) => option.value === value);
    batch(() => {
      if (named) { setAccount(named.account); setModel(named.model); setEffort(""); return; }
      setModel(value);
      if (!view().efforts.some((option) => option.value === effort())) setEffort("");
    });
    changed();
  };
  createEffect(() => props.onValid?.(view().valid));
  onMount(() => { void loadAccounts().catch(() => undefined); });
  const note = () => {
    if (!view().modelShown) return "";
    if (read.loading) return tr("Reading the account's models…");
    const refusal = answer()?.error;
    if (refusal === NO_ACCOUNT) return tr("No account of this agent is on this computer.");
    if (refusal === TOO_MANY) return tr("This agent has too many accounts to compare their models; choose an account.");
    if (!catalog()) return tr("The account's models could not be read: its defaults apply.");
    const of = reply()?.accounts ?? [];
    if (account()) return "";
    return of.length === 1 ? tr("Models of %1.", of[0].label) : tr("Models every account of this agent offers. A model only some offer is listed with its account.");
  };
  return <div class="team-member-edit-fields">
    <label>{tr("Agent")}<select class="field" aria-label={tr("Agent")} value={agent()} disabled={props.disabled} onChange={(e) => { const next = e.currentTarget.value; batch(() => { setAgent(next); setAccount(""); setModel(""); setEffort(""); }); changed(); }}>
      <For each={MEMBER_AGENTS}>{(a) => <option value={a}>{a === "dsh" ? "DeepSeek" : a}</option>}</For>
    </select></label>
    <Show when={view().modelShown}>
      <label>{tr("Model")}<Dropdown class="field" aria-label={tr("Model")} aria-describedby={id} value={model()} disabled={props.disabled} onChange={chooseModel} options={view().models} /></label>
    </Show>
    <Show when={view().effortShown}>
      <label>{tr("Effort")}<Dropdown class="field" aria-label={tr("Effort")} value={effort()} disabled={props.disabled || !model()} onChange={(value) => { setEffort(value); changed(); }} options={view().efforts} /></label>
    </Show>
    <label>{tr("Account")}<select class="field" aria-label={tr("Account")} value={account()} disabled={props.disabled} onChange={(e) => { const next = e.currentTarget.value; batch(() => { setAccount(next); setModel(""); setEffort(""); }); changed(); }}>
      <option value="">{tr("The freest account")}</option>
      <Show when={account() && !accounts().some((a) => a.id === account())}><option value={account()}>{listed()[account()] ?? tr("%1 (not in the current catalog)", account())}</option></Show>
      <For each={accounts()}>{(a) => <option value={a.id} disabled={!a.signedIn}>{a.signedIn ? a.label : tr("%1 (signed out)", a.label)}</option>}</For>
    </select></label>
    <Show when={agent() === "dsh"}><p class="dim">{tr("DeepSeek model is managed in its native account.")}</p></Show>
    <Show when={note()}><p class="dim" id={id}>{note()}</p></Show>
    <Show when={view().problem}><p class="danger" role="status" data-member-choice-problem>{view().problem}</p></Show>
  </div>;
}
