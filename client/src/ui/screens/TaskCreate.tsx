import { TextInput } from "../lib/TextField";
import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, type JSX } from "solid-js";
import { createCard, newCard } from "../../store/taskForms";
import { selected } from "../../store/ui";
import { requestTaskDescription } from "../../store/taskNavigation";
import { Icon } from "../lib/icons";
import { tr } from "../../i18n";
import { Dropdown } from "../lib/Dropdown";
import { TeamChip } from "../teams/TeamChips";
import { teams } from "../../store/teams";

/** Mockup's inline new-card title; Escape retains its private per-project draft. */
export default function TaskCreate(props: { project?: string; scope: string; backlog: boolean; accessible: boolean; children: JSX.Element }) {
  const [creating, setCreating] = createSignal(false);
  const [team, setTeam] = createSignal("");
  let field!: HTMLInputElement;
  /** The project's teams, while the preview is on: the card can join one's queue. */
  const teamChoices = createMemo(() => teams.available() === true
    ? (teams.list()?.teams ?? []).filter((candidate) => candidate.project === props.project).slice(0, 8)
    : []);
  createEffect(on(() => [props.project, props.accessible] as const, ([project, accessible]) => { setCreating(false); newCard.follow(project && accessible ? { project, task: "", mode: "new_card" } : undefined); }));
  onCleanup(() => newCard.follow());
  const view = () => newCard.view()?.project === props.project ? newCard.view() : undefined;
  createEffect(on(() => view()?.handle, () => { if (view()?.text || view()?.recovery) setCreating(true); }));
  createEffect(on(() => [creating(), view()?.handle] as const, () => { if (creating() && view()) queueMicrotask(() => field?.focus()); }));
  const submit = async () => {
    const project = props.project; if (!project || newCard.locked() || !view()?.text.trim() && !view()?.recovery) return;
    if (await createCard(project, team() ? false : props.backlog, selected(), team() || undefined)) {
      setCreating(false); const id = newCard.view()?.created; if (id) requestTaskDescription(project, id);
    }
  };
  return <><div class="tk-head"><Show when={creating() && props.project} fallback={<span class="tk-scope" aria-live="polite">{props.scope}</span>}>
    <TextInput ref={field} class="field tk-newtitle" maxlength={512} placeholder={tr("Task title")} aria-label={tr("New task title")} data-shortcut-context="none" value={view()?.text ?? ""} disabled={!!view()?.blocked} readOnly={newCard.opening() || newCard.busy() || view()?.read_only}
      onInput={(e) => newCard.edit(e.currentTarget.value, e.currentTarget.selectionEnd ?? 0, e.currentTarget.selectionStart ?? 0)} onBlur={() => void newCard.flush().catch(() => {})}
      onKeyDown={(e) => { if (e.isComposing) return; if (e.key === "Enter") { e.preventDefault(); void submit(); } else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setCreating(false); void newCard.flush().catch(() => {}); } }} />
  </Show><Show when={creating() && teamChoices().length}>
      <Dropdown class="field tk-team" aria-label={tr("Team")} title={tr("Send the new task to a team's queue")}
        value={team()} onChange={setTeam} options={[{ value: "", label: tr("No team") }, ...teamChoices().map((candidate) => ({ value: candidate.id, textValue: candidate.name, label: () => <TeamChip name={candidate.name} /> }))]} />
    </Show><button class="tk-sq add" aria-label={tr("New task")} title={tr("New task")} disabled={!props.project || !props.accessible} onClick={() => { setCreating(true); queueMicrotask(() => field?.focus()); }}><Icon name="plus" size={16} /></button>{props.children}</div>
    <Show when={newCard.error() && creating()}><div class="tk-create-error" role="alert">{newCard.error()}</div></Show></>;
}
