import { describe, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import type { Task } from "./tasks-ui";

// The recovery model alone (third confirmation, finding 3): one Send at a
// time on a notice, and a reply settles only the notice it was started for.
vi.mock("../../store/tasks", () => ({ tasks: { project: () => "example-project", find: () => undefined } }));
vi.mock("../../store/taskBoard", () => ({ sendToTeam: vi.fn() }));
import { createRecovery } from "./task-recovery";

const TEAM = "33333333-3333-4333-8333-333333333333";
const notice = (task: string) => ({ project: "example-project", task, team: TEAM, name: "Fixes", reason: "Not queued." });
const row = (uuid: string) => ({ uuid, stage: "todo", writable: true } as Task);
const settle = async () => { for (let n = 0; n < 6; n++) await Promise.resolve(); };

describe("the not-queued recovery", () => {
  it("sends once at a time, and a Dismiss during the send leaves the reply nothing to change", async () => {
    let finish!: (value: { sent: boolean }) => void;
    const send = vi.fn(() => new Promise<{ sent: boolean }>((resolve) => { finish = resolve; }));
    const recovery = createRoot(() => createRecovery(async (key) => row(key.split("\n")[1]), send));
    recovery.show(notice("11111111-1111-4111-8111-111111111111"));
    await settle();
    const first = recovery.send();
    const second = await recovery.send();
    expect(second).toBeUndefined();
    expect(send).toHaveBeenCalledTimes(1);
    recovery.dismiss();
    recovery.show(notice("22222222-2222-4222-8222-222222222222"));
    finish({ sent: true });
    await first;
    expect(recovery.notice()?.task).toBe("22222222-2222-4222-8222-222222222222");
    expect(recovery.sending()).toBe(false);
  });

  it("a refusal stays on the notice it was for", async () => {
    const send = vi.fn(async () => ({ sent: false, reason: "The queue is full." }));
    const recovery = createRoot(() => createRecovery(async (key) => row(key.split("\n")[1]), send));
    recovery.show(notice("11111111-1111-4111-8111-111111111111"));
    await settle();
    await recovery.send();
    expect(recovery.refusal()).toBe("The queue is full.");
    expect(recovery.notice()).not.toBeNull();
    expect(recovery.sending()).toBe(false);
  });

  it("a read that names another task is not taken", async () => {
    const recovery = createRoot(() => createRecovery(async () => row("99999999-9999-4999-8999-999999999999"), vi.fn()));
    recovery.show(notice("11111111-1111-4111-8111-111111111111"));
    await settle();
    expect(recovery.row()).toBeUndefined();
  });
});

describe("a title in a notification", () => {
  it("is kept whole when short and cut on a character with an ellipsis when long", async () => {
    const { boundedTitle, TITLE_CHARS } = await import("./task-recovery");
    expect(boundedTitle("  Fix the login  ")).toBe("Fix the login");
    const family = "👨‍👩‍👧";
    const cut = boundedTitle(family.repeat(TITLE_CHARS + 5));
    expect(cut.endsWith("…")).toBe(true);
    // Every kept family is whole: the cut never splits a grapheme.
    expect(cut.slice(0, -1)).toBe(family.repeat(TITLE_CHARS - 1));
    expect(boundedTitle("x".repeat(TITLE_CHARS))).toBe("x".repeat(TITLE_CHARS));
  });
});
