// @vitest-environment happy-dom
// Delta review of #2535 (codex/zerus-review2525/review2525-cx), adopted as guards.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import generatedBoard from "../../../core/src/teams/fixtures/board-pending.json";
import generatedDrawers from "../../../core/src/teams/fixtures/drawers.json";
import generatedProviders from "../../../core/src/teams/fixtures/providers.json";
import providerScope from "../../../core/src/teams/fixtures/provider-scope.json";
import type { DrawerRow } from "../../store/teamDrawers";
import type { ProviderInspection } from "../../store/teamProviders";
import type { TeamView } from "./types";
import { ProposalCards } from "./ProposalCards";
import { ProviderSettings } from "./ProviderSettings";
import { memberActions } from "./actions";
import { swapBlocked } from "./member-edit";
import { openTeamNode, setOpenTeamNode } from "./openNode";

const team = generatedBoard as unknown as TeamView;
let unmount: (() => void) | undefined;
afterEach(() => { unmount?.(); unmount = undefined; document.body.replaceChildren(); setOpenTeamNode(null); });
function mount(component: () => import("solid-js").JSX.Element) {
  const host = document.createElement("div");
  document.body.append(host);
  unmount = render(component, host);
  return host;
}

it("allows Swap for a stopped member with zero open work and a session", () => {
  const original = team.members.find((m) => m.id === "m1")!;
  expect(original.session).toBeTruthy();
  const member = { ...original, state: "paused", person_stopped: true, open_work: 0 };
  expect(swapBlocked(member)).toBeUndefined();
  const swap = memberActions(member, "running").find((entry) => entry.id === "swap_member")!;
  expect(swap.disabled).toBe(false);
  expect(swap.reason).toBeUndefined();
});

it("names a missing session or unverified count on a stopped member without reported work", () => {
  const original = team.members.find((m) => m.id === "m1")!;
  const member = { ...original, state: "paused", person_stopped: true, open_work: 0 };
  expect(swapBlocked({ ...member, session: null })).toBe("This member has no confirmed session to replace.");
  expect(swapBlocked({ ...member, open_work: undefined })).toBe("Refresh the team to check this member's open work.");
  expect(swapBlocked({ ...member, person_stopped: false })).toBe("Stop the member first; open work: 0");
});

it("links the maximum node and keeps zero, padded, and oversized decimal labels as complete text", () => {
  const card = (generatedDrawers.cards as { rows: DrawerRow[] }).rows.find((row) => row.card)!;
  const text = "#0 #007 #4294967296 #4294967295 #12345678901 #6";
  const host = mount(() => createComponent(ProposalCards, {
    rows: [{ ...card, card: { ...card.card!, text } }], busy: false, onAction: () => {}, team: team.bar.id,
  }));
  const buttons = [...host.querySelectorAll<HTMLButtonElement>("[data-card-node]")];
  expect(buttons.map((button) => button.textContent)).toEqual(["#4294967295", "#6"]);
  expect(host.querySelector("pre")?.textContent).toBe(text);
  buttons[0].click();
  expect(openTeamNode()).toEqual({ team: team.bar.id, node: 4294967295 });
});

it("does not label a saved selection with a null override as an override", async () => {
  const inspection = structuredClone(generatedProviders.inspection) as ProviderInspection;
  inspection.saved = { digest: inspection.digest, source_digest: inspection.source_digest, override_config: null };
  inspection.saved_current = true;
  const host = mount(() => createComponent(ProviderSettings, {
    view: providerScope.team as unknown as TeamView, home: inspection.home, locked: false,
    onSave: vi.fn(), port: async () => inspection,
  }));
  await vi.waitFor(() => expect(host.querySelector("[data-provider-source]")?.textContent).toBe("Detected from project metadata"));
  expect(host.querySelector("[data-provider-saved-override]")).toBeNull();
  expect(host.querySelector("details")?.open).toBe(false);
});
