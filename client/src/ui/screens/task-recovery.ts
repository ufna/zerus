import { createSignal } from "solid-js";
import { tr } from "../../i18n";
import { notify } from "../../store/actions";
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
// for a notice that was dismissed or replaced touches no notice. One Send is
// in flight at a time.
//
// The recovery owns the presentation of its Send. Every reply, wherever the
// person is and whatever happened to the notice, shows one notification that
// names the task by its bounded title and the team: that it was sent, or the
// result as it came, in words that never claim the send did not happen (an
// unconfirmed write may have). The notice itself is handled only when it is
// the one the reply was started for and is still shown: cleared on success,
// the reason kept on it otherwise, with Send still available; the native
// fresh-stage and read-only checks decide a repeat. A notice is retired when
// its form leaves its project.

export interface NotQueued { project: string; task: string; team: string; name: string; reason: string }

/** A task's title in a notification: at most this many characters. */
export const TITLE_CHARS = 60;

/** A title cut on a character (a grapheme where the platform can tell), with an ellipsis. */
export function boundedTitle(title: string, limit = TITLE_CHARS): string {
  const text = title.trim();
  const Segmenter = (Intl as { Segmenter?: new (locale?: string, options?: { granularity: "grapheme" }) => { segment(text: string): Iterable<{ segment: string }> } }).Segmenter;
  const parts = Segmenter ? [...new Segmenter(undefined, { granularity: "grapheme" }).segment(text)].map((part) => part.segment) : Array.from(text);
  return parts.length > limit ? `${parts.slice(0, limit - 1).join("").trimEnd()}…` : text;
}
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
      const title = boundedTitle(task.title || task.ref || shown.task);
      const outcome = await send(shown.project, { uuid: shown.task }, shown.team, shown.name, { read: task, quiet: true });
      if (current(shown.key)) {
        if (outcome.sent) setNotice(null);
        else setRefusal(outcome.reason ?? "");
        setSending(false);
      }
      if (outcome.sent) notify("info", tr("“%1” was sent to %2.", title, shown.name));
      else notify("error", tr("Result of sending “%1” to %2: %3", title, shown.name, outcome.reason ?? ""));
      return outcome;
    },
  };
}
