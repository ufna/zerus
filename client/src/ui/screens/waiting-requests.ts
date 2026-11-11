// The requests of team sessions that only a person can answer: the desktop's
// own session attention data (a question or an approval, unacknowledged and
// not a supervised child's), matched to a team only by a verified binding on
// that team's own list row: its lead_session or one of its member_sessions,
// and only while the same row says the team is live. A parent link is
// presentation and never makes a session a team's; a loaded team view, which
// can be older than the list, is not a source. A card in the team store is not
// this; the core says so in the waiting read.
import type { TeamSummary } from "../../store/teams";
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

/** The verified team sessions of the scanned list (bounded by its pages and
 * the core's member cap): each live row's lead and members, keyed by machine
 * and session. Status and membership come from the same row. */
export function teamSessions(listed: readonly TeamSummary[]): Map<string, { team: string; name: string }> {
  const sessions = new Map<string, { team: string; name: string }>();
  for (const summary of listed) {
    if (!LIVE_TEAM.has(summary.status)) continue;
    const team = { team: summary.id, name: summary.name };
    const lead = sessionKey(summary.lead_session);
    if (lead) sessions.set(lead, team);
    for (const member of summary.member_sessions ?? []) {
      const key = sessionKey(member);
      if (key) sessions.set(key, team);
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
