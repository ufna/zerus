import { draftKey } from "../../store/draftKeys";
import { tr } from "../../i18n";
import type { TeamView } from "./types";
import type { Session } from "../sessions/types";

type LeadRow = Pick<Session, "computer" | "catalogKey" | "name" | "sessionId" | "state" | "runId" | "launchId" | "conversationId">;

/** A name or an old composer never establishes the lead. The composer itself
 * keeps the inspected run/conversation and original delivery request UUID. */
export function leadChatBound(view: TeamView, row: LeadRow | undefined, machine: string | null | undefined, composerKey: string | undefined, child: boolean): boolean {
  return leadChatCause(view, row, machine, composerKey, child) === undefined;
}

/** Why the lead chat cannot send, and what to do, in the order the binding
 * is checked (zerus-63rru.199 C4); undefined when it can. */
export function leadChatCause(view: TeamView, row: LeadRow | undefined, machine: string | null | undefined, composerKey: string | undefined, child: boolean): string | undefined {
  if (!row) return tr("Open the lead's session first.");
  if (child) return tr("Close the open subagent to talk to the lead.");
  if (row.state === "archived") return tr("The lead's session is archived.");
  if (!row.runId || !row.conversationId) return tr("The lead's session is not running. Start it to talk to the lead.");
  const lead = view.members.find((member) => member.id === view.bar.lead && member.lead);
  const session = lead?.session;
  if (!session) return tr("The team's lead has no session yet.");
  if (!machine || session.machine !== machine) return tr("The lead runs on another computer.");
  if ((!row.launchId || session.session_id !== `launch_id:${row.launchId}`) && session.session_id !== `run_id:${row.runId}`) return tr("The selected session is no longer the team's lead.");
  if (composerKey !== draftKey({ computer: row.computer, key: row.catalogKey, name: row.name, archive_id: null, session_id: row.sessionId })) return tr("The lead's conversation is still loading.");
  return undefined;
}
