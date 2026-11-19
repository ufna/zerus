import { For, Show, createEffect, createMemo, createSignal, on, onCleanup } from "solid-js";
import { tr } from "../../i18n";
import { readTaskEvidence, type EvidencePort, type EvidenceStage, type TaskEvidenceView } from "../../store/teamEvidence";
import { waitingNote } from "./decisions";
import type { TaskView } from "./types";
import "../styles/team-evidence.css";

// The evidence beside a task's decision (zerus-5gcbh): the member's latest
// report and each stage's latest verdict (who gave it, passed or returned, its
// findings), from one bounded read for the task the inspector shows. It reads
// again when the task or the team's revision changes; an earlier task's
// answer never shows. A stage card shows its own facts and reads nothing.

type Shown = { node: number; loading: boolean; evidence?: TaskEvidenceView; error?: string };

function stageTitle(stage: EvidenceStage): string {
  switch (stage.kind) {
    case "review": return tr("Review, round %1", stage.round);
    case "test": return tr("Test, round %1", stage.round);
    case "land": return tr("Landing, round %1", stage.round);
    default: return tr("Stage, round %1", stage.round);
  }
}
function verdictLabel(stage: EvidenceStage): string {
  const verdict = stage.verdict;
  if (!verdict) return tr("No verdict yet");
  if (verdict.member) return verdict.pass ? tr("Passed by %1", verdict.member) : tr("Returned by %1", verdict.member);
  return verdict.pass ? tr("Passed") : tr("Returned");
}

export function TaskEvidence(props: { team: string; card: TaskView; revision: number; read?: EvidencePort; onUpdating?: (updating: boolean) => void }) {
  const [shown, setShown] = createSignal<Shown>();
  let generation = 0;
  const load = (node: number) => {
    const asked = ++generation;
    // The same task's evidence stays in view while a newer revision is read.
    setShown((previous) => previous?.node === node && previous.evidence ? previous : { node, loading: true });
    const port = props.read ?? readTaskEvidence;
    Promise.resolve().then(() => port(props.team, node)).then(
      (evidence) => { if (asked === generation) setShown({ node, loading: false, evidence }); },
      (error: unknown) => { if (asked === generation) setShown({ node, loading: false, error: error instanceof Error ? error.message : String(error) }); },
    );
  };
  // A refreshed view hands a new card object for the same task; only a new
  // team, task or revision reads again.
  const node = createMemo(() => props.card.kind === "stage" ? 0 : props.card.node);
  createEffect(on([() => props.team, node, () => props.revision], ([, asked]) => {
    if (asked) load(asked);
    else { generation++; setShown(undefined); }
  }));
  // Evidence of another revision than the decision shown (kept in view while
  // the newer one is read, or newer than the board) is marked as updating,
  // and the inspector offers no Accept or Return beside it (review of #2531).
  const updating = createMemo(() => {
    const state = shown();
    return props.card.kind !== "stage" && !!state?.evidence && state.node === node() && state.evidence.revision !== props.revision;
  });
  createEffect(() => props.onUpdating?.(updating()));
  onCleanup(() => { generation++; props.onUpdating?.(false); });
  return (
    <Show when={props.card.kind !== "stage" && shown()}>
      {(state) => (
        <section class="team-evidence" classList={{ "team-evidence-updating": updating() }} data-task-evidence
          data-evidence-updating={updating() ? "" : undefined} aria-label={tr("Report and verdicts")} aria-busy={updating()}>
          <h4>{tr("Report and verdicts")}</h4>
          <Show when={updating()}><p class="dim" role="status" data-evidence-reading>{tr("Reading the evidence…")}</p></Show>
          <Show when={!state().loading} fallback={<p class="dim">{tr("Reading the evidence…")}</p>}>
            <Show when={state().evidence} fallback={
              <div class="team-evidence-error" role="alert">
                <span data-evidence-error>{tr("The evidence could not be read: %1", state().error ?? "")}</span>
                <button type="button" class="btn small" data-evidence-retry onClick={() => load(state().node)}>{tr("Read again")}</button>
              </div>
            }>
              {(evidence) => (
                <>
                  <Show when={evidence().report} fallback={<Show when={!evidence().stages.length}><p class="dim">{tr("No report or verdict yet.")}</p></Show>}>
                    {(report) => (
                      <div class="team-evidence-item">
                        <header>
                          <b data-evidence-report-by>{report().member ? tr("Report by %1", report().member!) : tr("Report")}</b>
                          <span class="mono dim" data-evidence-commit title={report().commit}>{report().commit.slice(0, 12)}</span>
                        </header>
                        <pre class="team-drawer-text" data-evidence-report>{report().text}</pre>
                        <Show when={report().text_truncated}><p class="dim" data-evidence-cut>{tr("Only the beginning of this text is shown.")}</p></Show>
                        <For each={report().notes}>{(note) => <p class="dim">{waitingNote(note)}</p>}</For>
                      </div>
                    )}
                  </Show>
                  <For each={evidence().stages}>
                    {(stage) => (
                      <div class="team-evidence-item" data-evidence-stage-node={stage.node}>
                        <header>
                          <b data-evidence-stage>{stageTitle(stage)}</b>
                          <span class="team-evidence-verdict" data-evidence-verdict data-pass={stage.verdict ? String(stage.verdict.pass) : undefined}>{verdictLabel(stage)}</span>
                        </header>
                        <Show when={stage.verdict}>
                          {(verdict) => (
                            <>
                              <pre class="team-drawer-text" data-evidence-findings>{verdict().text}</pre>
                              <Show when={verdict().text_truncated}><p class="dim" data-evidence-cut>{tr("Only the beginning of this text is shown.")}</p></Show>
                            </>
                          )}
                        </Show>
                      </div>
                    )}
                  </For>
                  <Show when={evidence().more_stages}><p class="dim" data-evidence-more>{tr("Earlier stages are not shown.")}</p></Show>
                </>
              )}
            </Show>
          </Show>
        </section>
      )}
    </Show>
  );
}
