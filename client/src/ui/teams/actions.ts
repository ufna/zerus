import { tr } from "../../i18n";
import type { TeamCall } from "../../store/teams";
import type { MemberView, TaskView, TeamView } from "./types";

// The Team tab's action vocabulary: the calls the views can send and the
// words of their buttons. The outcome is never decided here: the store sends
// one intent and the core answers done, failed, uncertain, confirm or pending.

/** One intent's call: the person's request id, the team and its revision. */
export function teamCall(view: TeamView, requestId: string, kind: string, fields: Record<string, unknown> = {}): TeamCall {
  return { team: view.bar.id, team_name: view.bar.name, expected_revision: view.revision, request_id: requestId, action: { kind, ...fields } };
}

export interface BarAction {
  id: "pause" | "resume" | "stop" | "archive";
  text: string;
  danger?: boolean;
  disabled?: boolean;
}

// Every action is offered only in the states where the store accepts it; the
// rules are the store's operations (src/teams/ops on integrate/teams):
// team.rs pause, resume, stop and archive; queue.rs admit, hold and release;
// decisions.rs accept, return and cancel; members.rs pause, resume, stop and
// archive. Task and member actions need an active team (common.rs active: a
// running or paused team) and a live task (live_node: not accepted or
// cancelled). The store's refusal still decides what the rows cannot show: an
// unsettled decision, a person's earlier stop of a member, the queue's
// capacity.

/** A team the store takes task and member actions on: running or paused. */
export const activeTeam = (status: string) => status === "running" || status === "paused";
/** A task the store still decides: not accepted and not cancelled. */
export const liveTask = (state: string) => state !== "accepted" && state !== "cancelled";

/** The team's own operations: pause a running team, resume a paused one,
 * stop an active one, archive a stopped or finished one. */
export function barActions(view: TeamView): BarAction[] {
  const status = view.bar.status;
  const actions: BarAction[] = [];
  if (status === "running") actions.push({ id: "pause", text: tr("Pause") });
  if (status === "paused") actions.push({ id: "resume", text: tr("Resume") });
  if (activeTeam(status)) actions.push({ id: "stop", text: tr("Stop"), danger: true });
  if (status === "stopped" || status === "finished") actions.push({ id: "archive", text: tr("Archive"), danger: true });
  return actions;
}

export interface TaskButton {
  id: "admit" | "accept" | "return" | "hold" | "release" | "cancel";
  text: string;
  danger?: boolean;
  /** The button asks for a reason before it sends. */
  reason?: boolean;
}

/** A card's decisions, by the team's status and the card's state: admit a
 * task not admitted yet; accept a reported one; return a reported one that
 * has a holder (the row's implementer, who reported it); hold one not held
 * and release one held; cancel any live one. */
export function taskButtons(card: TaskView, teamStatus: string): TaskButton[] {
  if (!activeTeam(teamStatus) || !liveTask(card.state)) return [];
  const buttons: TaskButton[] = [];
  if (!card.admitted) buttons.push({ id: "admit", text: tr("Admit") });
  if (card.state === "reported") buttons.push({ id: "accept", text: tr("Accept") });
  if (card.state === "reported" && card.implementer) buttons.push({ id: "return", text: tr("Return"), reason: true });
  buttons.push(card.held ? { id: "release", text: tr("Release") } : { id: "hold", text: tr("Hold") });
  buttons.push({ id: "cancel", text: tr("Cancel task"), danger: true, reason: true });
  return buttons;
}

/** A member's operations, by the team's status and the member's state: pause
 * an active member; resume a paused one that has a session; stop an active or
 * paused one; remove any member that is not removed and is not the lead. */
export function memberActions(member: MemberView, teamStatus: string): { id: string; text: string; danger?: boolean; disabled?: boolean; reason?: string }[] {
  if (!activeTeam(teamStatus)) return [];
  const active = member.state === "active";
  const paused = member.state === "paused";
  return [
    ...(active ? [{ id: "pause-member", text: tr("Pause") }] : []),
    ...(paused && member.session ? [{ id: "resume-member", text: tr("Resume") }] : []),
    ...(active || paused ? [{ id: "stop-member", text: tr("Stop"), danger: true }] : []),
    ...(member.state !== "removed" && !member.lead ? [{ id: "archive-member", text: tr("Remove"), danger: true }] : []),
  ];
}

export function stageName(kind: string): string {
  return tr(kind === "test" ? "test" : "review");
}
