import { createSignal } from "solid-js";
import { tasks } from "../../store/tasks";
import { sendToTeam, type SendOutcome } from "../../store/taskBoard";
import { readSelectedTask } from "./task-view-read";
import type { Task } from "./tasks-ui";

// The New task form's recovery for a card the command line created and the
// team's queue did not take. The task exists, so the way on is Send to team
// for that task, never a second card.
//
// Each notice has its own identity. The created task is taken from one
// bounded exact read by the identity the create returned (the read TaskView
// uses), or from the board's own row when the board already has it; until
// then Send waits, and a failed read says so and can be read again. A reply,
// of the read or of Send, settles only the notice it was started for: a reply
// for a notice that was dismissed or replaced changes nothing on screen. One
// Send is in flight at a time.

export interface NotQueued { project: string; task: string; team: string; name: string; reason: string }
interface Notice extends NotQueued { key: number }
type Read = { state: "reading" } | { state: "failed"; reason: string } | { state: "ready"; row: Task };

export function createRecovery(read: (key: string) => Promise<Task> = readSelectedTask, send: typeof sendToTeam = sendToTeam) {
  const [notice, setNotice] = createSignal<Notice | null>(null);
  const [reading, setReading] = createSignal<Read>({ state: "reading" });
  const [sending, setSending] = createSignal(false);
  const [refusal, setRefusal] = createSignal("");
  let keys = 0, reads = 0;
  const current = (key: number) => notice()?.key === key;

  /** The row Send goes with: the board's own row when it has one, else the exact read's. */
  const row = () => {
    const shown = notice();
    if (!shown) return undefined;
    const board = tasks.project() === shown.project ? tasks.find(shown.task) : undefined;
    if (board) return board;
    const answer = reading();
    return answer.state === "ready" ? answer.row : undefined;
  };

  function readTask(shown: Notice) {
    const mine = ++reads;
    setReading({ state: "reading" });
    read(`${shown.project}\n${shown.task}`).then(
      (task) => { if (current(shown.key) && mine === reads && task.uuid === shown.task) setReading({ state: "ready", row: task }); },
      (error) => { if (current(shown.key) && mine === reads) setReading({ state: "failed", reason: String(error) }); },
    );
  }

  return {
    notice, reading, sending, refusal, row,
    show(created: NotQueued) {
      const shown = { ...created, key: ++keys };
      setNotice(shown); setSending(false); setRefusal("");
      readTask(shown);
    },
    readAgain() { const shown = notice(); if (shown && reading().state === "failed") readTask(shown); },
    dismiss() { setNotice(null); setSending(false); setRefusal(""); },
    async send(): Promise<SendOutcome | undefined> {
      const shown = notice(), task = row();
      if (!shown || !task || sending()) return undefined;
      setSending(true); setRefusal("");
      const outcome = await send(shown.project, { uuid: shown.task }, shown.team, shown.name, task);
      if (!current(shown.key)) return outcome;
      if (outcome.sent) setNotice(null);
      else setRefusal(outcome.reason ?? "");
      setSending(false);
      return outcome;
    },
  };
}
