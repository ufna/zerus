// @vitest-environment happy-dom
// A member opens its own session (zerus-rzvct G2): its name, "Session linked"
// in its details and Open session in its menu select the session row bound by
// the member's machine and launch, the same binding the lead's Team tab uses.
// The member is product-written (lanes-view.json); the row is the product's
// toSession of a catalog row; navigation is the real UI store.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import fixture from "./fixtures/lanes-view.json";
import type { Session } from "../sessions/types";
import type { TeamView } from "../../store/teams";
import type { ViewRow } from "../../types";
import type { MemberView } from "./types";

const f = vi.hoisted(() => ({ rows: [] as Session[] }));
vi.mock("../../store/machines", async (original) => ({ ...(await original<object>()), machineCards: { "": { machine_id: "00000000-0000-4000-8000-0000000000aa" } } }));
vi.mock("../../store/catalog", async (original) => ({ ...(await original<object>()), orderedRows: () => f.rows }));
let ui: typeof import("../../store/ui");
let MemberCard: typeof import("./MemberCard").MemberCard;
let MenuLayer: typeof import("../components/Overlays").MenuLayer;
let toSession: typeof import("../../store/catalog").toSession;
const page = (fixture.pages as unknown as TeamView[])[0];
const m1 = page.members.find((member) => member.id === "m1")! as MemberView;
const disposers: (() => void)[] = [];

function rowFor(member: MemberView): Session {
  return toSession("", `run:${member.id}`, { name: `claude/sample-project/${member.id}`, agent: "claude", status: "ready", state: "running",
    folder: "sample-project", run_id: `run-${member.id}`, launch_id: member.session!.session_id.slice("launch_id:".length) } as unknown as ViewRow);
}
function show(member: MemberView) {
  const host = document.createElement("div"); document.body.append(host);
  disposers.push(render(() => [createComponent(MemberCard, { member, teamStatus: "running", busy: false, onAction: vi.fn() }), createComponent(MenuLayer, {})], host));
  return host;
}
const menuItem = (host: HTMLElement, text: string) => {
  host.querySelector<HTMLButtonElement>('[aria-label="Member actions"]')!.click();
  return [...host.querySelectorAll<HTMLButtonElement>(".menu-item")].find((item) => item.textContent === text);
};
const opened = () => ({ page: ui.page(), key: ui.selectedKey(), tab: ui.detailTab() });

beforeEach(async () => {
  vi.resetModules(); f.rows = [];
  ui = await import("../../store/ui"); ({ MemberCard } = await import("./MemberCard"));
  ({ MenuLayer } = await import("../components/Overlays")); ({ toSession } = await import("../../store/catalog"));
  ui.setPage("teams"); ui.setSelectedKey(undefined); ui.setDetailTab("team");
});
afterEach(() => { while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); ui.setMenu(undefined); });

it("opens the member's bound session from its name", () => {
  const row = rowFor(m1); f.rows = [rowFor({ ...m1, id: "other", session: { ...m1.session!, session_id: "launch_id:00000000-0000-4000-8000-000000000099" } }), row];
  const host = show(m1);
  const name = host.querySelector<HTMLButtonElement>("button.team-member-name");
  expect(name?.textContent).toBe("m1");
  name!.click();
  expect(opened()).toEqual({ page: "sessions", key: row.key, tab: "activity" });
});

it("opens it from Session linked in the member's details", () => {
  const row = rowFor(m1); f.rows = [row];
  const host = show(m1);
  menuItem(host, "Member details")!.click();
  const linked = host.querySelector<HTMLButtonElement>("button.team-member-session");
  expect(linked?.textContent).toBe("Session linked");
  linked!.click();
  expect(opened()).toEqual({ page: "sessions", key: row.key, tab: "activity" });
});

it("offers Open session in the member's menu", () => {
  const row = rowFor(m1); f.rows = [row];
  const host = show(m1);
  const item = menuItem(host, "Open session")!;
  expect(item.getAttribute("aria-disabled")).not.toBe("true");
  item.click();
  expect(opened()).toEqual({ page: "sessions", key: row.key, tab: "activity" });
});

it("says why a member's session cannot be opened and never opens another row", () => {
  f.rows = [rowFor({ ...m1, session: { ...m1.session!, session_id: "launch_id:00000000-0000-4000-8000-000000000099" } })];
  let host = show(m1);
  expect(host.querySelector("button.team-member-name")).toBeNull();
  let item = menuItem(host, "Open session")!;
  expect(item.getAttribute("aria-disabled")).toBe("true");
  expect(item.getAttribute("title")).toBe("This member's session is not on the session list.");
  item.click();
  expect(opened()).toEqual({ page: "teams", key: undefined, tab: "team" });
  while (disposers.length) disposers.pop()!(); document.body.replaceChildren(); ui.setMenu(undefined);
  host = show({ ...m1, session: null });
  item = menuItem(host, "Open session")!;
  expect(item.getAttribute("aria-disabled")).toBe("true");
  expect(item.getAttribute("title")).toBe("This member has no linked session.");
});
