// Field layout and dependent-choice resets adapted from WhiteWh's add/swap
// dialogs. Our launch writer decides the account; this form grants no effects.
// Model and effort are chosen from the Accounts model catalog, the one
// Accounts > Default model and effort reads (zerus-efeo3 G16), never typed.
import { For, Show, batch, createEffect, createMemo, createResource, createSignal, createUniqueId, on, onMount, untrack } from "solid-js";
import { teamMemberModels } from "../../bridge";
import { accountsPage, loadAccounts } from "../../store/accounts";
import { tr } from "../../i18n";
import { Dropdown } from "../lib/Dropdown";
import { MEMBER_AGENTS, NO_ACCOUNT, TOO_MANY, memberAccounts, memberModelView, type MemberChoice, type SavedChoice } from "./member-edit";
/** `initial` is where the form starts (a role preset, or the member being
 * swapped). `saved` is a stopped member's own saved model and effort: only
 * they stay selectable while the catalog lacks them (#2533 delta review). */
export function MemberChoices(props: { initial: MemberChoice; saved?: SavedChoice; disabled: boolean; onChange: (choice: MemberChoice) => void; onValid?: (valid: boolean) => void }) {
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
  const view = createMemo(() => memberModelView(reply(), { agent: agent(), model: model(), effort: effort() }, props.saved));
  // Accounts the command listed: an account a model option named keeps its
  // label even before the window's own Accounts list has it.
  const listed = createMemo<Record<string, string>>((seen) => ({ ...seen, ...Object.fromEntries((reply()?.accounts ?? []).map((a) => [a.id, a.label])) }), {});
  const changed = () => props.onChange({ agent: agent(), model: model(), effort: effort(), account: account() ? { mode: "named", account: account() } : { mode: "freest" } });
  // A preset or a pick the answer does not offer is settled when the answer
  // arrives, never kept as a plain choice: a model only one account offers
  // names that account, any other goes back to the account's defaults, and
  // the form says which (#2533 delta review). A failed read changes nothing;
  // the problem line then keeps the form from being sent.
  const [adjusted, setAdjusted] = createSignal("");
  createEffect(on(answer, (got) => {
    const now = untrack(model), shown = got?.reply;
    if (!now || !shown || shown.account !== untrack(account) || untrack(view).models.some((option) => option.value === now)) return;
    const holders = !shown.account ? shown.on_some.find((entry) => entry.id === now) : undefined;
    batch(() => {
      if (holders && holders.accounts.length === 1) {
        const [only] = holders.accounts;
        setAccount(only);
        setAdjusted(tr("%1 is only on %2, so that account is chosen.", holders.label, shown.accounts.find((a) => a.id === only)?.label || only));
      } else {
        setModel(""); setEffort("");
        setAdjusted(tr("%1 is not offered here, so the account's defaults apply.", now));
      }
    });
    changed();
  }));
  const chooseModel = (value: string) => {
    // A model only some accounts offer is chosen with its account, named.
    const named = view().onSome.find((option) => option.value === value);
    setAdjusted("");
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
  // A Dropdown is named by a sibling label, never wrapped in one: a wrapping
  // label forwards an option's click to the list's button after the list
  // closes, and WebKitGTK reopened it.
  return <div class="team-member-edit-fields">
    <label>{tr("Agent")}<select class="field" aria-label={tr("Agent")} value={agent()} disabled={props.disabled} onChange={(e) => { const next = e.currentTarget.value; batch(() => { setAgent(next); setAccount(""); setModel(""); setEffort(""); setAdjusted(""); }); changed(); }}>
      <For each={MEMBER_AGENTS}>{(a) => <option value={a}>{a === "dsh" ? "DeepSeek" : a}</option>}</For>
    </select></label>
    <Show when={view().modelShown}>
      <div class="team-member-field"><label for={`${id}-model`}>{tr("Model")}</label><Dropdown id={`${id}-model`} class="field" aria-label={tr("Model")} aria-describedby={id} value={model()} disabled={props.disabled} onChange={chooseModel} options={view().models} /></div>
    </Show>
    <Show when={view().effortShown}>
      <div class="team-member-field"><label for={`${id}-effort`}>{tr("Effort")}</label><Dropdown id={`${id}-effort`} class="field" aria-label={tr("Effort")} value={effort()} disabled={props.disabled || !model()} onChange={(value) => { setEffort(value); changed(); }} options={view().efforts} /></div>
    </Show>
    <label>{tr("Account")}<select class="field" aria-label={tr("Account")} value={account()} disabled={props.disabled} onChange={(e) => { const next = e.currentTarget.value; batch(() => { setAccount(next); setModel(""); setEffort(""); setAdjusted(""); }); changed(); }}>
      <option value="">{tr("The freest account")}</option>
      <Show when={account() && !accounts().some((a) => a.id === account())}><option value={account()}>{listed()[account()] ?? tr("%1 (not in the current catalog)", account())}</option></Show>
      <For each={accounts()}>{(a) => <option value={a.id} disabled={!a.signedIn}>{a.signedIn ? a.label : tr("%1 (signed out)", a.label)}</option>}</For>
    </select></label>
    <Show when={agent() === "dsh"}><p class="dim">{tr("DeepSeek model is managed in its native account.")}</p></Show>
    <Show when={note()}><p class="dim" id={id}>{note()}</p></Show>
    <Show when={adjusted()}><p class="notice" role="status" data-member-choice-adjusted>{adjusted()}</p></Show>
    <Show when={view().problem}><p class="danger" role="status" data-member-choice-problem>{view().problem}</p></Show>
  </div>;
}
