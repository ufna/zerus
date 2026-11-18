// Field layout and dependent-choice resets adapted from WhiteWh's add/swap
// dialogs. Our launch writer decides the account; this form grants no effects.
// Model and effort are chosen from the Accounts model catalog, the one
// Accounts > Default model and effort reads (zerus-efeo3 G16), never typed.
import { For, Show, batch, createEffect, createMemo, createResource, createSignal, createUniqueId, onMount } from "solid-js";
import { teamMemberModels } from "../../bridge";
import { accountsPage, loadAccounts } from "../../store/accounts";
import { tr } from "../../i18n";
import { Dropdown } from "../lib/Dropdown";
import { MEMBER_AGENTS, NO_SIGNED_IN, memberAccounts, memberModelView, type MemberChoice } from "./member-edit";
export function MemberChoices(props: { initial: MemberChoice; disabled: boolean; onChange: (choice: MemberChoice) => void; onValid?: (valid: boolean) => void }) {
  const [agent, setAgent] = createSignal(props.initial.agent);
  const [model, setModel] = createSignal(props.initial.model);
  const [effort, setEffort] = createSignal(props.initial.effort);
  const [account, setAccount] = createSignal(props.initial.account.mode === "named" ? props.initial.account.account : "");
  const id = createUniqueId();
  const accounts = createMemo(() => memberAccounts(accountsPage(), agent()));
  // The chosen account's catalog lists the models. For the freest account
  // ("") the native side reads the first local account of the agent that
  // was signed in when last inspected, as `account ls` keeps it, and keeps
  // each catalog for its accounts revision. DeepSeek keeps its model native.
  const [read] = createResource(() => agent() === "dsh" ? false : `${agent()}\n${account()}`, async (key) => {
    const [forAgent, forAccount] = key.split("\n");
    try { return { reply: await teamMemberModels(forAgent, forAccount) }; } catch (error) { return { error: String(error) }; }
  });
  const answer = () => read.loading ? undefined : read();
  const catalog = () => { const reply = answer()?.reply; return reply && (!account() || reply.account === account()) ? reply.catalog : undefined; };
  const view = createMemo(() => memberModelView(catalog(), { agent: agent(), model: model(), effort: effort() }, props.initial));
  const changed = () => props.onChange({ agent: agent(), model: model(), effort: effort(), account: account() ? { mode: "named", account: account() } : { mode: "freest" } });
  const chooseModel = (value: string) => {
    batch(() => {
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
    if (answer()?.error === NO_SIGNED_IN) return tr("No signed-in account of this agent here: its account's defaults apply.");
    if (!catalog()) return tr("The account's models could not be read: its defaults apply.");
    const from = answer()?.reply?.account ?? "";
    return account() ? "" : tr("Models of %1.", accounts().find((a) => a.id === from)?.label ?? from);
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
      <Show when={account() && !accounts().some((a) => a.id === account())}><option value={account()}>{tr("%1 (not in the current catalog)", account())}</option></Show>
      <For each={accounts()}>{(a) => <option value={a.id} disabled={!a.signedIn}>{a.signedIn ? a.label : tr("%1 (signed out)", a.label)}</option>}</For>
    </select></label>
    <Show when={agent() === "dsh"}><p class="dim">{tr("DeepSeek model is managed in its native account.")}</p></Show>
    <Show when={note()}><p class="dim" id={id}>{note()}</p></Show>
    <Show when={view().problem}><p class="danger" role="status" data-member-choice-problem>{view().problem}</p></Show>
  </div>;
}
