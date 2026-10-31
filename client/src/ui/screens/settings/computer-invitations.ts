import {createSignal} from "solid-js";
import type {Workspace} from "./mobile-page";
export interface ComputerTarget{config_revision:string;server_url:string}
/** Private, transient credential. Never settings, notices or the shared page. */
export interface ComputerCode{handle:string;code:string;expires_at:number;target:ComputerTarget}
export interface ComputerCodeIo{
 open(target:ComputerTarget):Promise<string>;issue(handle:string):Promise<ComputerCode>;
 close(handle:string):Promise<unknown>;cancel(handle:string):Promise<unknown>;check(handle:string):Promise<unknown>;
 copy(text:string):Promise<unknown>;refresh():void;
}
export const COMPUTER_UNCONFIRMED="Workspace operation was not confirmed. Refresh status before continuing. Verify that the matching computer connector is installed.";
const key=(t:ComputerTarget)=>JSON.stringify([t.config_revision,t.server_url]);
export function createComputerCodes(io:ComputerCodeIo,now:()=>number=()=>Date.now()/1000){
 const [code,setCode]=createSignal<ComputerCode>(),[busy,setBusy]=createSignal(false),[status,setStatus]=createSignal("");
 let generation=0,target:ComputerTarget|undefined,disposed=false;
 const forget=()=>{const current=code();setCode(undefined);if(current)void io.close(current.handle).catch(()=>{});};
 const unknown=()=>{forget();setStatus(COMPUTER_UNCONFIRMED);io.refresh();};
 const tick=()=>{if(code()&&code()!.expires_at<=now()){forget();setStatus("Computer code expired.");}};
 return {code,busy,status,
  observe(workspace?:Workspace|null){const next=workspace?.state==="active"&&workspace.encrypted&&!workspace.pending?{config_revision:workspace.config_revision,server_url:workspace.server_url}:undefined;if((target&&next&&key(target)===key(next))||(!target&&!next))return;generation++;target=next;forget();if(status()!==COMPUTER_UNCONFIRMED)setStatus("");},
  tick,
  async issue(){if(disposed||busy()||!target||code())return;const current=generation,captured=target;let handle:string|undefined;setBusy(true);setStatus("");try{handle=await io.open(captured);if(disposed||current!==generation)return;const value=await io.issue(handle);if(disposed||current!==generation){void io.close(value.handle).catch(()=>{});return;}setCode(value);const date=new Date(value.expires_at*1000),time=`${String(date.getHours()).padStart(2,"0")}:${String(date.getMinutes()).padStart(2,"0")}`;setStatus(`Computer code ready until ${time}. Keep this computer online while the other computer joins.`);tick();}catch{if(!disposed&&current===generation)unknown();}finally{if(handle)void io.close(handle).catch(()=>{});setBusy(false);}},
  async copy(){const value=code();if(!value||value.expires_at<=now()||busy()||disposed)return false;try{await io.copy(value.code);return !disposed&&code()===value;}catch{if(!disposed)setStatus("Could not copy the computer code.");return false;}},
  async cancel(){const value=code();if(!value||busy()||disposed)return;const current=generation;setBusy(true);setCode(undefined);try{await io.cancel(value.handle);if(!disposed&&current===generation)setStatus("Code canceled. Computers already joined keep their access.");}catch{if(!disposed&&current===generation)unknown();}finally{void io.close(value.handle).catch(()=>{});setBusy(false);}},
  async check(){const value=code();if(!value||busy()||disposed)return;const current=generation;setBusy(true);try{await io.check(value.handle);}catch{if(!disposed&&current===generation)unknown();}finally{setBusy(false);}},
  dispose(){disposed=true;generation++;forget();target=undefined;},
 };
}
