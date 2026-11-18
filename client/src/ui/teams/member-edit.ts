// Adapted from WhiteWh's member-dialog choices to our revisioned operations.
import type { AccountsView } from "../../types";
import type { AccountModelCatalog } from "../../bridge";
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
  if (member.state !== "paused" || !member.person_stopped || member.open_work !== 0)
    return tr("Stop the member first; open work: %1", member.open_work);
  if (!member.session) return tr("This member has no confirmed session to replace.");
  return undefined;
}

/** Only fields the maintained native launch builders can deliver. */
export function supportedChoice(choice: MemberChoice): MemberChoice {
  return { ...choice, model: choice.agent === "dsh" ? "" : choice.model,
    effort: choice.agent === "claude" || choice.agent === "codex" ? choice.effort : "" };
}

/** team_member_models' refusal when no local account of the agent was
 * signed in when last inspected (client/src-tauri/src/screens/member_models.rs). */
export const NO_SIGNED_IN = "No signed-in account of this agent here.";
export interface ModelOption { value: string; label: string }
/** Add member and Swap's model and effort choices from an account's model
 * catalog, validated as client core's Catalog::validate: a model the catalog
 * lists, and an effort that model offers. The member's current values stay
 * selectable, marked, while the catalog lacks them (zerus-efeo3 G16). */
export function memberModelView(catalog: AccountModelCatalog | undefined, now: { agent: string; model: string; effort: string }, initial: MemberChoice) {
  const current = now.agent === initial.agent;
  const rows = (catalog?.models ?? []).slice(0, 256);
  const models: ModelOption[] = [{ value: "", label: tr("Account defaults") }, ...rows.map((m) => ({ value: m.id, label: m.label }))];
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
  const valid = models.some((o) => o.value === now.model) && efforts.some((o) => o.value === now.effort);
  // Catalog::validate's own reasons; a role can name an effort without a model.
  const problem = valid ? "" : now.effort && !now.model ? tr("Choose a model before setting effort.") : tr("Effort is unavailable for the selected model.");
  return { models, efforts, modelShown, effortShown, valid, problem };
}
