// @vitest-environment happy-dom
// A card offers the decisions its row lists, in the list's order, and nothing
// else (zerus-63rru.29: every opener states the decisions something reads, and
// DecideCard refuses one the card does not list). No button comes from the
// card's kind; a token this version has no words for is not offered; a row
// without a list (an older hgs) takes acknowledged alone.
import { afterEach, describe, expect, it } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { waitingDecisions } from "./decisions";
import { WaitingList } from "./Waiting";
import type { WaitingView } from "../../store/teams";
// The client core's waiting view of one card of each shape (client/core
// teams::view::tests pins this file from its own reading of the rows).
import fromCore from "../../../core/src/teams/fixtures/waiting-decisions.json";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

describe("the decisions a card offers", () => {
  it("are the card's list, in its order, whatever the card's kind", () => {
    expect(waitingDecisions({ kind: "note", decisions: ["approve", "reject"] }).map((choice) => choice.decision)).toEqual(["approve", "reject"]);
    expect(waitingDecisions({ kind: "integration", decisions: ["acknowledged"] }).map((choice) => choice.decision)).toEqual(["acknowledged"]);
    expect(waitingDecisions({ kind: "team_actor_blocked", decisions: ["retry", "acknowledged"] }).map((choice) => choice.text)).toEqual(["Retry", "Acknowledge"]);
    expect(waitingDecisions({ kind: "launch_capacity", decisions: ["retry"] }).map((choice) => choice.decision)).toEqual(["retry"]);
  });

  it("leave out a token this version has no words for, and take acknowledged alone without a list", () => {
    expect(waitingDecisions({ kind: "note", decisions: ["snooze", "acknowledged"] }).map((choice) => choice.decision)).toEqual(["acknowledged"]);
    expect(waitingDecisions({ kind: "note", decisions: ["snooze"] })).toEqual([]);
    expect(waitingDecisions({ kind: "integration" })).toEqual([{ decision: "acknowledged", text: "Acknowledge" }]);
  });

  it("mark only a rejection as dangerous", () => {
    for (const choice of waitingDecisions({ kind: "x", decisions: ["approve", "reject", "retry", "acknowledged"] })) {
      expect(!!choice.danger, choice.decision).toBe(choice.decision === "reject");
    }
  });

  it("through the rendered list and the core's reading of each card", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const decided: [string, string][] = [];
    dispose = render(() => createComponent(WaitingList, {
      view: fromCore as WaitingView, teamNames: new Map(), now: 10_000, busy: false,
      onDecide: (item, decision) => decided.push([item.id, decision]), onOpenTeam: () => {}, onPage: () => {},
    }), host);
    const offered = (card: number) => [...host.querySelectorAll(`[data-waiting$=":${card}"] button[data-decision]`)].map((button) => button.getAttribute("data-decision"));
    expect(offered(1), "a retry-only card offers no Acknowledge").toEqual(["retry"]);
    expect(offered(2), "a blocked launch offers no Retry by its kind").toEqual(["acknowledged"]);
    expect(offered(3)).toEqual(["approve", "reject"]);
    expect(offered(4), "the actor's gate card").toEqual(["retry", "acknowledged"]);
    expect(offered(5), "the actor's held card").toEqual(["retry", "acknowledged"]);
    expect(offered(6), "a token without words is not offered").toEqual(["acknowledged"]);
    expect(offered(7)).toEqual(["acknowledged"]);
    expect(offered(8), "a row from an older hgs").toEqual(["acknowledged"]);
    host.querySelector<HTMLButtonElement>('[data-waiting$=":1"] [data-decision="retry"]')!.click();
    expect(decided).toEqual([[`${fromCore.items[0].team}:1`, "retry"]]);
  });
});
