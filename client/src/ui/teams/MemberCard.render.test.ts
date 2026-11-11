// @vitest-environment happy-dom
// The member card (the fork's MemberCard, adapted): its role and colour, its
// state word, the account and the session it runs on, and only the M1 member
// operations.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { MemberCard } from "./MemberCard";
import type { MemberView } from "./types";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

const member = (over: Partial<MemberView> = {}): MemberView => ({
  id: "m1", role: "developer", agent: "claude", account: "account-example", state: "active", lead: false,
  session: { machine: "00000000-0000-4000-8000-0000000000aa", session_id: "launch_id:00000000-0000-4000-8000-000000000031" },
  ...over,
});

function setup(value: MemberView, onAction = vi.fn()) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(MemberCard, { member: value, busy: false, onAction }), host);
  return { host, onAction };
}

it("shows the member's role, state, account and bound session", () => {
  const { host } = setup(member());
  expect(host.querySelector(".team-member-name")?.textContent).toBe("m1");
  expect(host.querySelector(".role-badge")?.textContent).toBe("developer");
  expect(host.textContent).toContain("active");
  expect(host.textContent).toContain("account-example");
  expect(host.querySelector(".team-member-session")?.textContent).toContain("launch_id:");
});

it("marks the lead and a state's tone", () => {
  const { host } = setup(member({ id: "lead", role: "lead", lead: true }));
  expect(host.querySelector(".team-member-card")?.classList.contains("team-lead")).toBe(true);
  expect(host.querySelector(".tm-state-badge")?.getAttribute("data-tone")).toBe("info");
  expect(host.querySelector(".team-member-lead")?.textContent).toBe("lead");
});

it("shows a not-created member without a session", () => {
  const { host } = setup(member({ state: "not_created", account: "", session: null }));
  expect(host.querySelector(".tm-state-badge")?.getAttribute("data-tone")).toBe("dim");
  expect(host.textContent).toContain("not created");
  expect(host.textContent).toContain("the default account");
  expect(host.querySelector(".team-member-session")).toBeNull();
});

it("offers only the M1 operations and sends them by member id", () => {
  const { host, onAction } = setup(member());
  const buttons = [...host.querySelectorAll("[data-member-actions] button")].map((button) => button.getAttribute("data-member-action"));
  expect(buttons).toEqual(["pause-member", "stop-member", "archive-member"]);
  (host.querySelector('[data-member-action="pause-member"]') as HTMLButtonElement).click();
  expect(onAction).toHaveBeenCalledWith("pause-member", "m1");
});

it("offers resume only to a paused member", () => {
  const { host } = setup(member({ state: "paused" }));
  const buttons = [...host.querySelectorAll("[data-member-actions] button")].map((button) => button.getAttribute("data-member-action"));
  expect(buttons).toEqual(["resume-member", "stop-member", "archive-member"]);
});
