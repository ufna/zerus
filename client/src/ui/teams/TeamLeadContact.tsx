import { Show, createEffect, createSignal, on } from "solid-js";
import { tr } from "../../i18n";
import type { TeamOutcome } from "../../store/teams";
import type { EditAction } from "./TeamEditors";

export function TeamLeadContact(props: {
  team: string;
  revision: number;
  status: string;
  busy: boolean;
  onAction: EditAction;
}) {
  const [open, setOpen] = createSignal(false);
  const [note, setNote] = createSignal(false);
  const [body, setBody] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [saved, setSaved] = createSignal("");
  const active = () => ["planning", "running", "paused"].includes(props.status);
  const valid = () => body().trim() && [...body()].length <= 1000
    && new TextEncoder().encode(body()).length <= 4000
    && !/[\x00-\x09\x0b-\x1f\x7f-\x9f\u061c\u200e\u200f\u2028-\u202e\u2066-\u2069]/u.test(body());
  const busy = () => props.busy || submitting();
  let entry!: HTMLButtonElement;
  let text!: HTMLTextAreaElement;
  const close = () => {
    setOpen(false);
    entry?.focus();
  };
  createEffect(on(() => props.team, () => {
    setOpen(false);
    setNote(false);
    setBody("");
    setSaved("");
  }));

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!valid() || busy() || !active()) return;
    const team = props.team, revision = props.revision, isNote = note();
    setSubmitting(true);
    try {
      const outcome = await props.onAction(isNote ? "leave_lead_note" : "ask_lead", { text: body() }, revision, team) as TeamOutcome | undefined;
      if (props.team !== team || outcome?.outcome !== "done") return;
      setBody("");
      setSaved(isNote ? tr("Note saved for the lead.") : tr("Question saved for the lead."));
      close();
    } finally {
      setSubmitting(false);
    }
  };

  return <div class="team-lead-contact">
    <button type="button" class="btn small" ref={entry} data-team-contact
      disabled={!active() || busy()} aria-expanded={open()} onClick={() => {
        if (open()) close();
        else { setSaved(""); setOpen(true); queueMicrotask(() => text?.focus()); }
      }}>{tr("Ask the lead…")}</button>
    <Show when={saved()}>{(message) => <span class="dim" role="status">{message()}</span>}</Show>
    <Show when={open() && active()}>
      <form class="dlg-body team-drawer-text" aria-label={tr("Ask the lead…")} onSubmit={(event) => void submit(event)}>
        <label>{tr("Message type")}
          <select class="field" value={note() ? "note" : "question"} disabled={busy()}
            onChange={(event) => setNote(event.currentTarget.value === "note")}>
            <option value="question">{tr("Question")}</option>
            <option value="note">{tr("Leave a note")}</option>
          </select>
        </label>
        <label>{note() ? tr("Note") : tr("Question")}
          <textarea ref={text} class="field" rows={4} maxlength={2000} value={body()} disabled={busy()}
            onInput={(event) => setBody(event.currentTarget.value)} />
        </label>
        <p class="dim">{tr("Use up to 1,000 characters and 4,000 bytes.")}</p>
        <div class="team-drawer-actions">
          <button type="submit" class="btn small primary" disabled={!valid() || busy()}>{note() ? tr("Leave a note") : tr("Ask the lead…")}</button>
          <button type="button" class="btn small" disabled={busy()} onClick={close}>{tr("Cancel")}</button>
        </div>
      </form>
    </Show>
  </div>;
}
