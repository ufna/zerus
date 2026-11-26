import { createSignal } from "solid-js";
import { readTaskPage, type TaskScope } from "../bridge";
import { decodeTaskRows, object, TASK_PAGE, text, type Task } from "../ui/screens/tasks-ui";

export const REVIEW_LANES = ["person", "technical", "triage", "team"] as const;
export type ReviewLane = typeof REVIEW_LANES[number];
type Target = { project: string; scope?: TaskScope; query: string; lane: ReviewLane | "all" };
type ReviewPage = {
  counts: Record<ReviewLane, number>;
  damaged: number;
  next: string;
  previous: string;
  position: number;
  total: number;
  revision: string;
  rows: Task[];
};

/** One running bounded read. A newer scope or Watch publication supersedes it. */
export function createReviewQueue() {
  const [page, setPage] = createSignal<ReviewPage>();
  const [loading, setLoading] = createSignal(false);
  const [failed, setFailed] = createSignal(false);
  let target: Target | undefined, generation = 0, running = false, pending = false;

  const read = async (cursor?: { after?: string; before?: string; revision: string }) => {
    if (!target || running) return;
    const request = target, mine = generation;
    running = true;
    pending = false;
    setLoading(true);
    try {
      const reply = object(await readTaskPage(
        request.project, "review", request.lane === "all" ? 1 : TASK_PAGE,
        cursor?.after || null, request.scope, request.query, request.lane, cursor?.before,
      ));
      if (mine !== generation) return;
      const counts = object(reply.review_counts);
      const meta = object(reply.page), revision = text(reply.revision);
      const position = meta.position, total = meta.total;
      const damaged = reply.damaged_total;
      if (reply.review_lane !== request.lane || !revision || !Array.isArray(reply.tasks)
        || typeof position !== "number" || typeof total !== "number"
        || typeof damaged !== "number" || !Number.isSafeInteger(damaged) || damaged < 0
        || !REVIEW_LANES.every((lane) => typeof counts[lane] === "number" && Number.isSafeInteger(counts[lane]) && (counts[lane] as number) >= 0)) {
        throw new Error();
      }
      if (cursor && cursor.revision !== revision) {
        setPage(undefined);
        pending = true;
        return;
      }
      const rows = decodeTaskRows(reply.tasks, "review");
      setPage({
        counts: counts as ReviewPage["counts"], damaged, rows, revision, position, total,
        next: text(meta.next), previous: text(meta.previous),
      });
      setFailed(false);
    } catch {
      if (mine === generation) { if (!cursor) setPage(undefined); setFailed(true); }
    } finally {
      running = false;
      if (pending) void read();
      else setLoading(false);
    }
  };

  return {
    page, loading, failed,
    show(next?: Target) {
      target = next;
      generation++;
      setPage(undefined);
      setFailed(false);
      pending = !!next;
      if (pending) void read();
    },
    step(direction: "next" | "previous") {
      const current = page(), cursor = current?.[direction];
      if (current && cursor) void read({
        revision: current.revision,
        ...(direction === "next" ? { after: cursor } : { before: cursor }),
      });
    },
  };
}
