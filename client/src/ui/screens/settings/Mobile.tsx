import { For, Show, createSignal, onCleanup, onMount, createMemo, lazy, Suspense } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "../../../bridge";
import { Icon } from "../../lib/icons";
import { Actions, Group, Lines, SectionHead, Select, Status } from "./ui";
import {createPhoneActions,targetOf,phoneKey,targetKey,type PhoneActions} from "./phone-actions";
import { createMobile, type MobileState, type View } from "./mobile-page";

// Settings > Mobile connection (WorkspaceSettings.h, PhonePairing.cpp,
// MobileDeliverySettings.h, SettingsPage.h:124-139) in the approved
// mockup's layout. Reads and notification timing stay in the existing hub;
// phone actions use one-use native plans and the workspace/relay shown here.
// Remaining workspace/connector controls belong to .24.5 through .24.8.

const DELIVERY: { value: string; label: string }[] = [
  { value: "immediate", label: "Immediately, even if I'm at a computer" },
  { value: "away", label: "As soon as I'm away" },
  { value: "away_5", label: "After 5 minutes away" },
  { value: "away_10", label: "After 10 minutes away" },
  { value: "away_15", label: "After 15 minutes away" },
  { value: "away_30", label: "After 30 minutes away" },
];

/** QLocale's short date and time, or a dash when unknown. */
const when = (seconds: number | null) =>
  seconds ? new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" }) : "—";

/** A workspace's heading: where its phones were read from. */
export const listTitle = (list: { host: string; workspace_name: string }) =>
  list.host ? `${list.workspace_name} (via ${list.host})` : `${list.workspace_name} (this computer)`;

function WorkspacePanel(props: { view?: View; refresh: () => void }) {
  const read = () => props.view?.workspace ?? undefined;
  const busy = () => !!props.view?.workspace_busy;
  const controls = () => read()?.controls;
  return (
    <Group title="Workspace">
      <p class="set-group-hint">Connect your computers and phones to one workspace. Session content is encrypted between your devices; the relay keeps routing and delivery metadata.</p>
      <Show when={read()} fallback={
        <Show when={props.view?.workspace_error} fallback={<Status kind="busy">Workspace: checking…</Status>}>
          <Lines text="Workspace status is unconfirmed" />
        </Show>
      }>
        {(workspace) => (
          <>
            <Lines text={workspace().summary} />
            <span class="muted">{workspace().connection}</span>
            <Show when={workspace().pending}>{(text) => <Status kind="warn">{text()}</Status>}</Show>
          </>
        )}
      </Show>
      <Show when={props.view?.workspace_error}>{(text) => <Status kind="error">{text()}</Status>}</Show>
      <Actions>
        <Show when={controls()?.change}><button type="button" class="btn" disabled>Change workspace…</button></Show>
        <Show when={controls()?.enrollment}><button type="button" class="btn" disabled>Create workspace</button></Show>
        <Show when={controls()?.start}><button type="button" class="btn" disabled><Icon name="play" size={13} />Start connector</button></Show>
        <Show when={controls()?.stop}><button type="button" class="btn" disabled><Icon name="square" size={12} />Stop connector</button></Show>
        <button type="button" class="btn" disabled={busy() || !props.view} onClick={props.refresh}><Icon name="refresh-cw" size={13} />Refresh status</button>
      </Actions>
      <Show when={controls()?.check}>{(text) => <Actions><button type="button" class="btn" disabled>{text()}</button></Actions>}</Show>
      <Show when={controls()?.invitations}>
        <Actions>
          <button type="button" class="btn" disabled><Icon name="key-round" size={13} />Give computer code</button>
          <button type="button" class="btn" disabled><Icon name="copy" size={13} />Copy computer code</button>
          <button type="button" class="btn" disabled>Cancel code</button>
        </Actions>
      </Show>
      <Actions>
        <button type="button" class="btn ghost" disabled><Icon name="chevron-right" size={13} />Advanced: legacy connector configuration</button>
      </Actions>
      <p class="set-group-hint">Changing the workspace, starting or stopping the connector, checking a saved connection, computer invitation codes and the legacy connector configuration are not available in this client yet.</p>
    </Group>
  );
}

const PhoneDialogs=lazy(()=>import("./PhoneDialogs"));
function PhonesPanel(props: { view?: View; refresh: () => void; actions:PhoneActions }) {
  const [selected,setSelected]=createSignal<string>();
  const phones=()=>props.view?.phones, reading=()=>!!props.view?.phones_busy, busy=()=>reading()||props.actions.busy();
  const entries=createMemo(()=>phones()?.lists.flatMap(list=>list.phones.map(phone=>({list,phone,key:phoneKey(targetOf(list),phone.id)})))??[]);
  const chosen=()=>entries().find(e=>e.key===selected());
  const revoke=(origin:HTMLElement)=>{const entry=chosen();if(entry)void props.actions.openRevoke(targetOf(entry.list),entry.phone.id,origin);};
  return <section class="set-group set-phones"><div class="set-group-head"><h3>Phones</h3></div><p class="set-group-hint">Phones paired with your computers. A phone can control every computer of its workspace; revoke one you no longer use.</p>
   <div class="set-table-wrap"><table class="set-table" aria-label="Phones"><thead><tr><th scope="col">Phone</th><th scope="col">Platform</th><th scope="col">Paired</th><th scope="col">Last active</th><th scope="col">Added from</th></tr></thead><tbody>
    <Show when={!reading()&&entries().length} fallback={<tr class="set-table-empty"><td colspan={5}>{reading()?"":"No phones"}</td></tr>}><For each={phones()?.lists??[]}>{list=><><tr class="set-table-group"><td colspan={5}>{listTitle(list)}</td></tr><For each={list.phones}>{phone=>{const key=phoneKey(targetOf(list),phone.id);return <tr classList={{on:selected()===key}} tabIndex={0} aria-selected={selected()===key} onClick={()=>setSelected(key)} onKeyDown={e=>{if(e.key==="Enter"||e.key===" "){e.preventDefault();setSelected(key);}if(e.key==="Delete"){setSelected(key);revoke(e.currentTarget);}}}><td><span class="row"><Icon name="smartphone" size={13} class="dim"/>{phone.name}</span></td><td>{phone.platform}</td><td class="tabular">{when(phone.paired_at)}</td><td class="tabular">{when(phone.last_seen_at)}</td><td title={list.encrypted?"Verified device membership. Private device names are not stored by the relay.":"Paired with an administrator invitation, or before Zerus recorded it"}>{phone.added_from??<span class="dim">—</span>}</td></tr>;}}</For></>}</For></Show>
   </tbody></table></div>
   <Actions><button class="btn" disabled={busy()||!phones()?.lists.length} onClick={e=>{const targets=phones()!.lists.map(targetOf);const selected=chosen();const at=selected?targets.findIndex(t=>targetKey(t)===targetKey(targetOf(selected.list))):0;props.actions.pair(targets,at,e.currentTarget);}}><Icon name="plus" size={14}/>Pair a phone…</button><button class="btn danger" disabled={busy()||!chosen()} onClick={e=>revoke(e.currentTarget)}>Revoke…</button><button class="btn" disabled={busy()||!props.view} onClick={props.refresh}><Icon name="refresh-cw" size={13}/>Refresh</button><Status kind={reading()?"busy":"muted"}>{reading()?"Checking phones…":phones()?.lines.join("\n")}</Status></Actions>
   <Show when={props.actions.notice()}><Status>{props.actions.notice()}</Status></Show>
  </section>;
}

function DeliveryPanel(props: { view?: View; set: (value: string) => void }) {
  const value = () => props.view?.delivery ?? "";
  const busy = () => !!props.view?.delivery_busy;
  // Qt disables the choice while the shared setting could not be read.
  const usable = () => !busy() && !props.view?.delivery_failed && !!props.view?.delivery;
  // Made again when the value or its call changes, so a choice that was not
  // saved never stays shown.
  const shown = () => `${value()}|${usable()}`;
  return (
    <Group title="Phone notifications">
      <label class="set-label" for="mobile-delivery">When I'm not at a computer, send notifications to my phone</label>
      <Show when={shown()} keyed>
        <Select id="mobile-delivery" label="When I'm not at a computer, send notifications to my phone" value={value()}
          disabled={!usable()} onChange={props.set} options={DELIVERY} />
      </Show>
      <Show when={props.view?.delivery_status}>{(text) => <Status kind={busy() ? "busy" : "muted"}>{text()}</Status>}</Show>
      <Show when={props.view?.delivery_outdated}>{(text) => <Status kind="warn">{text()}</Status>}</Show>
      <Show when={props.view?.delivery_worker}>{(text) => <Status kind="warn">{text()}</Status>}</Show>
      <p class="set-group-hint">You count as away after two minutes without keyboard or mouse input on every computer running Zerus, or while their screens are locked. Your computers and the phone share this choice.</p>
    </Group>
  );
}

export function MobileSection() {
  const [state, setState] = createSignal<MobileState>({});
  const page = createMobile({
    view: () => invoke<View>("mobile_view"),
    open: () => invoke<View>("mobile_open"),
    refreshWorkspace: () => invoke<View>("mobile_refresh_workspace"),
    refreshPhones: () => invoke<View>("mobile_refresh_phones"),
    setDelivery: (value) => invoke<View>("mobile_delivery_set", { value }),
  }, setState);
  const phoneActions=createPhoneActions({
    open:(kind,target,phone)=>invoke("mobile_phone_open",{kind,target,phone:phone??null}),
    invite:handle=>invoke("mobile_phone_invite",{handle}),revoke:handle=>invoke("mobile_phone_revoke",{handle}),close:handle=>invoke("mobile_phone_close",{handle}),changed:()=>page.changed(),
  });
  onCleanup(()=>phoneActions.close());
  onMount(() => {
    if (!inTauri()) return;
    let gone = false;
    let stop: (() => void) | undefined;
    // Read again once listening: an answer in between would otherwise wait.
    listen("mobile-changed", () => page.changed())
      .then((off) => { if (gone) off(); else { stop = off; page.changed(); } }, () => {});
    page.open();
    // The 5-second swarm read keeps the timing and its notices current.
    const timer = setInterval(() => page.changed(), 5000);
    onCleanup(() => { gone = true; stop?.(); clearInterval(timer); });
  });
  const view = () => state().view;
  return (
    <>
      <SectionHead title="Mobile connection" />
      <Show when={state().error}>{(text) => <Status kind="error">{text()}</Status>}</Show>
      <WorkspacePanel view={view()} refresh={() => page.refreshWorkspace()} />
      <PhonesPanel view={view()} refresh={() => page.refreshPhones()} actions={phoneActions}/>
      <Show when={phoneActions.prompt()}>{prompt=><Suspense><PhoneDialogs prompt={prompt()} actions={phoneActions}/></Suspense>}</Show>
      <DeliveryPanel view={view()} set={(value) => page.setDelivery(value)} />
    </>
  );
}
