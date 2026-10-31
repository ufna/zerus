import type { AccountEditView,AccountModelCatalog } from "../../bridge";
import { providerName } from "../lib/usage";

export function permissionExplanation(view:AccountEditView,mode:string) {
  return mode==="provider" ? `Inherit ${providerName(view.provider)} settings on ${view.machine}. ${view.permission_mode==="bypass" ? "" : view.permission_detail??""}`
    : view.provider==="kimi" ? "Run tools and answer questions automatically (Never Ask)."
    : ["codex","dsh"].includes(view.provider) ? "Full access without approval prompts or a sandbox."
    : "Run tools without permission prompts.";
}
/** Account-specific choices; a missing saved model remains visible and cannot be saved. */
export function accountModelChoices(catalog:AccountModelCatalog) {
  const options=catalog.models.slice(0,256);
  const saved=catalog.defaults.model;
  return saved && !options.some(m=>m.id===saved) ? [...options,{id:saved,label:`${saved} (currently unavailable)`,effort_options:[]}] : options;
}
export function accountModelView(catalog:AccountModelCatalog,model:string) {
  const chosen=catalog.models.find(m=>m.id===model);
  return {
    efforts:catalog.effort_supported ? chosen?.effort_options??[] : [],
    effortEnabled:catalog.effort_supported && !!model && !!chosen?.effort_options.length,
    canSave:!model || !!chosen,
    hint:!catalog.effort_supported ? "Kimi manages effort in its native settings. This choice sets the model for new conversations."
      : !catalog.models.length ? "Open this account's agent once, then refresh to load its models."
      : model && !chosen ? "This model is absent from the current account catalog. Choose another model or Provider settings."
      : "Choose the reasoning effort for the selected model, or keep its provider default.",
  };
}
