import {createSignal} from "solid-js";
export const LEGACY_UNKNOWN="Configuration was not confirmed saved. Reload to check; no connector restart was requested.";
export const WINDOWS_DRAINING="Stop requested. The current phone request will finish before the connector exits. Reload status to confirm.";
export const OFFICIAL_RELAY="https://relay.zerus.dev";
export interface LegacyForm{handle:string;windows:boolean;configured:boolean;configured_relay:string;config:string;relay:string;directory:string;new_directory:string;status:string;implementation:string;protection:string;commands:[string,string][];running:boolean}
export interface LegacyFields{relay:string;replacement:string;directory:string}
export interface LegacyIo{open():Promise<LegacyForm>;apply(handle:string,request:{action:string;edit:LegacyFields}):Promise<{notice:string}>;close(handle:string):Promise<unknown>}
export function createLegacyRelay(io:LegacyIo){
 const [form,setForm]=createSignal<LegacyForm>(),[fields,setFields]=createSignal<LegacyFields>({relay:OFFICIAL_RELAY,replacement:"",directory:""}),[busy,setBusy]=createSignal(false),[notice,setNotice]=createSignal(""),[handle,setHandle]=createSignal<string>();
 let disposed=false,generation=0;
 const release=(h?:string)=>{if(h)void io.close(h).catch(()=>{});};
 const load=async()=>{const version=++generation;release(handle());setHandle(undefined);setFields(f=>({...f,replacement:""}));try{const f=await io.open();if(disposed||version!==generation){release(f.handle);return;}setHandle(f.handle);setForm(f);setFields({relay:f.relay,replacement:"",directory:f.directory});}catch(e){if(!disposed&&version===generation){setNotice(typeof e==="string"?e:LEGACY_UNKNOWN);}}};
 return{form,fields,setFields,busy,notice,ready:()=>!busy()&&!!handle(),
  async reload(){if(busy()||disposed)return;setBusy(true);setNotice("");try{await load();}finally{setBusy(false);}},
  async apply(action:string){if(busy()||disposed||!handle())return;const h=handle()!;setHandle(undefined);const edit=fields(),version=generation;setFields(f=>({...f,replacement:""}));setBusy(true);setNotice("");try{const result=await io.apply(h,{action,edit});if(!disposed&&version===generation){if(result.notice!==WINDOWS_DRAINING)await load();setNotice(result.notice);}}catch(e){if(!disposed&&version===generation)setNotice(typeof e==="string"?e:LEGACY_UNKNOWN);}finally{release(h);setBusy(false);}},
  dispose(){disposed=true;generation++;release(handle());setHandle(undefined);setForm(undefined);setFields({relay:OFFICIAL_RELAY,replacement:"",directory:""});}
 };
}
