// Delta review of #2535 (codex/zerus-review2525/review2525-cx), adopted: a
// view's ordering record outlives other views while it has outstanding
// outcomes or shown feedback, and a successful Dismiss records its order.
import { expect, it } from "vitest";
import { createTeams, type TeamCall, type TeamOutcome } from "./teams";

const example = (number: number): TeamCall => ({
  team: "00000000-0000-4000-8000-000000000010",
  team_name: "Review", expected_revision: 1,
  request_id: `00000000-0000-4000-8000-${String(number).padStart(12, "0")}`,
  action: { kind: "route", node: 12 },
});
const success = (call: TeamCall): TeamOutcome => ({ outcome: "done", receipt: {}, request_id: call.request_id });
const refusal = (call: TeamCall, detail: string): TeamOutcome =>
  ({ outcome: "failed", code: "stale_revision", detail, request_id: call.request_id });
const source = 'team-tab:["stable-lead-session","machine","launch","run"]:team';

async function otherViews(store: ReturnType<typeof createTeams>, count: number) {
  for (let index = 0; index < count; index++) {
    await store.run(example(index + 100), `team-tab:other-view-${index}`);
  }
}

it.each([63, 64])("does not restore an older dismissal after visiting %i other views and returning", async (views) => {
  let reject!: (e: Error) => void;
  const earlier = example(1);
  const store = createTeams({
    async act(call: TeamCall) {
      return call.request_id === earlier.request_id
        ? { outcome: "uncertain", detail: "Original reply missing", request_id: call.request_id }
        : success(call);
    },
    dismiss() { return new Promise<void>((_, denied) => { reject = denied; }); },
  } as never);
  await store.run(earlier, source);
  const dismissal = store.dismiss(store.unresolved()[0], source);
  expect(store.busy(earlier.team)).toBe(false);
  await store.run(example(2), source);
  expect(store.failureFor(source)).toBeNull();
  store.clearFailure(source); // The Team tab clears this key on leaving.
  await otherViews(store, views);
  expect(store.failureFor(source)).toBeNull(); // Returning uses the same session/run key.
  reject(new Error("Old dismissal refused"));
  expect(await dismissal).toBe(false);
  expect(store.failureFor(source)).toBeNull();
});

it.each([63, 64])("retains the newer refusal when an older success follows %i completions from other views", async (views) => {
  let resolve!: (outcome: TeamOutcome) => void;
  const earlier = example(1), later = example(2);
  const store = createTeams({
    act(call: TeamCall) {
      if (call.request_id === earlier.request_id) return new Promise<TeamOutcome>((done) => { resolve = done; });
      if (call.request_id === later.request_id) return Promise.resolve(refusal(call, "Newer Route refusal"));
      return Promise.resolve(success(call));
    },
  } as never);
  const waiting = store.run(earlier, source);
  await store.run(later, source);
  expect(store.failureFor(source)?.message).toBe("Newer Route refusal");
  await otherViews(store, views);
  expect(store.failureFor(source)?.message).toBe("Newer Route refusal");
  resolve(success(earlier));
  expect(await waiting).toMatchObject({ outcome: "done" });
  expect(store.busy()).toBe(false);
  expect(store.failureFor(source)?.message).toBe("Newer Route refusal");
});

it("keeps an earlier dismissal failure out of the banner after a later dismissal succeeds", async () => {
  const earlier = example(1), later = example(2);
  let reject!: (error: Error) => void;
  const store = createTeams({
    async act(call: TeamCall) {
      return { outcome: "uncertain", detail: "Original reply missing", request_id: call.request_id };
    },
    dismiss(call: TeamCall) {
      return call.request_id === earlier.request_id
        ? new Promise<void>((_, denied) => { reject = denied; })
        : Promise.resolve();
    },
  } as never);
  await store.run(earlier, source);
  await store.run(later, source);
  const originalDismiss = store.dismiss(store.unresolved()[0], source);
  expect(store.busy()).toBe(false);
  expect(await store.dismiss(store.unresolved()[1], source)).toBe(true);
  expect(store.unresolved().map(({ call }) => call.request_id)).toEqual([earlier.request_id]);
  reject(new Error("Older dismissal failed"));
  expect(await originalDismiss).toBe(false);
  expect(store.failureFor(source)).toBeNull();
});
