import { draftKey } from "../../store/draftKeys";
import type { TeamView } from "./types";
import type { Session } from "../sessions/types";

type LeadRow = Pick<Session, "computer" | "catalogKey" | "name" | "sessionId" | "state" | "runId" | "launchId" | "conversationId">;

/** Why Lead chat cannot send to the lead here (audit C4): each refusal of
 * the binding below, so the drawer can name its cause and the way out. */
export type LeadChatProblem = "no_row" | "subagent" | "archived" | "no_conversation" | "no_lead"
  | "unknown_computer" | "other_computer" | "other_session" | "composer";

export function leadChatProblem(view: TeamView, row: LeadRow | undefined, machine: string | null | undefined, composerKey: string | undefined, child: boolean): LeadChatProblem | undefined {
  if (!row) return "no_row";
  if (child) return "subagent";
  if (row.state === "archived") return "archived";
  if (!row.runId || !row.conversationId) return "no_conversation";
  const lead = view.members.find((member) => member.id === view.bar.lead && member.lead);
  const session = lead?.session;
  if (!session) return "no_lead";
  if (!machine) return "unknown_computer";
  if (session.machine !== machine) return "other_computer";
  if ((!row.launchId || session.session_id !== `launch_id:${row.launchId}`) && session.session_id !== `run_id:${row.runId}`) return "other_session";
  if (composerKey !== draftKey({ computer: row.computer, key: row.catalogKey, name: row.name, archive_id: null, session_id: row.sessionId })) return "composer";
  return undefined;
}

/** A name or an old composer never establishes the lead. The composer itself
 * keeps the inspected run/conversation and original delivery request UUID. */
export function leadChatBound(view: TeamView, row: LeadRow | undefined, machine: string | null | undefined, composerKey: string | undefined, child: boolean): boolean {
  return leadChatProblem(view, row, machine, composerKey, child) === undefined;
}
