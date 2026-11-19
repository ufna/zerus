// Review of #2535 (codex/zerus-review2525/review2525-cx), adopted: a view's
// refusal feedback follows the order its actions were dispatched, the
// unresolved-action Dismiss included; every request still settles.
import { describe, expect, it } from "vitest";
import { createTeams, type TeamCall, type TeamOutcome } from "./teams";

const source = "team-tab:retained";
const original: TeamCall = {
  team: "00000000-0000-4000-8000-000000000010", team_name: "Review", expected_revision: 1,
  request_id: "00000000-0000-4000-8000-000000000001", action: { kind: "reassign", node: 12 },
};
const newer: TeamCall = {
  ...original, request_id: "00000000-0000-4000-8000-000000000002", action: { kind: "route", node: 12 },
};
const failed = (call: TeamCall, detail: string): TeamOutcome =>
  ({ outcome: "failed", code: "stale_revision", detail, request_id: call.request_id });
const done = (call: TeamCall): TeamOutcome =>
  ({ outcome: "done", receipt: { ok: true }, request_id: call.request_id });

function control() {
  const transport = new Map<string, (result: TeamOutcome) => void>();
  const act = (call: TeamCall) => new Promise<TeamOutcome>((resolve) => transport.set(call.request_id, resolve));
  const store = createTeams({ act, actAgain: act } as never);
  return { store, resolve: (call: TeamCall, outcome: TeamOutcome) => transport.get(call.request_id)!(outcome) };
}

describe("ordered action feedback on a single team view", () => {
  it("does not clear a newer refusal when an earlier success finishes late", async () => {
    const { store, resolve } = control();
    const before = store.run(original, source);
    const after = store.run(newer, source);
    resolve(newer, failed(newer, "Newer Route refusal"));
    await after;
    expect(store.failureFor(source)?.action).toBe("route");
    resolve(original, done(original));
    await before;
    expect(store.failureFor(source)?.message).toBe("Newer Route refusal");
  });

  it("does not restore an older refusal after a newer success", async () => {
    const { store, resolve } = control();
    const before = store.run(original, source);
    const after = store.run(newer, source);
    resolve(newer, done(newer));
    await after;
    resolve(original, failed(original, "Older Reassign refusal"));
    await before;
    expect(store.failureFor(source)).toBeNull();
  });

  it("keeps a newer refusal against a late older refusal", async () => {
    const { store, resolve } = control();
    const before = store.run(original, source);
    const after = store.run(newer, source);
    resolve(newer, failed(newer, "Newer Route refusal"));
    await after;
    resolve(original, failed(original, "Older Reassign refusal"));
    await before;
    expect(store.failureFor(source)?.message).toBe("Newer Route refusal");
  });

  it("clears a known older refusal only on success in its requesting view", async () => {
    const { store, resolve } = control();
    const before = store.run(original, source);
    resolve(original, failed(original, "Earlier refusal"));
    await before;
    const elsewhere = store.run(newer, "another-view");
    resolve(newer, done(newer));
    await elsewhere;
    expect(store.failureFor(source)?.message).toBe("Earlier refusal");
    const after = store.run(newer, source);
    resolve(newer, done(newer));
    await after;
    expect(store.failureFor(source)).toBeNull();
  });
});

it("a dismissal failure started before a later ordinary success cannot restore a stale banner", async () => {
  let rejectDismiss!: (error: Error) => void;
  const store = createTeams({
    async act(call: TeamCall) {
      return call.request_id === original.request_id
        ? { outcome: "uncertain", detail: "Unknown original delivery", request_id: call.request_id }
        : done(call);
    },
    dismiss() { return new Promise<void>((_, reject) => { rejectDismiss = reject; }); },
  } as never);
  await store.run(original, source);
  const dismissal = store.dismiss(store.unresolved()[0], source);
  expect(store.busy(original.team)).toBe(false); // The Team tab permits the next ordinary action.
  expect(await store.run(newer, source)).toMatchObject({ outcome: "done" });
  rejectDismiss(new Error("Older dismissal did not settle"));
  expect(await dismissal).toBe(false);
  expect(store.failureFor(source)).toBeNull();
});
