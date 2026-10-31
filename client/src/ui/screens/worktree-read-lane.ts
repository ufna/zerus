/** One owned catalog read plus the newest waiting context. Obsolete promises
 * still settle; changing a folder never frees a running child's slot early. */
export class WorktreeReadLane {
  private busy = false;
  private closed = false;
  private pending?: { run(): Promise<Record<string, unknown>>; resolve(value: Record<string, unknown>): void; reject(error: unknown): void };
  read(run: () => Promise<Record<string, unknown>>): Promise<Record<string, unknown>> {
    if (this.closed) return Promise.reject("Worktree panel closed.");
    return new Promise((resolve, reject) => {
      this.pending?.reject("Folder changed.");
      this.pending = { run, resolve, reject };
      this.pump();
    });
  }
  close() { this.closed = true; this.pending?.reject("Worktree panel closed."); this.pending = undefined; }
  private pump() {
    if (this.closed || this.busy || !this.pending) return;
    const job = this.pending; this.pending = undefined; this.busy = true;
    void Promise.resolve().then(job.run).then(job.resolve, job.reject).finally(() => { this.busy = false; this.pump(); });
  }
}
