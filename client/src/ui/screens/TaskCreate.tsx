import { TextInput } from "../lib/TextField";
import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, type JSX } from "solid-js";
import { createCard, newCard } from "../../store/taskForms";
import { sendToTeam } from "../../store/taskBoard";
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
  /** The team chosen in this form, for this project only. */
  const [team, setTeam] = createSignal("");
  const [refusal, setRefusal] = createSignal("");
  /** A card the command line created and the team's queue did not take: the
   * task exists, so the way on is Send to team for that task, never a new card. */
  const [notQueued, setNotQueued] = createSignal<{ project: string; task: string; team: string; name: string; reason: string } | null>(null);
  const [sending, setSending] = createSignal(false);
  let field!: HTMLInputElement;
  /** The project's teams on the list's first page, while the preview is on: the card can join one's queue. */
  const teamChoices = createMemo(() => teams.available() === true
    ? (teams.list()?.teams ?? []).filter((candidate) => candidate.project === props.project).slice(0, 8)
    : []);
  createEffect(on(() => [props.project, props.accessible] as const, ([project, accessible]) => { setCreating(false); newCard.follow(project && accessible ? { project, task: "", mode: "new_card" } : undefined); }));
  onCleanup(() => newCard.follow());
  const view = () => newCard.view()?.project === props.project ? newCard.view() : undefined;
  // A choice belongs to the form's project and to a new card: another project,
  // lost access, Teams going off or a reserved card clear it.
  // Compared by value: an edit of the title is a new view of the same card and keeps the choice.
  const binding = createMemo(() => JSON.stringify([props.project ?? "", props.accessible, teams.available() ?? null, view()?.handle ?? null, !!view()?.recovery]));
  createEffect(on(binding, () => { setTeam(""); setRefusal(""); }, { defer: true }));
  /** A reserved card keeps the destination it was reserved with. */
  const reserved = () => view()?.recovery ? view()?.team ?? "" : undefined;
  const reservedName = () => teamChoices().find((candidate) => candidate.id === reserved())?.name ?? reserved() ?? "";
  createEffect(on(() => view()?.handle, () => { if (view()?.text || view()?.recovery) setCreating(true); }));
  createEffect(on(() => [creating(), view()?.handle] as const, () => { if (creating() && view()) queueMicrotask(() => field?.focus()); }));
  const submit = async () => {
    const project = props.project; if (!project || newCard.locked() || !view()?.text.trim() && !view()?.recovery) return;
    // Recovery sends the reserved card as it was reserved; a new card takes
    // only a team the form still offers for this project.
    const chosen = view()?.recovery ? "" : team();
    if (chosen && !teamChoices().some((candidate) => candidate.id === chosen)) {
      setTeam(""); setRefusal(tr("The chosen team is no longer offered for this project. Nothing was sent."));
      return;
    }
    setRefusal(""); setNotQueued(null);
    const name = teamChoices().find((candidate) => candidate.id === chosen)?.name ?? chosen;
    if (await createCard(project, chosen ? false : props.backlog, selected(), chosen || undefined)) {
      setCreating(false); setTeam("");
      const done = newCard.view(), id = done?.created, refused = done?.not_queued;
      if (id && refused && refused.task === id) setNotQueued({ project, task: id, team: refused.team, name: refused.team === chosen ? name : refused.team, reason: refused.reason });
      if (id) requestTaskDescription(project, id);
    }
  };
  const sendCreated = async () => {
    const created = notQueued();
    if (!created || sending()) return;
    setSending(true);
    try { if (await sendToTeam(created.project, { uuid: created.task }, created.team, created.name)) setNotQueued(null); }
    finally { setSending(false); }
  };
  return <><div class="tk-head"><Show when={creating() && props.project} fallback={<span class="tk-scope" aria-live="polite">{props.scope}</span>}>
    <TextInput ref={field} class="field tk-newtitle" maxlength={512} placeholder={tr("Task title")} aria-label={tr("New task title")} data-shortcut-context="none" value={view()?.text ?? ""} disabled={!!view()?.blocked} readOnly={newCard.opening() || newCard.busy() || view()?.read_only}
      onInput={(e) => newCard.edit(e.currentTarget.value, e.currentTarget.selectionEnd ?? 0, e.currentTarget.selectionStart ?? 0)} onBlur={() => void newCard.flush().catch(() => {})}
      onKeyDown={(e) => { if (e.isComposing) return; if (e.key === "Enter") { e.preventDefault(); void submit(); } else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setCreating(false); void newCard.flush().catch(() => {}); } }} />
  </Show><Show when={creating() && reserved() !== undefined && reserved()}>
      <Dropdown class="field tk-team" aria-label={tr("Team")} title={tr("The reserved card keeps its team")} disabled
        value={reserved()!} onChange={() => {}} options={[{ value: reserved()!, textValue: reservedName(), label: () => <TeamChip name={reservedName()} /> }]} />
    </Show><Show when={creating() && reserved() === undefined && teamChoices().length}>
      <Dropdown class="field tk-team" aria-label={tr("Team")} title={tr("Send the new task to a team's queue")}
        value={team()} onChange={(value) => { setTeam(value); setRefusal(""); }} options={[{ value: "", label: tr("No team") }, ...teamChoices().map((candidate) => ({ value: candidate.id, textValue: candidate.name, label: () => <TeamChip name={candidate.name} /> }))]} />
    </Show><button class="tk-sq add" aria-label={tr("New task")} title={tr("New task")} disabled={!props.project || !props.accessible} onClick={() => { setCreating(true); queueMicrotask(() => field?.focus()); }}><Icon name="plus" size={16} /></button>{props.children}</div>
    <Show when={(refusal() || newCard.error()) && creating()}><div class="tk-create-error" role="alert">{refusal() || newCard.error()}</div></Show>
    <Show when={notQueued()?.project === props.project ? notQueued() : undefined}>{(created) => <div class="tk-create-error" role="alert" data-not-queued>
      <span>{tr("The task was created, but %1 did not queue it: %2", created().name, created().reason)}</span>{" "}
      <button type="button" class="btn small" disabled={sending()} onClick={() => void sendCreated()}>{tr("Send to team")}</button>{" "}
      <button type="button" class="btn small" onClick={() => setNotQueued(null)}>{tr("Dismiss")}</button>
    </div>}</Show></>;
}
