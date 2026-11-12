import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "../ui/screens/tasks-ui";

// Send to team on a task card: the existing board path, one command, a fresh
// stage check and no second send while one is in flight.
const f = vi.hoisted(() => ({
  action: vi.fn(), notify: vi.fn(), project: "example-project",
  find: vi.fn(), pending: undefined as Promise<void> | undefined,
}));
vi.mock("../bridge", () => ({ taskBoardAction: f.action }));
vi.mock("./actions", () => ({ notify: f.notify }));
vi.mock("./tasks", () => ({ tasks: { project: () => f.project, find: (uuid: string) => f.find(uuid) } }));
import { sendToTeam, writingTask } from "./taskBoard";

const TASK = "11111111-1111-4111-8111-111111111111";
const TEAM = "33333333-3333-4333-8333-333333333333";
const task = (over: Partial<Task> = {}): Task => ({ uuid: TASK, stage: "todo", writable: true, id: 1, title: "T", ...over } as Task);

beforeEach(() => { f.action.mockReset(); f.notify.mockReset(); f.find.mockReset(); f.find.mockReturnValue(task()); });

describe("Send to team", () => {
  it("sends the fresh card's stage and the team", async () => {
    f.find.mockReturnValue(task({ stage: "backlog" }));
    await sendToTeam(f.project, task({ stage: "backlog" }), TEAM, "Fixes");
    expect(f.action).toHaveBeenCalledWith(f.project, TASK, "backlog", { kind: "send_to_team", team: TEAM });
    expect(f.notify).toHaveBeenCalledWith("info", "Sent to Fixes");
    expect(writingTask()).toBeUndefined();
  });

  it("refuses a card that left Backlog or To do without sending", async () => {
    f.find.mockReturnValue(task({ stage: "active" }));
    await sendToTeam(f.project, task(), TEAM, "Fixes");
    expect(f.action).not.toHaveBeenCalled();
    expect(f.notify).toHaveBeenCalledWith("error", expect.stringContaining("The task changed"));
  });

  it("shows the team's reason from a refusal", async () => {
    f.action.mockRejectedValue("The queue is full.");
    await sendToTeam(f.project, task(), TEAM, "Fixes");
    expect(f.notify).toHaveBeenCalledWith("error", "The queue is full.");
  });

  it("does not send twice while one send is in flight", async () => {
    let resolve!: () => void;
    f.action.mockReturnValue(new Promise<void>((done) => { resolve = done; }));
    const first = sendToTeam(f.project, task(), TEAM, "Fixes");
    const second = sendToTeam(f.project, task(), TEAM, "Fixes");
    expect(f.action).toHaveBeenCalledTimes(1);
    resolve();
    await Promise.all([first, second]);
    expect(f.action).toHaveBeenCalledTimes(1);
    expect(f.notify.mock.calls.filter(([tone]) => tone === "info")).toHaveLength(1);
  });
});

// Third confirmation, finding 2: a row the board does not hold is a task not
// read yet, never a changed one; a row the caller read exactly is used, and
// the board's own row wins when it has one.
describe("Send to team without the board's row", () => {
  it("says the task is not read yet and sends nothing", async () => {
    f.find.mockReturnValue(undefined);
    const outcome = await sendToTeam(f.project, { uuid: TASK }, TEAM, "Fixes");
    expect(outcome).toMatchObject({ sent: false, reason: "The task is not read yet. Nothing was sent." });
    expect(f.action).not.toHaveBeenCalled();
    expect(f.notify.mock.calls.flat().join(" ")).not.toContain("changed");
  });

  it("goes through the same checks with a row read exactly, and prefers the board's", async () => {
    f.find.mockReturnValue(undefined);
    expect(await sendToTeam(f.project, { uuid: TASK }, TEAM, "Fixes", { read: task({ stage: "active" }) })).toMatchObject({ sent: false, reason: expect.stringContaining("The task changed") });
    expect(await sendToTeam(f.project, { uuid: TASK }, TEAM, "Fixes", { read: task({ uuid: "22222222-2222-4222-8222-222222222222" }) })).toMatchObject({ sent: false, reason: "The task is not read yet. Nothing was sent." });
    expect(await sendToTeam(f.project, { uuid: TASK }, TEAM, "Fixes", { read: task({ stage: "backlog" }) })).toMatchObject({ sent: true });
    expect(f.action).toHaveBeenLastCalledWith(f.project, TASK, "backlog", { kind: "send_to_team", team: TEAM });
    f.find.mockReturnValue(task({ stage: "todo" }));
    await sendToTeam(f.project, { uuid: TASK }, TEAM, "Fixes", { read: task({ stage: "backlog" }) });
    expect(f.action).toHaveBeenLastCalledWith(f.project, TASK, "todo", { kind: "send_to_team", team: TEAM });
  });
});
