
import { trNoop, tr } from "../../../i18n";// The approval journal's reads as AutomaticApprovalSettings.cpp runs them:
// one list read at a time (an edit during a read reads once more, never a
// queue of pages), Newer and Older by the journal's cursors, nothing to page
// or select after a failed read, and an entry's detail where only the
// latest selection's answer counts.

export interface Entry { id: number; at: number; host: string; session: string; prompt: string; result: string; categories?: string[]; scope?: string | null; standing?: string | null }

/** The trust sentence a row's scope yields (zerus-63rru.66 round 4): only a
 * folder-trust record shows one, and only a confirmed grant claims the
 * trust; anything else says trust was requested. A team's Codex hook-trust
 * record says what it covered, by the same rule (zerus-f67qp). */
export function trustSentence(entry: Entry): string | null {
  if (entry.standing === "team_hook_trust") {
    return entry.result === "Approved once"
      ? tr("Codex hook trust recorded before start for Zerus's own and the team's committed hooks")
      : tr("Codex hook trust was requested; it was not confirmed");
  }
  const scope = entry.scope?.trim();
  if (!scope || entry.standing !== "team_folder_trust") return null;
  return entry.result === "Approved once"
    ? tr("Trusted for %1 and its subfolders on this account", scope)
    : tr("Trust was requested for %1 and its subfolders; it was not confirmed", scope);
}
/** The core's own sentences in a selected entry's detail, which it writes in
 * English (zerus-f67qp review): each translated where shown. The agent's
 * question and choices, and any other recorded text, stay as recorded. */
const DETAIL_SENTENCES: readonly string[] = [
  trNoop("Approved once"), trNoop("Failed"), trNoop("Cancelled"), trNoop("Unconfirmed"),
  trNoop("Codex hook trust recorded before start for Zerus's own and the team's committed hooks"),
  trNoop("Codex hook trust was requested; it was not confirmed"),
  trNoop("Trust Zerus's own and the team's committed Codex hooks"),
  trNoop("Codex saved trust for exactly these hooks"),
  trNoop("Codex's saved trust is not exactly these hooks"),
];
const DETAIL_PATTERNS: readonly (readonly [RegExp, string])[] = [
  [/^Trusted for (.+) and its subfolders on this account$/s, trNoop("Trusted for %1 and its subfolders on this account")],
  [/^Trust was requested for (.+) and its subfolders; it was not confirmed$/s, trNoop("Trust was requested for %1 and its subfolders; it was not confirmed")],
  [/^Hooks to trust: (\d+)$/, trNoop("Hooks to trust: %1")],
  [/^Selected answer: (.*)$/s, trNoop("Selected answer: %1")],
];
export function localizedDetail(text: string): string {
  return text.split("\n\n").map((part) => {
    if (DETAIL_SENTENCES.includes(part)) return tr(part);
    for (const [pattern, source] of DETAIL_PATTERNS) {
      const match = pattern.exec(part);
      if (match) return tr(source, match[1]);
    }
    return part;
  }).join("\n\n");
}
/** Read identity only; native code rechecks it against the selected service. */
export interface JournalScope { configuration: string; epoch: string; generation: number; journal: string; through: number; host: string; session: string }
export interface Page { entries: Entry[]; next: number; previous: number; scope?: JournalScope }
export interface ListRequest { host: string; session: string; before: number; after: number; scope?: JournalScope }

export interface JournalState {
  page: Page;
  /** A read runs: nothing to page or select. */
  loading: boolean;
  status: string;
  selected?: string;
  detail?: string;
  detailHint: string;
}

export interface JournalIo {
  list(request: ListRequest): Promise<Page>;
  detail(id: number, scope?: JournalScope): Promise<string>;
}

export const PLACEHOLDER = trNoop("Select a journal entry to read the question and its outcome.");
const EMPTY: Page = { entries: [], next: 0, previous: 0 };

export function createJournal(io: JournalIo, filters: () => { host: string; session: string }, show: (state: JournalState) => void) {
  let state: JournalState = { page: EMPTY, loading: false, status: "", detailHint: tr(PLACEHOLDER) };
  let before = 0;
  let after = 0;
  let scope: JournalScope | undefined;
  let busy = false;
  let again = false;
  let detailRequest = 0;
  const update = (change: Partial<JournalState>) => { state = { ...state, ...change }; show(state); };

  const refresh = () => {
    detailRequest++;
    update({ status: tr("Loading approval journal…"), loading: true, selected: undefined, detail: undefined, detailHint: tr(PLACEHOLDER) });
    if (busy) { again = true; return; }
    busy = true;
    const done = (finish: () => void) => {
      busy = false;
      if (again) { again = false; refresh(); return; }
      finish();
    };
    io.list({ ...filters(), before, after, ...(scope ? { scope } : {}) }).then(
      (page) => done(() => update({
        page,
        loading: false,
        status: page.entries.length
          ? `${page.entries.length} entries. Unconfirmed attempts require checking Activity or Terminal; they are not retried.`
          : tr("No automatic approvals recorded for this filter."),
      })),
      // A failed read leaves no page to select from or move through.
      (error) => done(() => update({ page: EMPTY, loading: false, status: tr(String(error)) })),
    );
  };

  return {
    state: () => state,
    firstPage() { before = 0; after = 0; scope = undefined; refresh(); },
    newer() { if (state.loading || state.page.previous <= 0) return; after = state.page.previous; before = 0; scope = state.page.scope; refresh(); },
    older() { if (state.loading || state.page.next <= 0) return; before = state.page.next; after = 0; scope = state.page.scope; refresh(); },
    select(key: string) {
      if (state.loading) return;
      const request = ++detailRequest;
      update({ selected: key, detail: undefined, detailHint: tr("Loading question…") });
      io.detail(Number(key), state.page.scope).then(
        (text) => { if (request === detailRequest) update({ detail: localizedDetail(text) }); },
        (error) => { if (request === detailRequest) update({ detail: tr(String(error)) }); },
      );
    },
  };
}
