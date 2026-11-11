// @vitest-environment happy-dom
// Settings > Agent Teams (preview): the row's states, the switch, the
// confirmation for turning it off with a live team, a refusal and an unknown
// result.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
const f = vi.hoisted(() => ({
  setAvailable: (_: boolean | undefined) => {}, setOff: (_: "inactive" | "unavailable" | null) => {}, setList: (_: unknown) => {},
  preview: vi.fn(async (): Promise<{ outcome: string; detail?: string }> => ({ outcome: "done" })), recheck: vi.fn(),
}));
vi.mock("../../../store/teams", async () => {
  const { createSignal } = await import("solid-js");
  const [available, setAvailable] = createSignal<boolean | undefined>(undefined);
  const [off, setOff] = createSignal<"inactive" | "unavailable" | null>(null);
  const [list, setList] = createSignal<unknown>(undefined);
  f.setAvailable = setAvailable; f.setOff = setOff; f.setList = setList;
  return { teams: { available, off, list, preview: f.preview, recheck: f.recheck } };
});
import { TeamsSection, previewState } from "./Teams";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; f.preview.mockClear(); f.recheck.mockClear(); f.setAvailable(undefined); f.setOff(null); f.setList(undefined); document.body.replaceChildren(); });

function setup(available: boolean | undefined, off: "inactive" | "unavailable" | null = null, live = false) {
  f.setAvailable(available); f.setOff(off);
  f.setList(live ? { teams: [{ id: "t", status: "running" }], truncated: false, next: null } : { teams: [], truncated: false, next: null });
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(TeamsSection, {}), host);
  return host;
}
const toggle = (host: HTMLElement) => host.querySelector('[role="switch"]') as HTMLButtonElement;
const wait = async () => { await Promise.resolve(); await Promise.resolve(); };

it("tells the three states of discovery", () => {
  expect(previewState(true, null)).toBe("on");
  expect(previewState(false, "inactive")).toBe("off");
  expect(previewState(false, "unavailable")).toBe("unavailable");
  expect(previewState(undefined, null)).toBe("unknown");
});

it("shows off from discovery and turns it on once", async () => {
  const host = setup(false, "inactive");
  expect(toggle(host).getAttribute("aria-checked")).toBe("false");
  toggle(host).click();
  await wait();
  expect(f.preview).toHaveBeenCalledTimes(1);
  expect(f.preview).toHaveBeenCalledWith(true);
});

it("shows on from discovery and turns it off once", async () => {
  const host = setup(true);
  expect(toggle(host).getAttribute("aria-checked")).toBe("true");
  toggle(host).click();
  await wait();
  expect(f.preview).toHaveBeenCalledWith(false);
});

it("asks before turning it off while a team is live, with Cancel as the default", async () => {
  const host = setup(true, null, true);
  toggle(host).click();
  await wait();
  expect(f.preview).not.toHaveBeenCalled();
  const dialog = host.querySelector("dialog")!;
  expect(dialog.textContent).toContain("stop being served");
  expect(dialog.textContent).toContain("Nothing is deleted");
  const cancel = dialog.querySelector("[data-cancel]") as HTMLButtonElement;
  expect(document.activeElement).toBe(cancel);
  (dialog.querySelector(".solid-danger") as HTMLButtonElement).click();
  await wait();
  expect(f.preview).toHaveBeenCalledWith(false);
});

it("disables the row while hgs is unavailable, with the reason", () => {
  const host = setup(false, "unavailable");
  expect(toggle(host).disabled).toBe(true);
  expect(host.querySelector("[data-teams-unavailable]")?.textContent).toContain("hgs is unavailable");
});

it("shows a refusal's reason in the row", async () => {
  f.preview.mockResolvedValueOnce({ outcome: "failed", detail: "not_allowed: Only a person turns the Teams preview on or off." });
  const host = setup(false, "inactive");
  toggle(host).click();
  await wait();
  expect(host.querySelector("[data-teams-error]")?.textContent).toContain("Only a person turns the Teams preview on or off.");
});

it("an unknown result changes nothing and offers to read the state again", async () => {
  f.preview.mockResolvedValueOnce({ outcome: "uncertain", detail: "reply lost" });
  const host = setup(true);
  toggle(host).click();
  await wait();
  const notice = host.querySelector("[data-teams-uncertain]")!;
  expect(notice.textContent).toContain("Nothing on screen changed.");
  (notice.querySelector("button") as HTMLButtonElement).click();
  expect(f.recheck).toHaveBeenCalledTimes(1);
});
