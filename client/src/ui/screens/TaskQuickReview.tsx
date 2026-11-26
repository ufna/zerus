import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { changeTaskQuickReview } from "../../bridge";
import { tr } from "../../i18n";
import { taskDetails } from "../../store/taskDetails";
import { setWritingTask, writingTask } from "../../store/taskBoard";
import { taskEditor } from "../../store/taskEditor";
import { TextArea, TextInput } from "../lib/TextField";
import { assigneeKey, outcomeLabel, reviewDetail, reviewField, reviewText, type QuickDisposition, type QuickReport, type ReportGuard } from "./quick-review";
import { reviewSessionChoices } from "./TaskReviewOutcome";
import { sessionLabel, type Task } from "./tasks-ui";

type Draft = ReportGuard & { project: string; task: string; kind: QuickDisposition["kind"]; text: string; assignee: string };
const KINDS = ["ready", "check", "rework", "parked"] as const;
const PAGE = 50;

const guard = (report: QuickReport): ReportGuard =>
  ({ report_id: report.report_id, history_revision: report.history_revision });
const matches = (a: ReportGuard, b?: ReportGuard) =>
  a.report_id === b?.report_id && a.history_revision === b.history_revision;

export function TaskQuickReview(props: { project: string; task: Task; stale: boolean; showHistory: (id?: string) => void }) {
  const [draft, setDraft] = createSignal<Draft>();
  const [busy, setBusy] = createSignal(false), [failed, setFailed] = createSignal(false);
  const [applied, setApplied] = createSignal<ReportGuard>();
  const [search, setSearch] = createSignal(""), [page, setPage] = createSignal(0);
  const report = () => props.task.quickReport;
  const disposition = () => report()?.disposition;
  const editedReportChanged = (d: Draft) => d.project !== props.project || d.task !== props.task.uuid || !matches(d, report());
  const available = () => props.task.writable && !props.stale && !busy() && !writingTask() && !!report()
    && !(applied() && matches(applied()!, report()));
  const filtered = createMemo(() => {
    const needle = search().trim().toLocaleLowerCase();
    return [...reviewSessionChoices().entries()].filter(([, row]) => !!row && row.label.toLocaleLowerCase().includes(needle));
  });
  const shown = () => filtered().slice(page() * PAGE, (page() + 1) * PAGE);
  const picked = (key: string) => reviewSessionChoices().get(key);
  const assigneeLabel = () => {
    const saved = disposition();
    if (saved?.kind === "check") return picked(assigneeKey(saved.assignee))?.label
      || report()?.assigneeLabel || tr("Assigned session unavailable");
  };
  const executor = () => props.task.executor.known && props.task.executor.id && props.task.executor.name
    ? sessionLabel(props.task.executor) : undefined;
  const editorReady = () => {
    const view = taskEditor.view();
    return !!view && view.project === props.project && view.task === props.task.uuid
      && view.mode === "note" && !view.recovery && !taskEditor.locked();
  };
  const edit = () => {
    const current = report();
    if (!available() || !current) return;
    setFailed(false); setSearch(""); setPage(0);
    const disposition = current.disposition;
    setDraft({ ...guard(current), project: props.project, task: props.task.uuid, kind: disposition?.kind ?? "ready",
      text: reviewDetail(disposition), assignee: disposition?.kind === "check" ? assigneeKey(disposition.assignee) : "" });
  };
  const fromDraft = (current: Draft): QuickDisposition | undefined => {
    const value = current.text.trim();
    if (!reviewText(value)) return;
    switch (current.kind) {
      case "ready": return { kind: current.kind, evidence: value };
      case "check": { const row = picked(current.assignee); return row ? { kind: current.kind, criterion: value, assignee: row.assignee } : undefined; }
      case "rework": return { kind: current.kind, defect: value };
      case "parked": return { kind: current.kind, reopen_when: value };
    }
  };
  const perform = async (pinned: ReportGuard, action: () => Promise<boolean>) => {
    const task = props.task.uuid;
    setBusy(true); setFailed(false); setWritingTask(task);
    try {
      if (await action()) { setApplied(pinned); setDraft(undefined); }
    } catch { setFailed(true); }
    finally {
      setBusy(false); setWritingTask(undefined); taskDetails.refresh();
    }
  };
  const save = (d: Draft) => {
    if (!available() || editedReportChanged(d)) return;
    const outcome = fromDraft(d);
    if (!outcome) return;
    const pinned = guard(d);
    void perform(pinned, async () => {
      await changeTaskQuickReview(d.project, d.task, { ...pinned, op: "set", disposition: outcome });
      return true;
    });
  };
  const accept = () => {
    const current = report(), project = props.project, task = props.task.uuid;
    if (!available() || current?.disposition?.kind !== "ready") return;
    const pinned = guard(current);
    void perform(pinned, async () => { await changeTaskQuickReview(project, task, { ...pinned, op: "accept" }); return true; });
  };
  const rework = () => {
    const current = report(), task = props.task;
    if (!available() || !editorReady() || !executor() || current?.disposition?.kind !== "rework") return;
    const pinned = guard(current);
    void perform(pinned, () => taskEditor.submit(false, {
      mode: "reopen", stage_at: task.stageAt, dispatch_id: task.dispatchId, review: pinned,
    }));
  };

  return <section class="tv-doc" aria-label={tr("Review outcome")}>
    <header><span class="eyebrow">{tr("Review outcome")}</span><span class="grow" />
      <button type="button" class="btn small ghost" onClick={() => props.showHistory(report()?.report_id)}>
        {report() ? tr("View report in History") : tr("History")}
      </button>
    </header>
    <Show when={props.task.quickStatus === "unresolved"}><p role="status">{tr("Review history must be reconciled before a verdict can be recorded.")}</p></Show>
    <Show when={report()}>{(current) => <>
      <div>{current().disposition ? outcomeLabel(current().disposition!.kind) : tr("Review needed")}</div>
      <Show when={disposition()}>{(saved) => <div class="tv-quick-evidence">
        <b>{reviewField(saved().kind)}</b><div>{reviewDetail(saved())}</div>
        <Show when={assigneeLabel()}>{(label) => <div>{label()}</div>}</Show>
      </div>}</Show>
      <Show when={failed()}><p role="alert">{tr("The review outcome could not be verified. Check the refreshed report before trying again.")}</p></Show>
      <Show when={draft()} fallback={<div class="tv-review-buttons">
        <Show when={disposition()?.kind === "ready"}><button type="button" class="btn small primary" disabled={!available()} onClick={accept}>{tr("Accept result")}</button></Show>
        <Show when={disposition()?.kind === "rework"}><button type="button" class="btn small primary"
          disabled={!available() || !editorReady() || !executor()} onClick={rework}>{executor() ? tr("Send rework to %1", executor()!) : tr("Original executor unavailable")}</button></Show>
        <button type="button" class="btn small ghost" disabled={!available()} onClick={edit}>
          {disposition()?.kind === "parked" ? tr("Review again…") : tr("Record review outcome…")}
        </button>
      </div>}>{(d) => <form class="tv-review-editor" onSubmit={(event) => { event.preventDefault(); save(d()); }}>
        <label>{tr("Review outcome")}
          <select value={d().kind} disabled={busy() || editedReportChanged(d())} onChange={(event) => {
            setDraft({ ...d(), kind: event.currentTarget.value as Draft["kind"], text: "", assignee: "" });
            setSearch(""); setPage(0);
          }}><For each={KINDS}>{(kind) => <option value={kind}>{outcomeLabel(kind)}</option>}</For></select>
        </label>
        <label>{reviewField(d().kind)}<TextArea rows={2} value={d().text} maxLength={1024}
          disabled={busy() || editedReportChanged(d())} onInput={(event) => setDraft({ ...d(), text: event.currentTarget.value })} /></label>
        <Show when={d().kind === "check"}>
          <label>{tr("Find a session on this computer")}<TextInput value={search()}
            onInput={(event) => { setSearch(event.currentTarget.value); setPage(0); }} /></label>
          <label>{tr("Check assignee")}
            <select value={d().assignee} disabled={busy() || editedReportChanged(d())}
              onChange={(event) => setDraft({ ...d(), assignee: event.currentTarget.value })}>
              <option value="">{tr("Choose a session")}</option>
              <Show when={d().assignee && picked(d().assignee) && !shown().some(([key]) => key === d().assignee)}>
                <option value={d().assignee}>{picked(d().assignee)!.label}</option>
              </Show>
              <For each={shown()}>{([key, row]) => <option value={key}>{row?.label}</option>}</For>
            </select>
          </label>
          <Show when={!reviewSessionChoices().size}><p role="status">{tr("No verified sessions on this computer.")}</p></Show>
          <Show when={filtered().length > PAGE}><div class="tv-review-buttons">
            <button type="button" class="btn small ghost" disabled={!page()} onClick={() => setPage(page() - 1)}>{tr("Previous")}</button>
            <button type="button" class="btn small ghost" disabled={(page() + 1) * PAGE >= filtered().length} onClick={() => setPage(page() + 1)}>{tr("Next")}</button>
          </div></Show>
        </Show>
        <Show when={editedReportChanged(d())}><p role="status">{tr("The report changed. Reopen the review form to use the current report.")}</p></Show>
        <div class="tv-review-buttons">
          <button type="submit" class="btn small primary" disabled={!available() || editedReportChanged(d()) || !fromDraft(d())}>{tr("Save review outcome")}</button>
          <button type="button" class="btn small ghost" disabled={busy()} onClick={() => { setDraft(undefined); setFailed(false); }}>{tr("Cancel")}</button>
        </div>
      </form>}</Show>
    </>}</Show>
  </section>;
}
