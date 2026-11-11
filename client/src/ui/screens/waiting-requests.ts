// The requests of team sessions that only a person can answer: the desktop's
// own session attention data (a question or an approval, unacknowledged and
// not a supervised child's), matched to a team only by a verified binding: a
// live team's lead session from the scanned list, or a member session of the
// team view that is loaded. A parent link is presentation and never makes a
// session a team's. A card in the team store is not this; the core says so in
// the waiting read.
import type { TeamSummary, TeamView } from "../../store/teams";
import { sessionKey } from "../teams/session-binding";

/** The team statuses whose sessions can still ask. */
export const LIVE_TEAM = new Set(["planning", "running", "paused"]);

export interface RequestRow {
  key: string;
  name: string;
  attention?: string;
  caption?: string;
  actionNeeded: boolean;
  /** A supervised child: its requests are its parent's, not the person's. */
  quiet?: boolean;
  /** The row's verified session keys: `machine\nlaunch_id:...`, `machine\nrun_id:...`. */
  identities?: readonly string[];
}

export interface TeamRequest {
  key: string;
  name: string;
  team: string;
  caption: string;
  attention: string;
}

/** The verified team sessions: live leads from the scanned list (bounded by
 * its pages) and the loaded team's members (bounded by the core's member cap). */
export function teamSessions(listed: readonly TeamSummary[], loaded?: TeamView): Map<string, { team: string; name: string }> {
  const sessions = new Map<string, { team: string; name: string }>();
  for (const summary of listed) {
    const key = LIVE_TEAM.has(summary.status) ? sessionKey(summary.lead_session) : "";
    if (key) sessions.set(key, { team: summary.id, name: summary.name });
  }
  if (loaded && LIVE_TEAM.has(loaded.bar.status)) {
    for (const member of loaded.members) {
      const key = member.state === "removed" ? "" : sessionKey(member.session);
      if (key) sessions.set(key, { team: loaded.bar.id, name: loaded.bar.name });
    }
  }
  return sessions;
}

/** The rows a person must answer, each with the team it belongs to. The rows
 * are read lazily and the read stops at `limit`. */
export function teamRequests(
  rows: Iterable<RequestRow>,
  sessions: ReadonlyMap<string, { team: string; name: string }>,
  limit = 20,
): TeamRequest[] {
  if (!sessions.size) return [];
  const found: TeamRequest[] = [];
  for (const row of rows) {
    if (!(row.attention === "question" || row.attention === "approval") || !row.actionNeeded || row.quiet) continue;
    const team = (row.identities ?? []).map((key) => sessions.get(key)).find(Boolean);
    if (!team) continue;
    found.push({ key: row.key, name: row.name, team: team.team, caption: row.caption ?? "", attention: row.attention ?? "" });
    if (found.length >= limit) break;
  }
  return found;
}
