import type { Page } from "../store/ui";
import type { Usage } from "../types";
import type { Session } from "./sessions/types";

/** SessionsWindow::renderAccountUsage's visibility rule. */
export function statusUsageVisible(session: Session | undefined, currentPage: Page, subagent: boolean): boolean {
  return currentPage === "sessions" && !!session?.tracked && !session.archiveId && !subagent;
}

/** The Accounts page profile key for the bound account, when it is known. */
export function statusUsageProfile(session: Session, usage: Usage): string | undefined {
  return usage.id ? `${session.computer}\u001f${usage.id}` : undefined;
}
