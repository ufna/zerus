// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("../../../bridge", () => ({ inTauri: () => true }));
import { desktopService } from "../../../store/desktopService";
import { ApprovalsSection } from "./Approvals";
import pageFixture from "./approvals-page.r6.fixture.json";

let dispose: (() => void) | undefined;
beforeEach(() => { try { localStorage.clear(); } catch { /* none */ } });
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
function mount() {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => createComponent(ApprovalsSection, {}), host); return host;
}

// zerus-63rru.66 round 6: the real service reply through the real core page
// (the committed fixture) rendered by the actual Approvals component: every
// trust outcome shows its exact row sentence, both resume forms show none,
// and the ordinary category names are unchanged. The reviewer's selector
// mutant must turn this red.
it("shows each trust outcome's exact sentence and none for the resume rows", async () => {
  native.invoke.mockImplementation(async (command: string, args: Record<string, unknown>) => {
    return { ...pageFixture, next: 0, previous: 0 };
  });
  desktopService.apply({ connected: true, epoch: "fixture", generation: 1, background: {},
    attention_counts: null, automatic_approvals: true, details_available: true, code: "" } as never);
  const host = mount();
  await vi.waitFor(() => expect(host.querySelectorAll(".an-table tbody tr, .team-stop-chip, tr").length).toBeGreaterThan(0));
  const text = host.textContent ?? "";
  expect(text).toContain("Trusted for /fixture/main and its subfolders on this account");
  expect(text.match(/Trust was requested for \/fixture\/main and its subfolders; it was not confirmed/g)?.length).toBe(4);
  expect(text).not.toContain("Trusted for /fixture/main and its subfolders on this account. Trust was requested");
  // The two resume rows (a new row and an old row with a scope) show no trust sentence.
  const trusted = (text.match(/Trusted for /g) ?? []).length;
  const requested = (text.match(/Trust was requested for /g) ?? []).length;
  expect(trusted).toBe(1);
  expect(requested).toBe(4);
});
