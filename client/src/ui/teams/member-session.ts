import { batch } from "solid-js";
import { tr } from "../../i18n";
import { orderedRows } from "../../store/catalog";
import { setDetailTab, setPage, setSelectedKey, setSubagentOpen } from "../../store/ui";
import type { Session } from "../sessions/types";
import { rowSessionKeys, sessionKey } from "./session-binding";
import type { MemberView } from "./types";

// A member's own session: the session row bound by the member's machine and
// launch or run, the binding the lead's Team tab uses; never a similarly named
// row. One pass over the session list finds every shown member's row and
// stops once all are found, as the Teams screen finds its leads.

export type MemberSession = { row: Session; reason?: undefined } | { row?: undefined; reason: string };

/** Each member's session row, or why it cannot be opened, by member ID. */
export function memberSessions(members: readonly Pick<MemberView, "id" | "session">[]): Map<string, MemberSession> {
  const wanted = new Set(members.map((member) => sessionKey(member.session)).filter(Boolean));
  const found = new Map<string, Session>();
  if (wanted.size) for (const row of orderedRows()) {
    for (const key of rowSessionKeys(row)) if (wanted.has(key) && !found.has(key)) found.set(key, row);
    if (found.size === wanted.size) break;
  }
  return new Map(members.map((member) => {
    const key = sessionKey(member.session);
    const row = key ? found.get(key) : undefined;
    const session: MemberSession = row ? { row }
      : { reason: key ? tr("This member's session is not on the session list.") : tr("This member has no linked session.") };
    return [member.id, session];
  }));
}

/** Opens the member's own session on its Activity, as a session request
 * does. A native child selected under the previous session is cleared in the
 * same step, as ordinary session navigation does (store/desk.ts): it belongs
 * to that session, never to the member's. */
export function openMemberSession(row: Session) {
  batch(() => { setPage("sessions"); setSubagentOpen(undefined); setSelectedKey(row.key); setDetailTab("activity"); });
}
