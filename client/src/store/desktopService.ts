import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { DesktopService } from "../types";

/** Presence of this projection is the startup mode. It never grants writer authority. */
export function createDesktopService() {
  const [state, setState] = createSignal<DesktopService>();
  return {
    state, apply: (next: DesktopService) => setState(next),
    enabled: () => state() !== undefined,
    ready: (component: string) => !!state()?.connected && state()?.background[component]?.ready === true,
  };
}
export const desktopService = createDesktopService();
export const retryDesktopService = () => invoke<void>("desktop_service_retry");

/** Called by the existing opt-in confirmation only. An uncertain request is not replayed. */
export function createServiceApprovalIntent(send: (id: string, enabled: boolean) => Promise<unknown>, id: () => string = () => crypto.randomUUID()) {
  let uncertain: { id: string; enabled: boolean } | undefined;
  return {
    pending: () => uncertain,
    async submit(enabled: boolean) {
      if (uncertain) throw Error("The previous approval setting is unconfirmed. Check the background service before making another change.");
      uncertain = { id: id(), enabled };
      // Keep the UUID after failure. Opening the dialog or confirming again
      // cannot turn an unknown result into another dangerous policy request.
      const result = await send(uncertain.id, enabled) as Record<string, unknown>;
      if (result?.request_id === uncertain.id && result.accepted === false && result.durable === false) {
        uncertain = undefined;
        throw Error("Automatic approval settings are unavailable with this background service connection.");
      }
      if (result?.request_id !== uncertain.id || result.op !== "automatic_approvals" || result.accepted !== true || result.durable !== true || result.enabled !== enabled) {
        throw Error("The background service has not confirmed this approval setting.");
      }
      uncertain = undefined;
    },
  };
}
export const serviceApprovalIntent = createServiceApprovalIntent((request_id, enabled) => invoke("desktop_service_automatic_approvals", { request_id, enabled }));
