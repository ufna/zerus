import {onMount,onCleanup} from "solid-js";
import {Icon} from "../../lib/icons";
import {STOP_TITLE,STOP_BODY,type ConnectorPrompt,type ConnectorActions} from "./connector-actions";
export default function ConnectorConfirm(props:{prompt:ConnectorPrompt;actions:ConnectorActions}){
 const view=props.prompt,actions=props.actions,origin=view.origin;let dialog!:HTMLDialogElement,alive=true;
 onMount(()=>{dialog.showModal();queueMicrotask(()=>dialog.querySelector<HTMLButtonElement>(".dlg-foot .btn")?.focus());});
 onCleanup(()=>{alive=false;dialog.close();if(origin.isConnected)origin.focus();});
 return <dialog ref={dialog} class="dlg" style={{width:"460px"}} aria-labelledby="connector-confirm-title" onCancel={e=>{e.preventDefault();actions.dismiss(view.id);}} onClose={()=>{if(alive)actions.dismiss(view.id);}}>
  <div class="dlg-head"><Icon name="triangle-alert" size={18} class="danger"/><h2 id="connector-confirm-title">{STOP_TITLE}</h2><span class="grow"/><button class="icon-btn" aria-label="Close" onClick={()=>actions.dismiss(view.id)}><Icon name="x" size={16}/></button></div>
  <div class="dlg-body"><div class="confirm-body"><p>{STOP_BODY}</p></div></div>
  <div class="dlg-foot"><span class="spacer"/><button class="btn" onClick={()=>actions.dismiss(view.id)}>Cancel</button><button class="btn solid-danger" disabled={actions.busy()} onClick={()=>void actions.confirm(view.id)}>Stop connector</button></div>
 </dialog>;
}
