import { createRoot, createSignal } from "solid-js";
import { expect, it, vi } from "vitest";
import { followTeamScope, type TeamScope } from "./team-tab-scope";

it("retires a stale follow before opening the latest selection, with no stale publication (review 4)", async () => {
  const calls: string[] = [];
  let finishA!: () => void, setTarget!: (scope: TeamScope) => void;
  let scope!: ReturnType<typeof followTeamScope>, dispose!: () => void;
  createRoot((close) => {
    dispose = close;
    const [target, change] = createSignal<TeamScope>({ key: "session-a", team: "team-a" }); setTarget = change;
    scope = followTeamScope(target, {
      close: async () => { calls.push("close"); },
      open: async (team) => { calls.push(team); if (team === "team-a") await new Promise<void>((resolve) => { finishA = resolve; }); },
    });
  });
  try {
    await vi.waitFor(() => expect(calls).toEqual(["close", "team-a"]));
    setTarget({ key: "session-b", team: "team-b" });
    setTarget({ key: "session-c", team: "team-c" });
    expect(scope.ready()).toBeUndefined();
    finishA();
    await vi.waitFor(() => expect(scope.ready()).toEqual({ key: "session-c", team: "team-c" }));
    expect(calls).toEqual(["close", "team-a", "close", "team-c"]);
  } finally { dispose(); }
  await vi.waitFor(() => expect(calls.at(-1)).toBe("close"));
});

it("closes an in-flight follow after unmount without opening another scope", async () => {
  const calls: string[] = [];
  let finish!: () => void, dispose!: () => void;
  createRoot((close) => {
    dispose = close;
    followTeamScope(() => ({ key: "a", team: "team-a" }), { close: async () => { calls.push("close"); }, open: async (team) => { calls.push(team); await new Promise<void>((resolve) => { finish = resolve; }); } });
  });
  await vi.waitFor(() => expect(calls).toEqual(["close", "team-a"]));
  dispose(); finish();
  await vi.waitFor(() => expect(calls).toEqual(["close", "team-a", "close"]));
});

it("ignores an obsolete follow failure and still binds the latest selection", async () => {
  let fail!: () => void, change!: (scope: TeamScope) => void, dispose!: () => void;
  let scope!: ReturnType<typeof followTeamScope>;
  createRoot((close) => {
    dispose = close; const [desired, setDesired] = createSignal<TeamScope>({ key: "a", team: "team-a" }); change = setDesired;
    scope = followTeamScope(desired, { close: async () => {}, open: async (id) => { if (id === "team-a") await new Promise<void>((_resolve, reject) => { fail = () => reject(Error("old scope failed")); }); } });
  });
  try {
    await vi.waitFor(() => expect(fail).toBeTypeOf("function")); change({ key: "b", team: "team-b" }); fail();
    await vi.waitFor(() => expect(scope.ready()?.team).toBe("team-b")); expect(scope.failure()).toBe("");
  } finally { dispose(); }
});
