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

/** The team's own operations: pause and resume by its status, stop, archive. */
export function barActions(view: TeamView): BarAction[] {
  const status = view.bar.status;
  const actions: BarAction[] = [];
  if (status === "running") actions.push({ id: "pause", text: tr("Pause") });
  if (status === "paused") actions.push({ id: "resume", text: tr("Resume") });
  if (status === "running" || status === "planning" || status === "paused") actions.push({ id: "stop", text: tr("Stop"), danger: true });
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

/** A card's decisions, by its state. */
export function taskButtons(card: TaskView): TaskButton[] {
  const state = card.state;
  const buttons: TaskButton[] = [];
  if (state === "waiting") buttons.push({ id: "admit", text: tr("Admit") });
  if (state === "reported") buttons.push({ id: "accept", text: tr("Accept") }, { id: "return", text: tr("Return"), reason: true });
  if (state === "in_progress") buttons.push({ id: "return", text: tr("Return"), reason: true });
  if (state === "pending" || state === "in_progress") {
    buttons.push(card.held ? { id: "release", text: tr("Release") } : { id: "hold", text: tr("Hold") });
  }
  if (state !== "accepted" && state !== "cancelled") buttons.push({ id: "cancel", text: tr("Cancel task"), danger: true, reason: true });
  return buttons;
}

/** A member's operations, by its state. */
export function memberActions(member: MemberView): { id: string; text: string; danger?: boolean; disabled?: boolean; reason?: string }[] {
  const active = member.state === "active";
  const paused = member.state === "paused";
  return [
    ...(active ? [{ id: "pause-member", text: tr("Pause") }] : []),
    ...(paused ? [{ id: "resume-member", text: tr("Resume") }] : []),
    ...(active || paused ? [{ id: "stop-member", text: tr("Stop"), danger: true }] : []),
    ...(active || paused ? [{ id: "archive-member", text: tr("Remove"), danger: true, disabled: member.lead, reason: member.lead ? tr("The team's lead cannot be removed.") : undefined }] : []),
  ];
}

export function stageName(kind: string): string {
  return tr(kind === "test" ? "test" : "review");
}
