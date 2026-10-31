import {createSignal} from "solid-js";
import type {Workspace} from "./mobile-page";
import {workspaceTarget,WORKSPACE_UNKNOWN,WORKSPACE_CHANGED,type WorkspaceTarget} from "./workspace-enrollment";
export type ConnectorKind="start"|"stop";
export interface ConnectorForm{handle:string;target:WorkspaceTarget;kind:ConnectorKind}
export interface ConnectorPrompt{id:number;form:ConnectorForm;origin:HTMLElement}
export interface ConnectorIo{open(target:WorkspaceTarget,kind:ConnectorKind):Promise<ConnectorForm>;apply(handle:string):Promise<{notice:string}>;close(handle:string):Promise<unknown>;refresh():void}
export const canStartConnector=(w?:Workspace|null)=>!!w&&w.encrypted&&w.state==="active"&&!w.pending&&!w.running&&w.connector_state==="stopped";
export const canStopConnector=(w?:Workspace|null)=>!!w&&w.encrypted&&["active","retired"].includes(w.state)&&w.running&&w.connector_state==="running"&&!!w.stop_supported&&!!w.generation;
export const CONNECTOR_DRAINING="The connector is draining its current request. Refresh status to confirm it has stopped.";
export const STOP_TITLE="Stop the mobile connector?";
export const STOP_BODY="Phones can't reach this computer until you start the connector again. A phone request that is already running finishes first. Agents and terminals keep running.";
const key=(target:WorkspaceTarget)=>JSON.stringify(target);
export function createConnectorActions(io:ConnectorIo){
 const [busy,setBusy]=createSignal(false),[notice,setNotice]=createSignal(""),[prompt,setPrompt]=createSignal<ConnectorPrompt>();
 let current:Workspace|undefined|null,disposed=false,generation=0,next=0,admitting:WorkspaceTarget|undefined,draining:WorkspaceTarget|undefined;
 const release=(handle:string)=>void io.close(handle).catch(()=>{});
 const dismiss=(id?:number)=>{const p=prompt();if(id!==undefined&&id!==p?.id)return;setPrompt(undefined);if(p)release(p.form.handle);};
 const run=async(form:ConnectorForm)=>{const version=generation;setBusy(true);setPrompt(undefined);try{const result=await io.apply(form.handle);if(!disposed&&version===generation){draining=result.notice===CONNECTOR_DRAINING?form.target:undefined;setNotice(result.notice);}}catch{if(!disposed&&version===generation){setNotice(WORKSPACE_UNKNOWN);io.refresh();}}finally{release(form.handle);setBusy(false);}};
 return {busy,notice,prompt,dismiss,
  observe(workspace?:Workspace|null){current=workspace;if(draining&&workspace){const same=workspace.config_revision===draining.config_revision&&workspace.node_id===draining.node_id&&workspace.server_url===draining.server_url;if(same&&!workspace.running&&workspace.connector_state==="stopped"){draining=undefined;setNotice("Connector stopped.");}else if(!same||(workspace.running&&workspace.generation!==draining.generation)){draining=undefined;setNotice(WORKSPACE_CHANGED);}}const target=workspace?workspaceTarget(workspace):undefined;if(admitting&&(!target||key(target)!==key(admitting))){generation++;setNotice(WORKSPACE_CHANGED);}const p=prompt();if(p&&(!target||key(target)!==key(p.form.target))){dismiss(p.id);setNotice(WORKSPACE_CHANGED);}},
  async request(kind:ConnectorKind,origin:HTMLElement){if(disposed||busy()||prompt()||!(kind==="start"?canStartConnector(current):canStopConnector(current)))return;const target=workspaceTarget(current!),version=generation;admitting=target;draining=undefined;setBusy(true);setNotice("");try{const form=await io.open(target,kind);if(disposed||version!==generation){release(form.handle);return;}admitting=undefined;if(form.kind!==kind||key(form.target)!==key(target)){release(form.handle);setNotice(WORKSPACE_CHANGED);return;}if(kind==="stop")setPrompt({id:++next,form,origin});else await run(form);}catch{if(!disposed&&version===generation){setNotice(WORKSPACE_UNKNOWN);io.refresh();}}finally{admitting=undefined;setBusy(false);}},
  async confirm(id:number){const p=prompt();if(!p||p.id!==id||busy()||disposed)return;await run(p.form);},
  dispose(){disposed=true;generation++;admitting=undefined;draining=undefined;dismiss();current=undefined;}
 };
}
export type ConnectorActions=ReturnType<typeof createConnectorActions>;
