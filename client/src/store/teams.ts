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
  /** The lead's bound session: how a session finds its team (package 5's list). */
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

/** One open owner card that waits for the person. */
export interface WaitingItem {
  id: string;
  team: string;
  card: number;
  kind: string;
  node: number | null;
  text: string;
  state: string;
  by: string;
  created_ms: number;
}

/** The open owner cards of every team, oldest first, one bounded page. */
export interface WaitingView { items: WaitingItem[]; next: string | null }

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

/** The session asking for its team, by this machine and its launch or run.
 * The row's `sessionId` is not this: a team binds the lead's launch id. */
export interface SessionIdentity { machine?: string | null; launchId?: string | null; runId?: string | null }

/** What the current list says about a session's team; an incomplete list says
 * `unknown`, never `none`. */
export interface TeamOfSession { state: "lead" | "none" | "unknown"; team?: string }

/** One action whose result the desktop does not know: it leaves only by Check
 * with its own request id or by Dismiss. */
export interface UnresolvedCall { call: TeamCall; detail: string; state: "uncertain" | "pending" }

/** A failed outcome's reason, for the view that asked for the action.
 *
 * A view names itself when it sends: `teams.run(call, "new-team")` (any stable
 * string; "" is the store's own). It reads `teams.failureFor("new-team")` in a
 * memo or an effect — the `failure` signal makes it reactive — and clears it
 * with `teams.clearFailure("new-team")` when the person has seen it or the view
 * changes. `teams.failure()` remains the newest reason of any view. */
export interface TeamFailure { key: string; code: string; message: string; source: string }

/** Unresolved calls kept before a new action is refused. */
export const UNRESOLVED_MAX = 8;
/** List pages followed to find a lead's session; an incomplete list says unknown. */
export const LIST_PAGES_MAX = 5;
/** A window hidden at least this long re-asks whether Teams exist when it returns. */
export const HIDDEN_RECHECK_MS = 10 * 60 * 1000;

export function createTeams(transport: TeamsTransport = tauriTeams()) {
  /** The latest list page, for a view that shows the list. */
  const [list, setList] = createSignal<TeamList>();
  /** Every page followed since the list was last asked for, for lead lookups. */
  const [listPages, setListPages] = createSignal<TeamSummary[]>([]);
  /** The last page's cursor; null once a page has no next. */
  const [listNext, setListNext] = createSignal<string | null>(null);
  const [waitingView, setWaitingView] = createSignal<WaitingView>();
  const [team, setTeam] = createSignal<TeamView>();
  const [failure, setFailure] = createSignal<TeamFailure | null>(null);
  /** undefined until the first list answer; false when Teams are off or hgs cannot run. */
  const [available, setAvailable] = createSignal<boolean>();
  const [off, setOff] = createSignal<"inactive" | "unavailable" | null>(null);
  const [followed, setFollowed] = createSignal<string>();
  const [visible, setVisible] = createSignal(true);
  const [catalog, setCatalog] = createSignal<TeamCatalog>();
  const [catalogError, setCatalogError] = createSignal("");
  const [newTeamOpen, setNewTeamOpen] = createSignal(false);
  const [confirmation, setConfirmation] = createSignal<{ call: TeamCall; confirmation: TeamConfirmation } | null>(null);
  /** Every call whose result is unknown, bounded and never evicted by another. */
  const [unresolved, setUnresolved] = createSignal<UnresolvedCall[]>([]);
  const [busy, setBusy] = createSignal(false);
  /** Resolvers by team id, for a team view a decision needs a revision from. */
  const pendingViews = new Map<string, (view: TeamView | null) => void>();
  /** List page cursors already requested, so one page is never asked twice. */
  const requestedPages = new Set<string>();
  let probing = false;
  let hiddenAt: number | undefined;

  const unresolvedOf = (requestId: string) => unresolved().find((entry) => entry.call.request_id === requestId);
  function putUnresolved(call: TeamCall, detail: string, state: UnresolvedCall["state"]) {
    setUnresolved([...unresolved().filter((entry) => entry.call.request_id !== call.request_id), { call, detail, state }]);
  }
  function settleUnresolved(requestId: string) {
    setUnresolved(unresolved().filter((entry) => entry.call.request_id !== requestId));
  }

  /** A positive off answer hides the entry points, unfollows and stops the reads. */
  function stopFor(code: string) {
    batch(() => { setAvailable(false); setOff(code === "unavailable" ? "unavailable" : "inactive"); setList(undefined); setTeam(undefined); setWaitingView(undefined); });
    void transport.follow({ list: false, waiting: false, team: null }).catch(() => {});
    void transport.hidden(true).catch(() => {});
  }

  function apply(entries: TeamsOut[]) {
    for (const entry of entries.slice(0, 8)) {
      if (entry.key === "waiting") {
        if (entry.kind === "waiting") {
          setWaitingView(entry.payload as WaitingView);
        } else if (entry.kind === "failed") {
          const payload = entry.payload as { code: string; message: string };
          setFailure({ key: "waiting", code: payload.code, message: payload.message, source: "" });
          if (payload.code === "teams_inactive" || payload.code === "unavailable") stopFor(payload.code);
        }
        continue;
      }
      if (entry.key === "list") {
        if (entry.kind === "list") {
          const view = entry.payload as TeamList;
          setList(view);
          setListNext(view.next);
          // Pages are kept for the lead lookup; the same team never counts twice.
          const known = new Set(listPages().map((summary) => summary.id));
          setListPages([...listPages(), ...view.teams.filter((summary) => !known.has(summary.id))]);
          batch(() => { setAvailable(true); setOff(null); });
        } else if (entry.kind === "failed") {
          const payload = entry.payload as { code: string; message: string };
          setFailure({ key: entry.key, code: payload.code, message: payload.message, source: "" });
          if (payload.code === "teams_inactive" || payload.code === "unavailable") { stopFor(payload.code); continue; }
          setAvailable(true);
        }
        continue;
      }
      if (entry.key.startsWith("team:")) {
        if (entry.kind === "team") {
          const view = entry.payload as TeamView;
          const resolve = pendingViews.get(view.bar.id);
          // A view is shown only for the followed team, or to the one decision
          // that asked for its revision; a late view after unfollow is dropped.
          if (view.bar.id === followed() || resolve) batch(() => { setTeam(view); setAvailable(true); });
          if (resolve) { pendingViews.delete(view.bar.id); resolve(view); }
        } else if (entry.kind === "failed") {
          const payload = entry.payload as { code: string; message: string };
          setFailure({ key: entry.key, code: payload.code, message: payload.message, source: "" });
          if (payload.code === "teams_inactive" || payload.code === "unavailable") stopFor(payload.code);
        } else if (entry.kind === "ended") {
          const shown = team();
          if (!shown || entry.key === `team:${shown.bar.id}` || entry.key === `team:${followed()}`) setTeam(undefined);
        }
      }
    }
  }

  /** Asks whether Teams exist and follows the list and the waiting read. */
  async function recheck() {
    if (probing) return;
    probing = true;
    batch(() => { setListPages([]); setListNext(null); setList(undefined); });
    requestedPages.clear();
    try {
      await transport.hidden(false);
      await transport.follow({ list: true, waiting: true, team: null });
    } catch {
      batch(() => { setAvailable(false); setOff("unavailable"); });
    } finally { probing = false; }
  }

  /** The application asks once at start (App's mount); it never polls by itself. */
  async function probe() { await recheck(); }

  /** Window visibility belongs to the application, not to a mounted tab. A
   * platform that cannot take the call leaves the availability answer to say
   * whether Teams exist, and never raises from a visibility change. */
  async function hidden(hiddenNow: boolean) {
    setVisible(!hiddenNow);
    try {
      if (hiddenNow) { hiddenAt = Date.now(); await transport.hidden(true); return; }
      const was = hiddenAt;
      hiddenAt = undefined;
      await transport.hidden(false);
      if (was !== undefined && Date.now() - was >= HIDDEN_RECHECK_MS) await recheck();
    } catch { /* a synthetic or failed helper: the reads stay as they are */ }
  }

  async function openTeam(id: string) {
    setFollowed(id);
    await transport.follow({ list: false, waiting: true, team: id });
  }
  async function closeTeam() {
    setFollowed(undefined);
    setTeam(undefined);
    await transport.follow({ list: true, waiting: true, team: null });
  }
  async function tasksChanged() { if (followed()) await transport.tasksChanged(); }
  async function page(key: string, cursor: string | null) { await transport.page(key, cursor); }
  async function openWaiting() { await transport.follow({ list: true, waiting: true, team: null }); }
  async function pageWaiting(cursor: string | null) { await transport.page("waiting", cursor); }
  const waitingCount = () => {
    const view = waitingView();
    return !view ? 0 : view.items.length + (view.next ? 1 : 0);
  };
  async function revisionOf(id: string): Promise<number | undefined> {
    const shown = team();
    if (shown?.bar.id === id) return shown.revision;
    await transport.follow({ list: false, waiting: true, team: id });
    return await new Promise<number | undefined>((resolve) => {
      pendingViews.set(id, (view) => resolve(view?.revision));
      setTimeout(() => { if (pendingViews.delete(id)) resolve(undefined); }, 5000);
    });
  }
  async function decide(card: WaitingItem, decision: string): Promise<TeamOutcome | undefined> {
    const expected_revision = await revisionOf(card.team);
    if (expected_revision === undefined) return undefined;
    return run({ team: card.team, expected_revision, request_id: newRequestId(), action: { kind: "decide", card: card.card, decision } });
  }

  /**
   * Whether the selected session leads a team, from the followed list and
   * nothing else: no cache, no prefix guessing, and no negative answer from an
   * incomplete list. Pages are asked for one at a time up to the cap.
   */
  function teamOfSession(identity: SessionIdentity): TeamOfSession {
    const ids = [identity.launchId ? `launch_id:${identity.launchId}` : "", identity.runId ? `run_id:${identity.runId}` : ""].filter(Boolean);
    if (!ids.length) return { state: "none" };
    const machine = identity.machine ?? "";
    const found = listPages().find((summary) => {
      const lead = summary.lead_session;
      if (!lead || !ids.includes(lead.session_id)) return false;
      return !machine || !lead.machine || lead.machine === machine;
    });
    if (found) return { state: "lead", team: found.id };
    if (listPages().length === 0) return { state: "unknown" };
    const cursor = listNext();
    if (cursor && listPages().length < LIST_PAGES_MAX) {
      if (!requestedPages.has(cursor)) { requestedPages.add(cursor); void transport.page("list", cursor); }
      return { state: "unknown" };
    }
    // A list with no next page is complete: no match is a real "no team".
    return { state: "none" };
  }

  /**
   * The old per-session accessor, kept only until the Team tab's design branch
   * is wired to `teamOfSession`: it remembers nothing and answers nothing, so
   * the tab shows its "finding" state rather than a stale team.
   */
  function sessionTeams(): Record<string, string | null> { return {}; }

  /** The Team tab's compatibility seam: the lead's team, followed when found. */
  async function leadTeamFor(_key: string, identity: { launchId?: string | null; sessionId?: string | null; runId?: string | null }): Promise<string | null> {
    // The old Team tab passed its row's `sessionId`; the launch id is the team's
    // binding, so prefer it and keep the old field as a fallback only.
    const found = teamOfSession({ launchId: identity.launchId ?? identity.sessionId, runId: identity.runId });
    if (found.state !== "lead" || !found.team) return null;
    if (followed() !== found.team) {
      setFollowed(found.team);
      await transport.follow({ list: false, waiting: true, team: found.team });
    }
    return found.team;
  }

  async function openNewTeam() {
    setNewTeamOpen(true);
    if (catalog()) return;
    try {
      const reply = (await transport.catalog()) as { catalog?: TeamCatalog } | TeamCatalog;
      const candidate = reply && "catalog" in reply && reply.catalog ? reply.catalog : (reply as TeamCatalog);
      if (!candidate || !Array.isArray(candidate.templates) || !Array.isArray(candidate.roles) || !Array.isArray(candidate.pipelines)) {
        throw new Error("The catalog answer has no templates. Nothing was created.");
      }
      setCatalog(candidate);
      setCatalogError("");
    } catch (error) { setCatalogError(errorText(error)); }
  }
  function closeNewTeam() { setNewTeamOpen(false); }

  async function saveTemplate(template: TeamCatalogTemplate) {
    await transport.saveTemplate(template);
    setCatalog(catalog() ? { ...catalog()!, templates: [...catalog()!.templates.filter((t) => t.id !== template.id), template] } : catalog());
  }

  async function project(path: string): Promise<TeamProjectFacts> {
    const reply = (await transport.project(path)) as { git?: boolean; head?: string; branch?: string; state?: string };
    const git = reply.git ?? reply.state === "ok";
    return { git, head: reply.head ?? "", branch: reply.branch ?? "", reason: git ? "" : "not a Git repository" };
  }

  /** Sends one intent; `source` names the view that asked for its reason. */
  async function run(call: TeamCall, source = ""): Promise<TeamOutcome> {
    if (unresolved().length >= UNRESOLVED_MAX) {
      const detail = "There are too many unresolved actions. Check or dismiss them first.";
      setFailure({ key: "action", code: "unresolved_full", message: detail, source });
      return { outcome: "failed", detail, code: "unresolved_full", request_id: call.request_id };
    }
    setBusy(true);
    try {
      const outcome = await transport.act(call);
      accept(call, outcome, source);
      return outcome;
    } catch (error) {
      const detail = errorText(error);
      putUnresolved(call, detail, "uncertain");
      return { outcome: "uncertain", detail, request_id: call.request_id };
    } finally { setBusy(false); }
  }
  async function runAgain(call: TeamCall, source = ""): Promise<TeamOutcome> {
    setBusy(true);
    try { const outcome = await transport.actAgain(call); accept(call, outcome, source); return outcome; }
    catch (error) { const detail = errorText(error); putUnresolved(call, detail, "uncertain"); return { outcome: "uncertain", detail, request_id: call.request_id }; }
    finally { setBusy(false); }
  }
  /** The newest failure of one view, or of any view when no source is given. */
  const failureFor = (source: string) => {
    const found = failure();
    return found && (source === "" || found.source === source) ? found : null;
  };
  function accept(call: TeamCall, outcome: TeamOutcome, source: string) {
    if (outcome.outcome === "confirm") setConfirmation({ call: { ...call, confirmed: false }, confirmation: outcome.confirmation });
    else if (outcome.outcome === "uncertain") putUnresolved(call, outcome.detail, "uncertain");
    else if (outcome.outcome === "pending") putUnresolved(call, "The action waits for the team.", "pending");
    else if (outcome.outcome === "done") settleUnresolved(call.request_id);
    else if (outcome.outcome === "failed") {
      settleUnresolved(call.request_id);
      setFailure({ key: "action", code: outcome.code ?? "", message: outcome.detail, source });
    }
  }
  async function confirm(source = "") {
    const asking = confirmation();
    if (!asking) return;
    setConfirmation(null);
    await run({ ...asking.call, confirmed: true }, source);
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  function cancelConfirmation() { setConfirmation(null); }
  async function check(entry?: UnresolvedCall, source = "") {
    const asking = entry ?? unresolved()[0];
    if (!asking) return;
    await runAgain(asking.call, source);
  }
  async function dismiss(entry?: UnresolvedCall, source = "") {
    const asking = entry ?? unresolved()[0];
    if (!asking) return;
    try {
      await transport.dismiss(asking.call);
      settleUnresolved(asking.call.request_id);
    } catch (error) {
      setFailure({ key: "action", code: "dismiss_failed", message: errorText(error), source });
    }
  }
  /** The first unresolved call, for a view that shows one at a time. */
  const uncertain = () => {
    const first = unresolved()[0];
    return first ? { call: first.call, detail: first.detail } : null;
  };
  /** Clears one view's reason, or every reason when no source is given. */
  function clearFailure(source?: string) {
    if (source === undefined) { setFailure(null); return; }
    if (failure()?.source === source) setFailure(null);
  }

  return {
    list, listPages, listNext, team, failure, available, off, followed, visible, catalog, catalogError, newTeamOpen, confirmation, unresolved, uncertain, busy,
    waitingView, waitingCount, apply, probe, recheck, hidden, openTeam, closeTeam, tasksChanged, page, openWaiting, pageWaiting, decide, revisionOf,
    teamOfSession, sessionTeams, leadTeamFor, unresolvedOf, failureFor, openNewTeam, closeNewTeam, saveTemplate, project, run, runAgain, confirm, cancelConfirmation, check, dismiss, clearFailure,
  };
}

/** The application's Teams store. */
export const teams = createRoot(() => createTeams());
