import { WorktreeReadLane } from "./worktree-read-lane";
export const REMINDER_MS = 300_000;
export interface ReminderTarget { key: string; computer: string; machine: string; path: string; common: string }
export interface Reminder { count: number | null; retry_ms: number }

/** One current panel and one owned read; no timer or new call while hidden.
 * Native code keeps the bounded per-machine/repository attempt interval. */
export class WorktreeReminder {
  private lane = new WorktreeReadLane<Reminder>();
  private timer?: ReturnType<typeof setTimeout>;
  private signature = "";
  private target?: ReminderTarget;
  private version = 0;
  constructor(private read: (target: ReminderTarget) => Promise<Reminder>, private changed: (count?: number) => void) {}
  show(target: ReminderTarget | undefined, visible: boolean) {
    const signature = visible && target ? target.key + "\n" + target.common : "";
    this.target = signature ? target : undefined;
    if (signature === this.signature) return;
    this.signature = signature;
    const ticket = ++this.version;
    clearTimeout(this.timer); this.timer = undefined; this.changed();
    if (signature && target) this.pull(target, ticket);
  }
  refresh() {
    if (!this.target || !this.signature) return;
    const ticket = ++this.version;
    clearTimeout(this.timer); this.timer = undefined; this.changed();
    this.pull(this.target, ticket);
  }
  dispose() { this.show(undefined, false); this.version++; this.lane.close(); }
  private pull(target: ReminderTarget, ticket: number) {
    const current = () => this.version === ticket && !!this.signature;
    const schedule = (ms: number) => { if (current()) this.timer = setTimeout(() => this.pull(target, ticket), Math.max(1000, Math.min(REMINDER_MS, ms))); };
    void this.lane.read(() => current() ? this.read(target) : Promise.reject("Folder changed.")).then((reply) => {
      if (!current()) return;
      this.changed(Number.isSafeInteger(reply.count) && reply.count! >= 0 ? reply.count! : undefined);
      schedule(Number.isFinite(reply.retry_ms) ? reply.retry_ms : REMINDER_MS);
    }, () => { if (current()) { this.changed(); schedule(REMINDER_MS); } });
  }
}

/** Qt's disabled reasons and advisory language in the mockup's button. */
export function cleanupReminderView(state: unknown, current: boolean, common: string, linked: boolean, count?: number) {
  const enabled = current && state === "ok" && !!common && linked;
  const ready = enabled ? count ?? 0 : 0;
  return {
    enabled,
    label: ready > 0 ? `Clean up (${ready})…` : "Clean up…",
    title: !enabled ? state !== "ok" || !common ? "Cleanup is available for Git repositories with linked worktrees." : linked ? "Waiting for a current worktree catalog." : "This repository has no linked worktrees."
      : ready === 1 ? "1 worktree can be removed without losing work. Review it first."
      : ready > 1 ? `${ready} worktrees can be removed without losing work. Review them first.`
      : "Review linked worktrees and remove the ones you no longer need.",
  };
}
