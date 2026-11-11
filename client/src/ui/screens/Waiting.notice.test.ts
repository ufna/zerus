// @vitest-environment happy-dom
// The read failure uses the Team tab's shared notice: the tinted bar, its icon
// and the error tone, not plain text.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
vi.mock("../../store/teams", () => ({
  teams: {
    failure: () => null, readFailureFor: (key: string) => key === "waiting" ? { key: "waiting", code: "read_failed", message: "The computer is offline.", source: "" } : null,
    failureFor: () => null, clearFailure: vi.fn(), firstWaitingPage: vi.fn(async () => {}), listed: () => [], team: () => undefined, summaryOf: () => ({ state: "reading" }), busy: () => false,
    waitingView: () => undefined, list: () => undefined, probe: vi.fn(async () => {}), openWaiting: vi.fn(async () => {}),
    pageWaiting: vi.fn(async () => {}), decide: vi.fn(async () => undefined), uncertain: () => null,
    check: vi.fn(), dismiss: vi.fn(), available: () => true, waitingCount: () => 0,
  },
}));
vi.mock("../../store/ui", () => ({ now: () => 0, setDetailTab: vi.fn(), setPage: vi.fn(), setSelectedKey: vi.fn() }));
vi.mock("../../store/catalog", () => ({ orderedRows: () => [] }));
vi.mock("../../store/attention", () => ({ actionNeeded: () => false }));
vi.mock("../teams/TeamTab", () => ({ TeamUncertain: () => null }));
vi.mock("../teams/TeamConfirmDialog", () => ({ TeamConfirmDialog: () => null }));
import { WaitingScreen } from "./Waiting";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

it("shows the read failure as the Team tab's shared notice", () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(WaitingScreen, {}), host);
  const notice = host.querySelector(".notice.danger.team-notice")!;
  expect(notice).not.toBeNull();
  expect(notice.getAttribute("role")).toBe("alert");
  expect(notice.querySelector("svg")).not.toBeNull();
  expect(notice.textContent).toBe("Waiting could not be read: The computer is offline.");
});
