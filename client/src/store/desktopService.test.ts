import { describe, expect, it, vi } from "vitest";
import { createDesktopService, createServiceApprovalIntent } from "./desktopService";
import { createApprovalOptIn } from "../ui/screens/settings/approval-opt-in";
import { emptyBatch, isEmptyBatch, type DesktopService } from "../types";
import { normalize } from "../bridge";

export const serviceView = (patch: Partial<DesktopService> = {}): DesktopService => ({
  connected: true, epoch: "epoch", generation: 1, background: {}, attention_counts: null,
  automatic_approvals: null, details_available: false, code: "background_unready", ...patch,
});
describe("desktop service presentation", () => {
  it("is off until the native startup mode publishes a projection; IPC alone enables no component", () => {
    const service = createDesktopService();
    expect(service.enabled()).toBe(false);
    expect(service.ready("reads")).toBe(false);
    service.apply(serviceView());
    expect(service.enabled()).toBe(true);
    expect(service.ready("reads")).toBe(false);
    service.apply(serviceView({ background: { reads: { ready: true, code: "ready" } } }));
    expect(service.ready("reads")).toBe(true);
    expect(service.ready("approvals")).toBe(false);
    service.apply(serviceView({ connected: false }));
    expect(service.enabled()).toBe(true);
    expect(service.ready("reads")).toBe(false);
  });
  it("delivers service-only batches and preserves incomplete/null count provenance", () => {
    const projection = serviceView({ attention_counts: { epoch: "epoch", scope: "shared_host_state", display_only: true, authority: false, local_marks_included: false,
      aggregate: { complete: false, attention: null, total: null, known_attention: 0, known_total: 3 }, computers: [] } });
    const batch = normalize({ ...emptyBatch(), desktop_service: projection });
    expect(isEmptyBatch(batch)).toBe(false);
    expect(batch.desktop_service).toEqual(projection);
    expect(batch.desktop_service?.attention_counts?.aggregate.attention).toBeNull();
  });
});
describe("service approval intents", () => {
  const origin = {} as HTMLElement;
  it("uses the existing enable confirmation before its native seam and never writes a setting", async () => {
    const send = vi.fn(async (request_id: string, enabled: boolean) => ({ request_id, enabled, op: "automatic_approvals", accepted: true, durable: true }));
    const intent = createServiceApprovalIntent(send, () => "request-1");
    const confirmation = createApprovalOptIn((_key, enabled) => intent.submit(enabled), vi.fn());
    confirmation.request(true, origin);
    expect(send).not.toHaveBeenCalled();
    confirmation.cancel(); await confirmation.confirm();
    expect(send).not.toHaveBeenCalled();
    confirmation.request(true, origin); await confirmation.confirm();
    expect(send).toHaveBeenCalledExactlyOnceWith("request-1", true);
  });
  it("retains an uncertain UUID and refuses a replay, including another confirm", async () => {
    const send = vi.fn(async () => { throw Error("transport closed"); });
    const intent = createServiceApprovalIntent(send, () => "request-1");
    await expect(intent.submit(true)).rejects.toThrow("transport closed");
    expect(intent.pending()).toEqual({ id: "request-1", enabled: true });
    await expect(intent.submit(true)).rejects.toThrow("unconfirmed");
    await expect(intent.submit(false)).rejects.toThrow("unconfirmed");
    expect(send).toHaveBeenCalledOnce();
  });
  it("rejects non-durable or mismatched receipts and permits a fresh request after a known refusal", async () => {
    for (const reply of [{ request_id: "request-1", accepted: true, durable: false }, { request_id: "other", accepted: true, durable: true }]) {
      const intent = createServiceApprovalIntent(async () => reply, () => "request-1");
      await expect(intent.submit(true)).rejects.toThrow("not confirmed");
      expect(intent.pending()?.id).toBe("request-1");
    }
    const send = vi.fn(async (request_id: string) => ({ request_id, accepted: false, durable: false }));
    const intent = createServiceApprovalIntent(send, () => "request-1");
    await expect(intent.submit(true)).rejects.toThrow("unavailable");
    expect(intent.pending()).toBeUndefined();
  });
});
