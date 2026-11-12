import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { TextInput } from "../lib/TextField";
import { Icon } from "../lib/icons";
import { tr } from "../../i18n";
import { editSessionGroup, groupOf, grouping, headerOf, memberOf } from "../../store/organization";
import { catalog } from "../../store/catalog";
import { archive } from "../../store/archive";
import { notify } from "../../store/actions";
import { sameInstance } from "./selection";
import type { Session } from "./types";
import type { SessionGroupChange } from "../../types";
import { setSessionGroupPrompt, type SessionGroupPrompt } from "./session-group-dialogs";

export default function SessionGroupDialog(props: { prompt: SessionGroupPrompt }) {
  const prompt = props.prompt;
  const project = () => headerOf(prompt.project);
  const groups = () => project()?.session_groups ?? [];
  const [busy, setBusy] = createSignal(false);
  const [name, setName] = createSignal("");
  const [renaming, setRenaming] = createSignal<string>();
  const [deleting, setDeleting] = createSignal<string>();
  const ready = () => !!project() && grouping().ready && !busy();
  const close = () => { if (!busy()) setSessionGroupPrompt(undefined); };
  let dialog!: HTMLDialogElement;
  onMount(() => { dialog.showModal(); queueMicrotask(() => dialog.querySelector<HTMLElement>("[data-initial]")?.focus()); });
  onCleanup(() => { dialog.close(); if (prompt.origin?.isConnected) prompt.origin.focus(); });

  const run = async (change: SessionGroupChange, done: () => void) => {
    if (!ready()) return;
    setBusy(true);
    try { const result = await editSessionGroup(prompt.project, change); if (result.outcome !== "refused") done(); }
    finally { setBusy(false); }
  };
  const assign = (group: string | null) => {
    const rows = prompt.rows.map((original) => catalog.rows[original.key] ?? archive.row(original.key));
    if (rows.some((now, i) => !now || !sameInstance(now, prompt.rows[i]) || groupOf(now.key) !== prompt.project)) {
      notify("info", tr("The selected sessions changed. Open the menu again."));
      setSessionGroupPrompt(undefined);
      return;
    }
    void run({ kind: "assign", group, members: (rows as Session[]).map(memberOf) }, () => setSessionGroupPrompt(undefined));
  };
  const submit = (event: Event) => {
    event.preventDefault();
    if (!name().trim()) return;
    const id = renaming();
    void run(id ? { kind: "rename", id, name: name() } : { kind: "create", id: "", name: name() },
      () => { setName(""); setRenaming(undefined); });
  };
  return <dialog ref={dialog} class="dlg" aria-labelledby="session-groups-title" style={{ width: "460px" }}
    onCancel={(e) => { e.preventDefault(); close(); }} onClose={close}>
    <div class="dlg-head"><Icon name="list-tree" size={18} class="accent" />
      <h2 id="session-groups-title">{prompt.rows.length ? tr("Move to session group") : tr("Session groups")}</h2>
    </div>
    <div class="dlg-body col session-groups-body">
      <p class="muted">{project()?.name ?? tr("This project is no longer listed. Open the menu again.")}</p>
      <Show when={prompt.rows.length}>
        <button type="button" class="menu-item" disabled={!ready()} data-initial onClick={() => assign(null)}>
          <Icon name="folder" size={14} />{tr("No group")}
        </button>
      </Show>
      <For each={groups()}>{(g) => <div class="row session-group-item">
        <Show when={prompt.rows.length} fallback={<span class="grow ell">{g.name}</span>}>
          <button type="button" class="menu-item grow" disabled={!ready()} onClick={() => assign(g.id)}>
            <Icon name="list-tree" size={14} /><span class="ell">{g.name}</span>
          </button>
        </Show>
        <Show when={!prompt.rows.length}>
          <button type="button" class="icon-btn" disabled={!ready()} aria-label={tr("Rename group %1", g.name)}
            onClick={() => { setRenaming(g.id); setName(g.name); setDeleting(undefined); dialog.querySelector<HTMLInputElement>("#session-group-name")?.focus(); }}><Icon name="pencil" size={14} /></button>
          <button type="button" class="icon-btn" disabled={!ready()} aria-label={tr("Delete group %1", g.name)}
            onClick={() => setDeleting(g.id)}><Icon name="trash" size={14} /></button>
        </Show>
      </div>}</For>
      <Show when={deleting()}>{(id) => <div role="alert" class="session-group-delete">
        <p>{tr("Delete “%1”? Its sessions will stay in the project without a group.", groups().find((g) => g.id === id())?.name ?? "")}</p>
        <div class="row"><button type="button" class="btn solid-danger" disabled={!ready()} onClick={() => void run({ kind: "delete", id: id() }, () => { setDeleting(undefined); setRenaming(undefined); setName(""); })}>{tr("Delete group")}</button>
          <button type="button" class="btn" onClick={() => setDeleting(undefined)}>{tr("Cancel")}</button></div>
      </div>}</Show>
      <form class="row" onSubmit={submit}>
        <TextInput id="session-group-name" class="field grow" maxLength={80} data-initial={prompt.rows.length ? undefined : true}
          aria-label={tr("Group name")} placeholder={tr("Group name")} value={name()} onInput={(e) => setName(e.currentTarget.value)} />
        <button type="submit" class="btn" disabled={!ready() || !name().trim()}>{renaming() ? tr("Save") : tr("Create group")}</button>
        <Show when={renaming()}><button type="button" class="btn" onClick={() => { setRenaming(undefined); setName(""); }}>{tr("Cancel")}</button></Show>
      </form>
      <Show when={!grouping().ready}><p role="status" class="muted">{grouping().reason}</p></Show>
    </div>
    <div class="dlg-foot"><span class="spacer" /><button type="button" class="btn" disabled={busy()} onClick={close}>{tr("Close")}</button></div>
  </dialog>;
}
