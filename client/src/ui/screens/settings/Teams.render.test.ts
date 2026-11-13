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
  expect([...dialog.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Turn off", "Cancel"]);
  expect(cancel.classList.contains("primary")).toBe(true);
  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  cancel.dispatchEvent(enter);
  expect(enter.defaultPrevented).toBe(false);
  expect(f.preview).not.toHaveBeenCalled();
  (dialog.querySelector(".solid-danger") as HTMLButtonElement).click();
  await wait();
  expect(f.preview).toHaveBeenCalledWith(false);
});

it("disables the row while hgs is unavailable, with the reason", () => {
  const host = setup(false, "unavailable");
  expect(toggle(host).disabled).toBe(true);
  expect(host.querySelector("[data-teams-unavailable]")?.textContent).toContain("hgs is unavailable");
});

it("explains an unknown state and can read it again without changing the preview", async () => {
  const host = setup(undefined);
  expect(toggle(host).disabled).toBe(true);
  const notice = host.querySelector("[data-teams-unknown]")!;
  expect(notice.textContent).toContain("The preview state is unknown.");
  const retry = notice.querySelector("button")!;
  expect(retry.textContent).toBe("Read again");
  expect(retry.disabled).toBe(false);
  f.recheck.mockImplementationOnce(async () => { f.setAvailable(false); f.setOff("inactive"); });
  retry.click();
  await wait();
  expect(f.recheck).toHaveBeenCalledTimes(1);
  expect(f.preview).not.toHaveBeenCalled();
  expect(host.querySelector("[data-teams-unknown]")).toBeNull();
  expect(toggle(host).disabled).toBe(false);
  expect(toggle(host).getAttribute("aria-checked")).toBe("false");
});

it("shows a refusal's reason in the row", async () => {
  f.preview.mockResolvedValueOnce({ outcome: "failed", detail: "not_allowed: Only a person turns the Teams preview on or off." });
  const host = setup(false, "inactive");
  toggle(host).click();
  await wait();
  expect(host.querySelector("[data-teams-error]")?.textContent).toContain("Only a person turns the Teams preview on or off.");
});

it("shows a refusal's sentence without its code; the code stays in the switch's data", async () => {
  f.preview.mockResolvedValueOnce({ outcome: "failed", detail: "store_busy: The team store is busy. Nothing was written; try again." });
  const host = setup(false, "inactive");
  toggle(host).click();
  await wait();
  expect(host.querySelector("[data-teams-error]")?.textContent).toBe("The team store is busy. Nothing was written; try again.");
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

// Second confirmation, finding 5: turning off skips the confirmation only when
// the first page is the whole list and shows no live team.
it.each([
  ["a first page that goes on", { teams: Array.from({ length: 200 }, (_, n) => ({ id: `stopped-${n}`, status: "stopped" })), truncated: false, next: "live-team-on-page-two" }],
  ["a page cut short", { teams: [{ id: "stopped", status: "stopped" }], truncated: true, next: null }],
  ["a list not read yet", undefined],
])("asks before turning off with %s", async (_label, list) => {
  const host = setup(true);
  f.setList(list);
  toggle(host).click();
  await wait();
  expect(f.preview).not.toHaveBeenCalled();
  expect(host.querySelector("dialog")).not.toBeNull();
});

it("turns off at once when the whole list shows no live team", async () => {
  const host = setup(true);
  f.setList({ teams: [{ id: "stopped", status: "stopped" }, { id: "done", status: "archived" }], truncated: false, next: null });
  toggle(host).click();
  await wait();
  expect(host.querySelector("dialog")).toBeNull();
  expect(f.preview).toHaveBeenCalledWith(false);
});

it("says only that the teams stop being served and nothing is deleted", async () => {
  const host = setup(true, null, true);
  toggle(host).click();
  await wait();
  const text = host.querySelector("#teams-off-text")?.textContent;
  expect(text).toBe("Teams on this computer stop being served. Nothing is deleted.");
});
