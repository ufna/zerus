import { machineCards } from "../../store/machines";
import type { SessionIdentity, TeamSession } from "../../store/teams";
import type { Session } from "../sessions/types";

// The one binding between a session row and a team's session reference: the
// row's computer's machine UUID and its launch or run. The Team tab, the entry
// gate and Waiting for you all use it; a row's native session id, its name or
// a parent link never stands for it.

type Row = Pick<Session, "computer" | "launchId" | "runId">;

/** The identity a team binding is compared with; no machine when its card is unknown. */
export function rowIdentity(row: Row): SessionIdentity {
  return { machine: machineCards[row.computer]?.machine_id || null, launchId: row.launchId || null, runId: row.runId || null };
}

/** The keys a row answers to: `machine\nlaunch_id:...` and `machine\nrun_id:...`. */
export function rowSessionKeys(row: Row): string[] {
  const machine = machineCards[row.computer]?.machine_id;
  if (!machine) return [];
  return [row.launchId ? `${machine}\nlaunch_id:${row.launchId}` : "", row.runId ? `${machine}\nrun_id:${row.runId}` : ""].filter(Boolean);
}

/** The key of a team's session reference, or "" when it names no machine or session. */
export const sessionKey = (ref: TeamSession | null | undefined) => ref?.machine && ref.session_id ? `${ref.machine}\n${ref.session_id}` : "";

/** Whether a row is the bound session: the same machine and the same launch or run. */
export function rowIsSession(row: Row, ref: TeamSession | null | undefined): boolean {
  const key = sessionKey(ref);
  return !!key && rowSessionKeys(row).includes(key);
}
