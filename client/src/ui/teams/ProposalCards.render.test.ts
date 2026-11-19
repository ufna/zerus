// @vitest-environment happy-dom
import { afterEach, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import type { DrawerRow } from "../../store/teamDrawers";
import type { WaitingItem } from "../../store/teams";
import { ProposalCards } from "./ProposalCards";
import saved from "./fixtures/deepseek-host-cards-legacy.json";
import current from "./fixtures/deepseek-host-cards.json";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

function row(card: WaitingItem): DrawerRow {
  return { seq: card.card, kind: card.kind, by: card.by, text: card.text, text_truncated: false,
    at_ms: card.created_ms, node: card.node, generation: null, state: card.state, reply: null, notes: [], card };
}

it.each([
  ["saved member", saved.member, "remove this member and add a replacement"],
  ["saved lead", saved.lead, "stop this team and create a new one"],
  ["current member", current.member, "remove this member and add a replacement"],
  ["current lead", current.lead, "stop this team and create a new one"],
] as const)("shows one explanation and the recovery action for a %s card", (_, card, action) => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => createComponent(ProposalCards, { rows: [row(card as WaitingItem)], busy: false, onAction: () => {} }), host);
  const article = host.querySelector("[data-proposal]")!;
  const heading = `Member ${card.parameters[0][1]} needs a DeepSeek host with Agent Teams support.`;
  expect(article.textContent?.split(heading)).toHaveLength(2);
  expect(article.textContent).not.toMatch(/invalid_state|\/work\/example\/\.dsh/);
  expect(article.querySelector(".team-drawer-text")?.textContent).toBe(
    `Stop idle DeepSeek sessions or restart the host yourself, then ${action} once the host is back, since Zerus will not retry this start.`,
  );
});
