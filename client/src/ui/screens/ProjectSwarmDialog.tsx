import { For, Match, Show, Switch, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "../lib/icons";
import { MachineLabel } from "../components/MachineLabel";
import { ProjectLabel } from "../components/ProjectLabel";
import { inTauri } from "../../bridge";
import { setPage } from "../../store/ui";
import type { Project } from "../../types";
import { createSwarm, syncText } from "./project-swarm";
import "../styles/project-swarm.css";

/** setup-swarm Computers / Connect computer; the native worker owns every call. */
export default function ProjectSwarmDialog(props: { projects: Project[]; connect?: boolean; origin: HTMLElement; close(): void }) {
  const state = createSwarm(), view = state.frame;
  const [tab, setTab] = createSignal<"computers" | "connect">(props.connect ? "connect" : "computers");
  const [filter, setFilter] = createSignal("");
  const [sort, setSort] = createSignal({ column: 0, ascending: true });
  const [alias, setAlias] = createSignal("");
  const [requested, setRequested] = createSignal<{ alias: string; previous: string | undefined }>();
  const [validPreview, setValidPreview] = createSignal<string>();
  const [mapping, setMapping] = createSignal<Record<string, string>>({});
  const [keep, setKeep] = createSignal(true);
  const busy = () => state.working() || !!view()?.busy;
  const changing = () => state.working() || !!view()?.changing;
  const preview = () => { const p = view()?.preview; return p?.id === validPreview() && p?.alias === alias() ? p : undefined; };
  const differs = () => !!preview()?.initialized && !preview()?.same_swarm;
  const loading = () => !!requested() && busy() && view()?.mode === "preview";
  const computers = createMemo(() => view()?.catalog?.computers ?? []);
  const computerById = createMemo(() => new Map(computers().map((row) => [row.id, row])));
  const localById = createMemo(() => new Map((view()?.projects ?? []).map((p) => [p.id, p])));
  const rows = createMemo(() => {
    const f = filter().trim().toLowerCase(), s = sort();
    const key = (row: (ReturnType<typeof computers>)[number]) => [row.name, row.access, syncText(row)][s.column];
    return computers().filter((r) => !f || `${r.name} ${r.access} ${syncText(r)}`.toLowerCase().includes(f))
      .toSorted((a, b) => (key(a).localeCompare(key(b)) || a.id.localeCompare(b.id)) * (s.ascending ? 1 : -1));
  });
  let dialog!: HTMLDialogElement, gone = false, off: (() => void) | undefined;
  const close = async () => { if (await state.close()) props.close(); };
  createEffect(() => {
    const aliases = view()?.aliases ?? [];
    if (!aliases.includes(alias())) { setAlias(aliases[0] ?? ""); setRequested(undefined); setValidPreview(undefined); }
  });
  createEffect(() => {
    const p = view()?.preview, req = requested();
    if (p && req && p.id !== req.previous && p.alias === req.alias && p.alias === alias() && p.id !== validPreview()) {
      setValidPreview(p.id); setMapping({}); setKeep(true);
    }
  });
  const chooseAlias = (next: string) => { setAlias(next); setRequested(undefined); setValidPreview(undefined); setMapping({}); };
  const runPreview = async () => {
    if (busy() || !alias()) return;
    setValidPreview(undefined); setMapping({});
    setRequested({ alias: alias(), previous: view()?.preview?.id });
    await state.preview(alias());
  };
  const join = () => {
    if (!preview()?.initialized || !view()?.can_join || busy()) return;
    const pairs: [string, string][] = differs() ? (view()?.projects ?? []).flatMap((p) => mapping()[p.id] ? [[p.id, mapping()[p.id]] as [string, string]] : []) : [];
    void state.join(pairs, keep());
  };
  const manage = async () => { if (await state.close()) { props.close(); setPage("machines"); } };
  onMount(() => {
    dialog.showModal();
    void (async () => {
      const stop = inTauri() ? await listen("project-swarm-changed", state.changed) : () => {};
      if (gone) { stop(); return; }
      off = stop; await state.open();
      if (!gone) dialog.querySelector<HTMLElement>(tab() === "computers" ? "input[type=search]" : "select")?.focus();
    })();
  });
  onCleanup(() => { gone = true; off?.(); state.dispose(); dialog.close(); queueMicrotask(() => { if (props.origin.isConnected) props.origin.focus(); }); });
  const header = (column: number, label: string) => <th aria-sort={sort().column === column ? (sort().ascending ? "ascending" : "descending") : "none"}><button class="su-th" onClick={() => setSort((s) => ({ column, ascending: s.column === column ? !s.ascending : true }))}>{label}</button></th>;
  const label = (id: string): Project => props.projects.find((p) => p.id === id) ?? { id, name: localById().get(id)?.name ?? id, color: null, accessible: false, is_default: false, palette: 0, folders: [] };
  return <dialog ref={dialog} class="dlg su-dlg project-swarm" style={{ width: "780px", height: "min(680px, calc(100vh - 48px))", position: "fixed", inset: "0", margin: "auto" }} aria-labelledby="project-swarm-title" onCancel={(e) => { e.preventDefault(); void close(); }}>
    <div class="dlg-head"><Icon name="network" size={18} class="accent" /><h2 id="project-swarm-title">Project swarm</h2><span class="grow" /><button class="icon-btn" aria-label="Close" disabled={changing()} onClick={() => void close()}><Icon name="x" size={16} /></button></div>
    <div class="dlg-body sw-body">
      <p class="muted" title={view()?.catalog ? `Swarm ID: ${view()!.catalog!.swarm_id}` : undefined}>Projects are shared between connected computers. SSH settings and credentials stay on this computer. Joining through one member brings the whole project catalog.</p>
      <div class="tabs sw-tabs" role="tablist" aria-label="Project swarm">
        <button id="swarm-computers-tab" class="tab" role="tab" classList={{ on: tab() === "computers" }} aria-selected={tab() === "computers"} aria-controls="swarm-computers" onClick={() => setTab("computers")}>Computers</button>
        <button id="swarm-connect-tab" class="tab" role="tab" classList={{ on: tab() === "connect" }} aria-selected={tab() === "connect"} aria-controls="swarm-connect" onClick={() => setTab("connect")}>Connect computer</button>
        <button class="tab" role="tab" disabled aria-selected="false">Conflicts ({view()?.catalog?.conflicts ?? 0})</button>
      </div>
      <div class="sw-tabbody">
        <Show when={tab() === "computers"}>
          <div id="swarm-computers" class="sw-page" role="tabpanel" aria-labelledby="swarm-computers-tab">
            <div class="row"><input class="field grow" type="search" maxLength={4096} placeholder="Find a computer or connection…" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} /><span class="hint tabular">{filter() ? `${rows().length} of ${computers().length} computers` : `${computers().length} computers`}</span></div>
            <div class="su-tablewrap grow"><table class="su-table sw-table"><thead><tr>{header(0, "Computer")}{header(1, "Local access")}{header(2, "Sync connection")}</tr></thead><tbody>
              <For each={rows().map((r) => r.id)}>{(id) => { const row = () => computerById().get(id)!; return <tr><td title={`${row().name}\n${row().id}`}><MachineLabel alias={row().name} class="mbadge" /></td><td title={row().access} classList={{ dim: row().access === "Not configured" }}>{row().access}</td><td title={syncText(row())} classList={{ danger: row().sync.state === "error", warn: row().tasks_outdated }}>{syncText(row())}</td></tr>; }}</For>
            </tbody></table></div>
            <div class="row"><button class="btn" disabled={changing()} onClick={() => void manage()}>Manage connections…</button><span class="grow" /><button class="btn" disabled={busy() || !view()?.can_sync} onClick={() => void state.sync()}><Icon name="refresh-cw" size={13} />Sync now</button></div>
          </div>
        </Show>
        <Show when={tab() === "connect"}>
          <div id="swarm-connect" class="sw-page" role="tabpanel" aria-labelledby="swarm-connect-tab">
            <div class="row"><select class="field grow" aria-label="Computer to connect" value={alias()} disabled={busy()} onChange={(e) => chooseAlias(e.currentTarget.value)}><For each={view()?.aliases ?? []}>{(a) => <option value={a}>{a}</option>}</For></select><button class="btn" disabled={busy() || !view()?.can_preview || !alias()} onClick={() => void runPreview()}>Preview connection</button></div>
            <div class="sw-preview grow"><Switch>
              <Match when={!busy() && !view()?.aliases.length}><p class="sw-msg">Add a computer in Manage connections first.</p></Match>
              <Match when={loading()}><div class="sw-msg"><p>Reading the other project catalog…</p><div class="su-progress su-indet" role="progressbar" aria-label="Reading the other project catalog"><i /></div></div></Match>
              <Match when={preview()?.initialized && preview()?.same_swarm}><p class="sw-msg su-pre">{`${alias()} already shares this project catalog.\nConnect directly to synchronize through this computer.`}</p></Match>
              <Match when={differs()}><p class="muted">Choose which projects are the same. Projects stay separate unless you match them below.</p><div class="su-tablewrap"><table class="su-table sw-map"><thead><tr><th>Project on this computer</th><th>Project in the other catalog</th></tr></thead><tbody><For each={(view()?.projects ?? []).map((p) => p.id)}>{(id) => <tr><td><ProjectLabel project={label(id)} small /></td><td><select class="field" aria-label={`Match ${localById().get(id)?.name ?? id}`} disabled={busy()} value={mapping()[id] ?? ""} onChange={(e) => setMapping((m) => ({ ...m, [id]: e.currentTarget.value }))}><option value="">Keep as a separate project</option><For each={preview()?.projects ?? []}>{(p) => <option value={p.id}>{p.name} ({p.id.slice(0, 8)})</option>}</For></select></td></tr>}</For></tbody></table></div></Match>
              <Match when={view()?.status === `Connected to ${alias()}.`}><p class="sw-msg">Connected to {alias()}.</p></Match>
              <Match when={true}><p class="sw-msg">Choose a computer and preview its project catalog before connecting.</p></Match>
            </Switch></div>
            <label class="check" style={{ visibility: differs() ? "visible" : "hidden" }}><input type="checkbox" checked={keep()} disabled={busy()} onChange={(e) => setKeep(e.currentTarget.checked)} />Keep swarm placement for sessions already in its catalog</label>
            <div class="row"><span class="grow" /><button class="btn primary" disabled={busy() || !preview()?.initialized || !view()?.can_join} onClick={join}>{differs() ? "Join and merge projects" : "Connect this member"}</button></div>
          </div>
        </Show>
      </div>
    </div>
    <div class="dlg-foot"><span class="hint grow" classList={{ danger: !!state.error() || !!view()?.error }} role="status">{state.error() || view()?.status}</span><button class="btn" disabled={changing()} onClick={() => void close()}>Close</button></div>
  </dialog>;
}
