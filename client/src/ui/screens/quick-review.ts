import { tr, trc } from "../../i18n";

export type QuickAssignee = { machine: string; session_id: string };
export type QuickDisposition =
  | { kind: "ready"; evidence: string }
  | { kind: "check"; criterion: string; assignee: QuickAssignee }
  | { kind: "rework"; defect: string }
  | { kind: "parked"; reopen_when: string };
export type ReportGuard = { history_revision: string; report_id: string };
export type QuickReviewChange = ReportGuard & ({ op: "set"; disposition: QuickDisposition } | { op: "accept" });
export type QuickReport = ReportGuard & { disposition?: QuickDisposition; assigneeLabel?: string };
export type QuickStatus = QuickDisposition["kind"] | "unclassified" | "unresolved" | "team";
export type QuickQueue = "work" | "ready" | "parked" | "unresolved" | "all";
export const QUICK_QUEUES = ["work", "ready", "parked", "unresolved", "all"] as const;
export const QUICK_LANES = ["all", "work", "ready", "check", "rework", "parked", "unclassified", "unresolved"] as const;
export type QuickCounts = Record<typeof QUICK_LANES[number], number>;

export function reviewRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown> : {};
}
export const reviewText = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.trim().length > 0
  && new TextEncoder().encode(value).length <= 1024
  && !/[\p{Cc}\u2028-\u202e\u2066-\u2069]/u.test(value.replace(/[\n\t]/g, ""));
export const reportId = (value: unknown): value is string =>
  typeof value === "string" && !!value && new TextEncoder().encode(value).length <= 512
  && !/[\s\p{Cc}\u2028-\u202e\u2066-\u2069]/u.test(value);
export const canonicalReviewUuid = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value)
  && value !== "00000000-0000-0000-0000-000000000000";
export const quickAssignee = (machine: unknown, id: unknown): QuickAssignee | undefined =>
  canonicalReviewUuid(machine) && reportId(id) && new TextEncoder().encode(id).length <= 200
    && (id.startsWith("run_id:") && id.length > 7 || id.startsWith("launch_id:") && canonicalReviewUuid(id.slice(10)))
    ? { machine, session_id: id } : undefined;
export const assigneeKey = (assignee: QuickAssignee) => `${assignee.machine}\n${assignee.session_id}`;

export function readDisposition(value: unknown): QuickDisposition | undefined {
  const v = reviewRecord(value);
  switch (v.kind) {
    case "ready": return reviewText(v.evidence) ? { kind: v.kind, evidence: v.evidence } : undefined;
    case "check": {
      const a = reviewRecord(v.assignee), assignee = quickAssignee(a.machine, a.session_id);
      return reviewText(v.criterion) && assignee ? { kind: v.kind, criterion: v.criterion, assignee } : undefined;
    }
    case "rework": return reviewText(v.defect) ? { kind: v.kind, defect: v.defect } : undefined;
    case "parked": return reviewText(v.reopen_when) ? { kind: v.kind, reopen_when: v.reopen_when } : undefined;
  }
}

export const quickStatus = (value: unknown): QuickStatus | undefined => {
  switch (value) {
    case "ready": case "check": case "rework": case "parked":
    case "unclassified": case "unresolved": case "team": return value;
  }
};
export function readQuickReport(value: Record<string, unknown>, eligible: boolean): QuickReport | undefined {
  const status = quickStatus(value.quick_review_status);
  if (eligible && value.stage === "review" && status && status !== "unresolved" && status !== "team"
      && reportId(value.review_report_id) && typeof value.history_revision === "string"
      && /^[0-9a-f]{64}$/.test(value.history_revision)) {
    const disposition = readDisposition(value.quick_review);
    if (status === "unclassified" ? value.quick_review == null : disposition?.kind === status) {
      const hint = reviewRecord(reviewRecord(value.quick_review).assignee);
      const assigneeLabel = disposition?.kind === "check" && reviewText(hint.session)
        ? reviewText(hint.host) ? tr("%1 @%2", hint.session, hint.host) : hint.session
        : undefined;
      return { report_id: value.review_report_id, history_revision: value.history_revision, disposition, assigneeLabel };
    }
  }
}

export function readQuickCounts(value: unknown): QuickCounts | undefined {
  const counts = reviewRecord(value);
  if (!QUICK_LANES.every((lane) => typeof counts[lane] === "number" &&
      Number.isSafeInteger(counts[lane]) && (counts[lane] as number) >= 0)) return;
  const result = counts as QuickCounts;
  if (result.work !== result.check + result.rework + result.unclassified) return;
  return result;
}

export function reviewField(kind: QuickDisposition["kind"]): string {
  switch (kind) {
    case "ready": return tr("Supporting evidence");
    case "check": return tr("Acceptance criterion");
    case "rework": return tr("Concrete defect or example");
    case "parked": return tr("Reopening condition");
  }
}
export function reviewDetail(disposition?: QuickDisposition): string {
  switch (disposition?.kind) {
    case "ready": return disposition.evidence;
    case "check": return disposition.criterion;
    case "rework": return disposition.defect;
    case "parked": return disposition.reopen_when;
    default: return "";
  }
}

export function outcomeLabel(kind: QuickDisposition["kind"]): string {
  switch (kind) {
    case "ready": return tr("Ready to accept");
    case "check": return tr("Check assigned");
    case "rework": return tr("Rework needed");
    case "parked": return tr("Parked");
  }
}

export function queueLabel(lane: QuickQueue): string {
  switch (lane) {
    case "work": return trc("review queue", "Review work");
    case "all": return tr("All ordinary reviews");
    case "unresolved": return tr("History reconciliation");
    default: return outcomeLabel(lane);
  }
}
