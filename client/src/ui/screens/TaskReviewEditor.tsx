import { createSignal, Show } from "solid-js";
import { classifyTaskReview } from "../../bridge";
import { tr } from "../../i18n";
import { taskDetails } from "../../store/taskDetails";
import { writingTask } from "../../store/taskBoard";
import { TextInput } from "../lib/TextField";
import type { Task } from "./tasks-ui";

type Reviewer = "person" | "technical" | "unspecified";
type Draft = { revision: string; owner: Reviewer; criterion: string };

export function TaskReviewEditor(props: { project: string; task: Task; stale: boolean }) {
  const [draft, setDraft] = createSignal<Draft>();
  const [saving, setSaving] = createSignal(false);
  const [failed, setFailed] = createSignal(false);
  const changed = () => draft()?.revision !== props.task.historyRevision || !props.task.reviewEditable;
  const valid = (draft: Draft) => draft.owner === "unspecified" || (
    !!draft.criterion.trim() && new TextEncoder().encode(draft.criterion.trim()).length <= 512
    && !/[\p{Cc}\u2028-\u202e\u2066-\u2069]/u.test(draft.criterion.trim())
  );
  const edit = () => {
    setFailed(false);
    setDraft({
      revision: props.task.historyRevision,
      owner: props.task.reviewAssignment?.owner ?? "unspecified",
      criterion: props.task.reviewAssignment?.criterion ?? "",
    });
  };
  const save = async (draft: Draft) => {
    if (changed() || props.stale || saving() || writingTask() || !valid(draft)) return;
    setSaving(true);
    setFailed(false);
    try {
      await classifyTaskReview(props.project, props.task.uuid, draft.revision, draft.owner,
        draft.owner === "unspecified" ? "" : draft.criterion.trim());
      setDraft(undefined);
    } catch {
      setFailed(true);
    } finally {
      setSaving(false);
      taskDetails.refresh();
    }
  };

  return <Show when={draft()} fallback={<Show when={props.task.reviewEditable}>
    <button type="button" class="btn small ghost" disabled={props.stale || !!writingTask()} onClick={edit}>{tr("Choose next reviewer")}</button>
  </Show>}>{(value) =>
    <form class="tv-review-editor" onSubmit={(event) => { event.preventDefault(); void save(value()); }}>
      <label>{tr("Next reviewer")}
        <select value={value().owner} disabled={saving()} onChange={(event) => {
          const owner = event.currentTarget.value as Reviewer;
          setDraft({ ...value(), owner });
        }}>
          <option value="unspecified">{tr("Coordinator triage")}</option>
          <option value="person">{tr("Person review")}</option>
          <option value="technical">{tr("Technical review")}</option>
        </select>
      </label>
      <Show when={value().owner !== "unspecified"}>
        <label>{tr("Acceptance criterion")}
          <TextInput value={value().criterion} maxLength={512} disabled={saving()} required
            onInput={(event) => setDraft({ ...value(), criterion: event.currentTarget.value })} />
        </label>
      </Show>
      <Show when={changed()}><p role="status">{tr("The task changed. Reopen the reviewer form to use the current report.")}</p></Show>
      <Show when={failed()}><p role="alert">{tr("The review assignment could not be verified. Check the refreshed task before trying again.")}</p></Show>
      <div class="tv-review-buttons">
        <button class="btn small" type="submit" disabled={props.stale || changed() || saving() || !!writingTask() || !valid(value())}>{tr("Save reviewer")}</button>
        <button class="btn small ghost" type="button" disabled={saving()} onClick={() => { setDraft(undefined); setFailed(false); }}>{tr("Cancel")}</button>
      </div>
    </form>
  }</Show>;
}
