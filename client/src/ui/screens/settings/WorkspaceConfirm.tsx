import {onMount,onCleanup} from "solid-js";
import {Icon} from "../../lib/icons";
import {enrollmentLabel,type WorkspacePrompt,type WorkspaceEnrollment} from "./workspace-enrollment";
import { tr } from "../../../i18n";
export default function WorkspaceConfirm(props:{prompt:WorkspacePrompt;actions:WorkspaceEnrollment}){
 const view=props.prompt,actions=props.actions,origin=view.origin;let dialog!:HTMLDialogElement,alive=true;
 const local=view.form.legacy_local;
 onMount(()=>{dialog.showModal();queueMicrotask(()=>dialog.querySelector<HTMLButtonElement>(".dlg-foot .btn")?.focus());});
 onCleanup(()=>{alive=false;dialog.close();if(origin.isConnected)origin.focus();});
 return <dialog ref={dialog} class="dlg" style={{width:"460px"}} aria-labelledby="workspace-confirm-title" onCancel={e=>{e.preventDefault();actions.dismiss(view.id);}} onClose={()=>{if(alive)actions.dismiss(view.id);}}>
  <div class="dlg-head"><Icon name="triangle-alert" size={18} class="accent"/><h2 id="workspace-confirm-title">{local?"Disconnect legacy locally":"Change workspace"}</h2><span class="grow"/><button class="icon-btn" aria-label={tr("Close")} onClick={()=>actions.dismiss(view.id)}><Icon name="x" size={16}/></button></div>
  <div class="dlg-body"><div class="confirm-body"><p>{local?"The old relay has not confirmed retirement. Its old records and credentials may remain active on that relay. This only disconnects the saved legacy configuration on this computer and switches to the new encrypted workspace. Local agents, projects and conversations stay on this computer.":"This computer will leave its current workspace. Phone access through this computer will end. Local agents, projects and saved conversations stay on this computer."}</p></div></div>
  <div class="dlg-foot"><span class="spacer"/><button class="btn" onClick={()=>actions.dismiss(view.id)}>{tr("Cancel")}</button><button class="btn solid-danger" disabled={actions.busy()} onClick={()=>void actions.confirm(view.id)}>{local?"Disconnect legacy locally and switch":enrollmentLabel(view.form.mode!,true)}</button></div>
 </dialog>;
}
