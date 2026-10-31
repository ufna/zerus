import {For,Show,createEffect,createMemo,createSignal,on,onMount,onCleanup} from "solid-js";
import {createVirtualizer} from "@tanstack/solid-virtual";
import {Icon} from "../lib/icons";
import {MachineTile} from "../components/MachineTile";
import {MachineLabel} from "../components/MachineLabel";
import {osOf} from "../../store/overview";
import {accountDistribution,signInDistributedProfile,type DistributionPrompt} from "../../store/accountDistribution";
import type {AccountDistributionValues,AccountDistributionState} from "../../bridge";
const slug=(s:string)=>s.toLowerCase().replace(/[^a-z0-9]+/g,"-").replace(/^-|-$/g,"").slice(0,48)||"account";
const RANK:Record<AccountDistributionState,number>={failed:0,unknown:1,"needs-signin":2,working:3,pending:4,added:5};
/** Approved AddToMachine.tsx layout, driven by a bounded native snapshot. */
export default function AccountDistributionDialog(props:{prompt:DistributionPrompt}) {
 const view=props.prompt,state=accountDistribution;
 const [step,setStep]=createSignal<1|2|3>(1),[query,setQuery]=createSignal(""),[picked,setPicked]=createSignal<string[]>([]),[availableOnly,setAvailableOnly]=createSignal(false),[method,setMethod]=createSignal<AccountDistributionValues["method"]>(view.provider==="dsh"?"apikey":view.sources.length?"copy":"signin"),[useSaved,setUseSaved]=createSignal(true),[source,setSource]=createSignal(view.sources[0]?.key??""),[name,setName]=createSignal(view.label),[profileId,setProfileId]=createSignal(view.provider==="dsh"?"native-dsh":slug(`${view.provider}-${view.label}`)),[apiKey,setApiKey]=createSignal("");
 const blocked=(host:string)=>view.machines.find(m=>m.key===host)?.blocked;
 const shown=createMemo(()=>{const q=query().trim().toLowerCase();return view.machines.filter(m=>(!q||`${m.machine} ${m.platform}`.toLowerCase().includes(q))&&(!availableOnly()||!m.blocked));});
 const selectable=()=>shown().filter(m=>!m.blocked),toggle=(host:string)=>setPicked(rows=>rows.includes(host)?rows.filter(h=>h!==host):[...rows,host]);
 const savedCount=()=>view.machines.filter(m=>picked().includes(m.key)&&m.saved).length;
 const idValid=()=>/^[a-z0-9][a-z0-9-]{0,47}$/.test(profileId());
 const canAdd=()=>picked().length>0&&!!name().trim()&&idValid()&&(method()!=="apikey"||!!apiKey().trim())&&(method()!=="copy"||!!source()||useSaved()&&savedCount()===picked().length);
 const values=():AccountDistributionValues=>({hosts:picked(),method:method(),use_saved:useSaved(),source:source(),label:name().trim(),profile_id:profileId(),api_key:apiKey().trim()});
 const start=()=>{if(!canAdd())return;setStep(3);void state.start(values(),view.handle);};
 const finished=()=>!state.busy()&&state.progress().length>0&&state.progress().every(p=>p.state!=="pending"&&p.state!=="working");
 const count=(s:AccountDistributionState)=>state.progress().filter(p=>p.state===s).length;
 const ordered=()=>finished()?[...state.progress()].sort((a,b)=>RANK[a.state]-RANK[b.state]):state.progress();
 let dialog!:HTMLDialogElement;let scroller:HTMLDivElement|undefined;
 const virtualizer=createVirtualizer({get count(){return shown().length;},getScrollElement:()=>scroller??null,estimateSize:()=>48,overscan:6});
 createEffect(on(()=>shown().length,()=>{scroller?.scrollTo({top:0});virtualizer.measure();},{defer:true}));
 onMount(()=>{dialog.showModal();queueMicrotask(()=>dialog.querySelector<HTMLInputElement>("input")?.focus());});
 onCleanup(()=>{setApiKey("");dialog.close();if(view.origin.isConnected)view.origin.focus();});
 return <dialog ref={dialog} class="dlg" style={{width:"600px"}} aria-labelledby="accounts-add-to-machine-title" onCancel={e=>{e.preventDefault();state.close(view.handle);}} onClose={()=>state.close(view.handle)}>
  <div class="dlg-head"><Icon name="user-round-cog" size={18} class="accent"/><h2 id="accounts-add-to-machine-title">Add {view.label} to machines</h2><span class="grow"/><button type="button" class="icon-btn" aria-label="Close" onClick={()=>state.close(view.handle)}><Icon name="x" size={16}/></button></div>
  <div class="dlg-body">
   <ol class="atm-steps" aria-label="Steps"><For each={["Machines","How to add","Add"]}>{(label,i)=><li classList={{on:step()===i()+1,done:step()>i()+1}}><b>{i()+1}</b>{label}</li>}</For></ol>
   <Show when={step()===1}>
    <div class="atm-tools"><input class="field atm-search" type="search" placeholder={`Search ${view.machines.length} machines`} aria-label="Search machines" value={query()} onInput={e=>setQuery(e.currentTarget.value)}/><label class="check"><input type="checkbox" checked={availableOnly()} onChange={e=>setAvailableOnly(e.currentTarget.checked)}/>Available only</label><button class="btn small ghost" disabled={!selectable().length} onClick={()=>setPicked([...new Set([...picked(),...selectable().map(m=>m.key)])])}>Select all available</button></div>
    <div class="atm-list" role="listbox" aria-multiselectable="true" aria-label="Machines" ref={el=>{scroller=el;requestAnimationFrame(()=>virtualizer._willUpdate());}}>
     <Show when={shown().length} fallback={<div class="atm-empty dim">No machine matches “{query()}”.</div>}><div style={{height:`${virtualizer.getTotalSize()}px`,position:"relative"}}><For each={virtualizer.getVirtualItems()}>{item=>{const m=()=>shown()[item.index];return <Show when={m()}><div class="atm-row" role="option" tabIndex={m().blocked?-1:0} aria-selected={picked().includes(m().key)} aria-disabled={!!m().blocked} classList={{on:picked().includes(m().key),blocked:!!m().blocked,has:m().present}} style={{transform:`translateY(${item.start}px)`}} onClick={()=>{if(!blocked(m().key))toggle(m().key);}} onKeyDown={e=>{if((e.key==="Enter"||e.key===" ")&&!blocked(m().key)){e.preventDefault();toggle(m().key);}}}>
      <span class="atm-check" aria-hidden="true"><Show when={m().present||picked().includes(m().key)}><Icon name="check" size={12}/></Show></span><MachineTile os={osOf(m().platform)} state={m().blocked==="Not connected"?"offline":"online"} size={28}/><span class="atm-name"><MachineLabel alias={m().machine}/><Show when={m().version}><span class="dim tabular">hgs {m().version}</span></Show></span><span class="atm-why" classList={{ok:!m().blocked}}>{m().blocked??(m().saved?"Saved sign-in available":"")}</span>
     </div></Show>;}}</For></div></Show>
    </div>
   </Show>
   <Show when={step()===2}>
    <p>{view.label} will get a separate profile on {picked().length===1?view.machines.find(m=>m.key===picked()[0])?.machine:`${picked().length} machines`}. Other accounts and sessions there stay as they are; after sign-in, the provider identity decides which account a profile belongs to.</p>
    <Show when={method()==="apikey"} fallback={<div class="col atm-how" role="radiogroup" aria-label="How to add">
     <Show when={savedCount()}><label class="check"><input type="checkbox" checked={useSaved()} onChange={e=>setUseSaved(e.currentTarget.checked)}/>Use the saved sign-in where a machine already has one ({savedCount()} of {picked().length})</label></Show>
     <label class="check"><input type="radio" name="atm-how" checked={method()==="copy"} disabled={!view.sources.length} onChange={()=>setMethod("copy")}/>Copy the existing sign-in over SSH from<select class="field atm-src" value={source()} disabled={method()!=="copy"||!view.sources.length} onChange={e=>setSource(e.currentTarget.value)}><For each={view.sources}>{s=><option value={s.key}>{s.label}</option>}</For></select></label>
     <label class="check"><input type="radio" name="atm-how" checked={method()==="signin"} onChange={()=>setMethod("signin")}/>Create the profile and sign in on each machine (a terminal opens there)</label>
    </div>}><div class="form-row"><label for="atm-key">DeepSeek API key</label><input id="atm-key" class="field" type="password" autocomplete="off" maxLength={4096} value={apiKey()} onInput={e=>setApiKey(e.currentTarget.value)}/></div><p class="hint">The same key is saved on every selected machine.</p></Show>
    <div class="form-row"><label for="atm-name">Account name</label><input id="atm-name" class="field" maxLength={160} value={name()} onInput={e=>setName(e.currentTarget.value)}/></div>
    <div class="form-row"><label for="atm-id">Profile ID</label><input id="atm-id" class="field mono" maxLength={48} disabled={view.provider==="dsh"} classList={{invalid:!idValid()}} value={profileId()} onInput={e=>setProfileId(e.currentTarget.value)}/></div>
    <Show when={!idValid()}><p class="hint warn">Use lowercase letters, digits and hyphens, starting with a letter or digit.</p></Show>
   </Show>
   <Show when={step()===3}>
    <p class="atm-summary"><Show when={finished()} fallback={<>Adding {view.label} to {state.progress().length} machine{state.progress().length===1?"":"s"}…</>}>{count("added")} added{count("needs-signin")?`, ${count("needs-signin")} need sign-in`:""}{count("failed")?`, ${count("failed")} failed`:""}{count("unknown")?`, ${count("unknown")} unknown`:""}.</Show></p>
    <div class="atm-results"><For each={ordered()}>{p=><div class={`atm-result r-${p.state}`}><span class="atm-ricon" aria-hidden="true"><Show when={p.state==="working"} fallback={<Icon name={p.state==="added"?"check":p.state==="needs-signin"?"key-round":p.state==="failed"?"x":p.state==="unknown"?"circle-alert":"clock"} size={14}/>}><i class="atm-spin"/></Show></span><MachineLabel alias={p.machine}/><span class="atm-rtext">{p.text}</span><Show when={p.state==="failed"}><button class="btn small" disabled={state.busy()} onClick={()=>void state.retry(p.host,values())}>Retry</button></Show><Show when={p.state==="needs-signin"&&p.profile}><button class="btn small" disabled={state.busy()} onClick={e=>void signInDistributedProfile(p.profile!,e.currentTarget)}>Sign in…</button></Show></div>}</For></div>
    <Show when={count("unknown")}><p class="hint">An unknown result is never retried automatically: the copy may have finished. Open the machine in Accounts to check it.</p></Show>
   </Show>
  </div>
  <div class="dlg-foot"><Show when={step()===1}><span class="dim atm-count">{picked().length?`${picked().length} selected`:"Choose one or more machines"}</span></Show><span class="spacer"/><Show when={step()!==3} fallback={<button class="btn primary" disabled={!finished()} onClick={()=>state.done(view.handle)}>Done</button>}><Show when={step()===2} fallback={<button class="btn" onClick={()=>state.close(view.handle)}>Cancel</button>}><button class="btn" onClick={()=>setStep(1)}>Back</button></Show><Show when={step()===1} fallback={<button class="btn primary" disabled={!canAdd()} onClick={start}>Add to {picked().length} machine{picked().length===1?"":"s"}</button>}><button class="btn primary" disabled={!picked().length} onClick={()=>setStep(2)}>Next</button></Show></Show></div>
 </dialog>;
}
