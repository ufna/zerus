// Adapted from WhiteWh's member-dialog choices to our revisioned operations.
import type { AccountsView } from "../../types";
import type { MemberModels } from "../../bridge";
import type { TeamMember } from "../../store/teams";
import { tr } from "../../i18n";
export const MEMBER_AGENTS = ["claude", "codex", "kimi", "dsh"] as const;
export type AccountChoice = { mode: "freest" } | { mode: "named"; account: string };
export interface MemberChoice { agent: string; model: string; effort: string; account: AccountChoice }
/** Current local catalogs alone grant account membership; remote/evidence
 * rows never become launch choices. The launch writer rechecks sign-in. */
export function memberAccounts(view: AccountsView, agent: string) {
  const found = new Map<string, { id: string; label: string; signedIn: boolean }>();
  for (const account of view.accounts) for (const p of account.members) {
    if (p.host !== "" || p.provider !== agent || !p.installed) continue;
    found.set(p.id, { id: p.id, label: p.label || p.id, signedIn: p.signed_in });
  }
  return [...found.values()].sort((a, b) => a.label.localeCompare(b.label));
}
export function swapBlocked(member: TeamMember): string | undefined {
  if (member.state === "removed") return tr("This member was removed.");
  if (member.open_work === undefined || member.open_work === null) return tr("Refresh the team to check this member's open work.");
  // A member the person stopped keeps its assigned work: Swap waits for that
  // work to move, and asking to stop it again would be wrong (review of #2535).
  if (member.state !== "paused" || !member.person_stopped) return tr("Stop the member first; open work: %1", member.open_work);
  if (member.open_work !== 0) return tr("Move this member's open work to another member first; open work: %1", member.open_work);
  if (!member.session) return tr("This member has no confirmed session to replace.");
  return undefined;
}

/** Only fields the maintained native launch builders can deliver. */
export function supportedChoice(choice: MemberChoice): MemberChoice {
  return { ...choice, model: choice.agent === "dsh" ? "" : choice.model,
    effort: choice.agent === "claude" || choice.agent === "codex" ? choice.effort : "" };
}

/** team_member_models' refusals (client/src-tauri/src/screens/member_models.rs):
 * no account of the agent on this computer, and too many to compare for the
 * freest account. The window shows each with the defaults it falls back to. */
export const NO_ACCOUNT = "No account of this agent is on this computer.";
export const TOO_MANY = "This agent has too many accounts to compare their models; choose an account.";
export interface ModelOption { value: string; label: string }
/** A freest-account option for a model only some accounts list: choosing it
 * names that account (#2533 review). */
export interface OnSomeOption { value: string; account: string; model: string }
/** Add member and Swap's model and effort choices from the command's reply,
 * validated as client core's Catalog::validate: a model the catalog lists,
 * and an effort that model offers. For the freest account the catalog is the
 * models every account of the agent lists; a model only some list is offered
 * with one of those accounts, which choosing it names. The member's current
 * values stay selectable, marked, while the catalog lacks them (zerus-efeo3 G16). */
export function memberModelView(reply: MemberModels | undefined, now: { agent: string; model: string; effort: string }, initial: MemberChoice) {
  const catalog = reply?.catalog;
  const current = now.agent === initial.agent;
  const rows = (catalog?.models ?? []).slice(0, 256);
  const models: ModelOption[] = [{ value: "", label: tr("Account defaults") }, ...rows.map((m) => ({ value: m.id, label: m.label }))];
  const label = (account: string) => reply?.accounts.find((a) => a.id === account)?.label || account;
  const onSome: OnSomeOption[] = [];
  if (reply && !reply.account) for (const model of reply.on_some.slice(0, 256)) for (const account of model.accounts.slice(0, 16)) {
    const value = `@${account}/${model.id}`;
    onSome.push({ value, account, model: model.id });
    models.push({ value, label: tr("%1 (only on %2)", model.label, label(account)) });
  }
  if (current && initial.model && !rows.some((m) => m.id === initial.model)) models.push({ value: initial.model, label: tr("%1 (not in the current catalog)", initial.model) });
  const offered = catalog?.effort_supported ? rows.find((m) => m.id === now.model)?.effort_options ?? [] : [];
  const efforts: ModelOption[] = [{ value: "", label: tr("Default effort") }];
  if (now.model) {
    efforts.push(...offered.map((value) => ({ value, label: value })));
    if (current && now.model === initial.model && initial.effort && !offered.includes(initial.effort)) efforts.push({ value: initial.effort, label: tr("%1 (not in the current catalog)", initial.effort) });
  }
  // DeepSeek keeps its model native; the launch builders take effort for
  // Claude and Codex only, and Kimi's catalog offers none.
  const modelShown = now.agent !== "dsh";
  const effortShown = (now.agent === "claude" || now.agent === "codex") && catalog?.effort_supported !== false;
  const plain = (value: string) => !onSome.some((option) => option.value === value);
  const valid = models.some((o) => o.value === now.model && plain(o.value)) && efforts.some((o) => o.value === now.effort);
  // Catalog::validate's own reasons; a role can name an effort without a model.
  const problem = valid ? "" : now.effort && !now.model ? tr("Choose a model before setting effort.") : tr("Effort is unavailable for the selected model.");
  return { models, efforts, onSome, modelShown, effortShown, valid, problem };
}
