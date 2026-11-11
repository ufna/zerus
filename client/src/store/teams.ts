import { invoke } from "@tauri-apps/api/core";
import { createRoot, createSignal, batch } from "solid-js";
import { inTauri } from "../bridge";
import type { TeamsOut } from "../types";

// The Teams views the desktop shows (M1): the team list, one followed team
// and the owner cards. Every value comes from the `teams` key of the per-frame
// batch; the store asks package 12's Tauri commands to follow, to read a page
// and to act, and never runs `hgs` or starts a timer of its own. An action has
// five outcomes; the person's confirmation is asked by the core (`confirm`),
// and an uncertain action only offers Check and Dismiss.

/** A member's session: the computer UUID and the session identity. */
export interface TeamSession { machine: string; session_id: string }
export interface TeamSummary {
  id: string; project: string; name: string; status: string; lead: string; home: string; revision: number;
  /** The lead's bound session, when package 5 sends it: how a session finds its team. */
  lead_session?: TeamSession | null;
}
export interface TeamList { teams: TeamSummary[]; truncated: boolean; next: string | null }
export interface TeamMember { id: string; role: string; agent: string; account: string; state: string; lead: boolean; session: TeamSession | null }
export interface TeamMemberCounts { total: number; not_created: number; active: number; paused: number; removed: number }
export interface TeamNodeCounts { waiting: number; pending: number; in_progress: number; reported: number; accepted: number; cancelled: number }
export interface TeamCounts { members: TeamMemberCounts; nodes: TeamNodeCounts; inbox: number; cards: number }
export interface TeamStage { node: number; index: number; role: string; state: string; assignee: string; implementer: string }
export interface TeamTaskRef { project: string; uuid: string }
export interface TeamCard {
  node: number; title: string; state: string; task: TeamTaskRef | null; role: string; pipeline: string;
  assignee: string; implementer: string; rank: string; round: number; held: boolean; admitted: boolean;
  integrated: boolean; decided_by: string; stages: TeamStage[];
}
export interface TeamGroup { state: string; cards: TeamCard[] }
export interface TeamBar {
  id: string; project: string; name: string; status: string; goal: string; lead: string;
  integration_branch: string; base_commit: string; revision: number; counts: TeamCounts;
}
export interface TeamView { bar: TeamBar; members: TeamMember[]; groups: TeamGroup[]; truncated: boolean; next: string | null; revision: number }

export interface TeamConfirmation { name: string; title: string; text: string; action: string }
export interface TeamCall {
  team: string;
  team_name?: string;
  action: { kind: string } & Record<string, unknown>;
  expected_revision?: number;
  confirmed?: boolean;
  request_id: string;
}
export type TeamOutcome =
  | { outcome: "done"; receipt: unknown; request_id: string }
  | { outcome: "failed"; detail: string; code?: string; request_id: string }
  | { outcome: "uncertain"; detail: string; request_id: string }
  | { outcome: "confirm"; confirmation: TeamConfirmation }
  | { outcome: "pending"; request_id: string };

export interface TeamFollow { list: boolean; waiting: boolean; team: string | null }
export interface TeamCatalogRole { id: string; name: string; agent: string; account: { mode: string; account?: string }; model: string; effort: string; pipeline: string; context: string }
export interface TeamCatalogStage { kind: string; role: string }
export interface TeamCatalogPipeline { id: string; name: string; implementer: string; stages: TeamCatalogStage[]; max_rounds: number; accept: string }
export interface TeamCatalogSeat { role: string; count: number; optional: boolean }
export interface TeamCatalogSettings { queue: boolean; routing: string; plan_approval: string; phase_acceptance: string; integration: string; land_allowed: boolean }
export interface TeamCatalogLead { agent: string; account: { mode: string; account?: string }; model: string; effort: string; rules: string }
export interface TeamCatalogTemplate {
  id: string; name: string; description: string; lead: TeamCatalogLead;
  roster: TeamCatalogSeat[]; default_pipeline: string; settings: TeamCatalogSettings;
}
export interface TeamCatalog { roles: TeamCatalogRole[]; pipelines: TeamCatalogPipeline[]; templates: TeamCatalogTemplate[]; planning_rules: string[] }
export interface TeamProjectFacts { git: boolean; head: string; branch: string; reason: string }

/** The data the views need from the platform; a test passes its own. */
export interface TeamsTransport {
  follow(follow: TeamFollow): Promise<void>;
  hidden(hidden: boolean): Promise<void>;
  tasksChanged(): Promise<void>;
  page(key: string, cursor: string | null): Promise<void>;
  act(call: TeamCall): Promise<TeamOutcome>;
  actAgain(call: TeamCall): Promise<TeamOutcome>;
  dismiss(call: TeamCall): Promise<void>;
  catalog(): Promise<unknown>;
  saveTemplate(template: TeamCatalogTemplate): Promise<void>;
  project(path: string): Promise<unknown>;
}

/** The failure an action call itself can raise before any outcome: a refusal to run it. */
const errorText = (error: unknown) => (typeof error === "string" ? error : error instanceof Error ? error.message : String(error));

/** Package 12's Tauri commands; nothing else touches `hgs`. */
export function tauriTeams(): TeamsTransport {
  const live = () => {
    if (!inTauri()) throw new Error("Teams need a live hgs event stream.");
  };
  return {
    async follow(follow) { live(); await invoke("teams_follow", { list: follow.list, waiting: follow.waiting, team: follow.team }); },
    async hidden(hidden) { live(); await invoke("teams_hidden", { hidden }); },
    async tasksChanged() { live(); await invoke("teams_tasks_changed"); },
    async page(key, cursor) { live(); await invoke("teams_page", { key, cursor }); },
    async act(call) { live(); return invoke<TeamOutcome>("team_action", { call }); },
    async actAgain(call) { live(); return invoke<TeamOutcome>("team_action_again", { call }); },
    async dismiss(call) { live(); await invoke("team_intent_dismiss", { call }); },
    async catalog() { live(); return invoke<unknown>("teams_catalog"); },
    async saveTemplate(template) { live(); await invoke("teams_template_save", { template }); },
    async project(path) { live(); return invoke<unknown>("teams_project", { path }); },
  };
}

/** A request identity for one intent, made once before the first send. */
export function newRequestId(): string {
  const crypto = globalThis.crypto;
  if (crypto?.randomUUID) return crypto.randomUUID();
  const bytes = new Uint8Array(16);
  crypto?.getRandomValues?.(bytes);
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-8${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
}

/** The pinned team of a project: what the Team tab shows. */
export interface SessionIdentity { sessionId?: string | null; runId?: string | null }

export function createTeams(transport: TeamsTransport = tauriTeams()) {
  const [list, setList] = createSignal<TeamList>();
  const [team, setTeam] = createSignal<TeamView>();
  const [failure, setFailure] = createSignal<{ key: string; code: string; message: string } | null>(null);
  /** undefined until the first list answer; false when Teams are off or hgs cannot run. */
  const [available, setAvailable] = createSignal<boolean>();
  const [off, setOff] = createSignal<"inactive" | "unavailable" | null>(null);
  const [followed, setFollowed] = createSignal<string>();
  const [visible, setVisible] = createSignal(true);
  const [catalog, setCatalog] = createSignal<TeamCatalog>();
  const [catalogError, setCatalogError] = createSignal("");
  const [newTeamOpen, setNewTeamOpen] = createSignal(false);
  const [confirmation, setConfirmation] = createSignal<{ call: TeamCall; confirmation: TeamConfirmation } | null>(null);
  const [uncertain, setUncertain] = createSignal<{ call: TeamCall; detail: string } | null>(null);
  const [busy, setBusy] = createSignal(false);
  /** Session key → its team, null when the session is not a lead. */
  const [sessionTeams, setSessionTeams] = createSignal<Record<string, string | null>>({});
  /** Waiting resolvers by team id, for the session-to-team lookup. */
  const waiting = new Map<string, (view: TeamView | null) => void>();
  let probing = false, probingDone = false;
  const known = (key: string) => key in sessionTeams();

  function apply(entries: TeamsOut[]) {
    for (const entry of entries.slice(0, 8)) {
      if (entry.key === "list") {
        if (entry.kind === "list") {
          const view = entry.payload as TeamList;
          batch(() => { setList(view); setAvailable(true); setOff(null); });
        } else if (entry.kind === "failed") {
          const payload = entry.payload as { code: string; message: string };
          batch(() => {
            if (payload.code === "teams_inactive") { setAvailable(false); setOff("inactive"); }
            else if (payload.code === "unavailable") { setAvailable(false); setOff("unavailable"); }
            else setAvailable(true);
            setFailure({ key: entry.key, code: payload.code, message: payload.message });
          });
        }
        continue;
      }
      if (entry.key.startsWith("team:")) {
        if (entry.kind === "team") {
          const view = entry.payload as TeamView;
          if (view.bar.id === followed() || !followed()) batch(() => { setTeam(view); setAvailable(true); });
          const resolve = waiting.get(view.bar.id);
          if (resolve) { waiting.delete(view.bar.id); resolve(view); }
        } else if (entry.kind === "failed") {
          setFailure({ key: entry.key, code: (entry.payload as { code: string }).code, message: (entry.payload as { message: string }).message });
        } else if (entry.kind === "ended") {
          const shown = team();
          if (!shown || entry.key === `team:${shown.bar.id}` || entry.key === `team:${followed()}`) setTeam(undefined);
        }
      }
    }
  }

  /** One read tells whether Teams exist on this computer; it also fills the list. */
  async function probe() {
    if (probing || probingDone) return;
    probing = true; probingDone = true;
    try { await transport.follow({ list: true, waiting: false, team: null }); }
    catch { batch(() => { setAvailable(false); setOff("unavailable"); }); }
    finally { probing = false; }
  }

  /** Follows one team's views (and the list); the tab's mount and the page's effect call it. */
  async function openTeam(id: string) {
    setFollowed(id);
    await transport.follow({ list: false, waiting: false, team: id });
  }
  async function closeTeam() {
    setFollowed(undefined);
    setTeam(undefined);
    await transport.follow({ list: true, waiting: false, team: null });
  }
  async function hidden(hidden: boolean) {
    setVisible(!hidden);
    await transport.hidden(hidden);
    if (!hidden) return;
    // A hidden tab keeps its team followed but stops reading (the core stops).
  }
  async function tasksChanged() { if (followed()) await transport.tasksChanged(); }
  async function page(key: string, cursor: string | null) { await transport.page(key, cursor); }

  /** The session's team: the list's `lead_session` names the lead's session. */
  async function leadTeamFor(key: string, identity: SessionIdentity): Promise<string | null> {
    if (known(key)) return sessionTeams()[key];
    const ids = [identity.sessionId ? `launch_id:${identity.sessionId}` : "", identity.runId ? `run_id:${identity.runId}` : ""].filter(Boolean);
    const found = (list()?.teams ?? []).find((summary) => !!summary.lead_session && ids.includes(summary.lead_session.session_id));
    setSessionTeams({ ...sessionTeams(), [key]: found?.id ?? null });
    if (found) { setFollowed(found.id); await transport.follow({ list: false, waiting: false, team: found.id }); }
    return found?.id ?? null;
  }

  /** Opens the New team dialog: its catalog read is lazy, as the dialog is. */
  async function openNewTeam() {
    setNewTeamOpen(true);
    if (catalog()) return;
    try {
      const reply = (await transport.catalog()) as { catalog?: TeamCatalog } | TeamCatalog;
      setCatalog(reply && "catalog" in reply && reply.catalog ? reply.catalog : (reply as TeamCatalog));
      setCatalogError("");
    } catch (error) { setCatalogError(errorText(error)); }
  }
  function closeNewTeam() { setNewTeamOpen(false); }

  async function saveTemplate(template: TeamCatalogTemplate) { await transport.saveTemplate(template); setCatalog(catalog() ? { ...catalog()!, templates: [...catalog()!.templates.filter((t) => t.id !== template.id), template] } : catalog()); }

  async function project(path: string): Promise<TeamProjectFacts> {
    const reply = (await transport.project(path)) as { git?: boolean; head?: string; branch?: string; state?: string };
    const git = reply.git ?? reply.state === "ok";
    return { git, head: reply.head ?? "", branch: reply.branch ?? "", reason: git ? "" : "not a Git repository" };
  }

  /** Sends one intent: the confirmation is asked by the core, then re-sent confirmed. */
  async function run(call: TeamCall): Promise<TeamOutcome> {
    setBusy(true);
    try {
      const outcome = await transport.act(call);
      accept(call, outcome);
      return outcome;
    } catch (error) {
      return { outcome: "failed", detail: errorText(error), request_id: call.request_id };
    } finally { setBusy(false); }
  }
  async function runAgain(call: TeamCall): Promise<TeamOutcome> {
    setBusy(true);
    try { const outcome = await transport.actAgain(call); accept(call, outcome); return outcome; }
    catch (error) { return { outcome: "failed", detail: errorText(error), request_id: call.request_id }; }
    finally { setBusy(false); }
  }
  function accept(call: TeamCall, outcome: TeamOutcome) {
    if (outcome.outcome === "confirm") setConfirmation({ call: { ...call, confirmed: false }, confirmation: outcome.confirmation });
    else if (outcome.outcome === "uncertain") setUncertain({ call, detail: outcome.detail });
    else if (outcome.outcome === "done" || outcome.outcome === "failed") {
      if (uncertain()?.call.request_id === call.request_id) setUncertain(null);
    }
  }
  /** The person confirmed a destructive action: the same intent, marked confirmed. */
  async function confirm() {
    const asking = confirmation();
    if (!asking) return;
    setConfirmation(null);
    await run({ ...asking.call, confirmed: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  function cancelConfirmation() { setConfirmation(null); }
  async function check() {
    const asking = uncertain();
    if (!asking) return;
    await runAgain(asking.call);
  }
  async function dismiss() {
    const asking = uncertain();
    if (!asking) return;
    try { await transport.dismiss(asking.call); setUncertain(null); } catch { /* the intent stays for a retry */ }
  }
  function clearFailure() { setFailure(null); }

  return {
    list, team, failure, available, off, followed, visible, catalog, catalogError, newTeamOpen, confirmation, uncertain, busy, sessionTeams,
    apply, probe, openTeam, closeTeam, hidden, tasksChanged, page, leadTeamFor, known,
    openNewTeam, closeNewTeam, saveTemplate, project, run, runAgain, confirm, cancelConfirmation, check, dismiss, clearFailure,
  };
}

/** The application's Teams store. */
export const teams = createRoot(() => createTeams());
