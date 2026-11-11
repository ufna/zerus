// The requests of team sessions that only a person can answer: the desktop's
// own session attention data (a question or an approval, unacknowledged),
// matched to a team by the lead's session or by the session's parent link.
// A card in the team store is not this; the core says so in the waiting read.

export interface RequestRow {
  key: string;
  name: string;
  attention?: string;
  caption?: string;
  actionNeeded: boolean;
  /** The row's own identity: `launch_id:...` or `run_id:...`. */
  identity: string;
  /** The parent link's identity, when the session nests under a lead. */
  parentIdentity: string;
}

export interface TeamRequest {
  key: string;
  name: string;
  team: string;
  caption: string;
  attention: string;
}

/** The rows a person must answer, each with the team it belongs to. */
export function teamRequests(
  rows: readonly RequestRow[],
  byLead: ReadonlyMap<string, { team: string; name: string }>,
  limit = 20,
): TeamRequest[] {
  if (!byLead.size) return [];
  const found: TeamRequest[] = [];
  for (const row of rows) {
    if (!(row.attention === "question" || row.attention === "approval") || !row.actionNeeded) continue;
    const team = byLead.get(row.identity) ?? byLead.get(row.parentIdentity);
    if (!team) continue;
    found.push({ key: row.key, name: row.name, team: team.team, caption: row.caption ?? "", attention: row.attention ?? "" });
    if (found.length >= limit) break;
  }
  return found;
}
