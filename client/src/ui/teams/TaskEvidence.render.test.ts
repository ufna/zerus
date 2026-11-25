// @vitest-environment happy-dom
// zerus-5gcbh: the evidence beside a task's decision in the Team tab's
// inspector. The evidence is the core's projection of the product-written
// reply (crates/team-plan/fixtures/cli-evidence.json: m1's report and m3's
// passing review); the port stands for the one bounded `hgs team evidence`
// read. Tests named `handwritten_*` change that evidence on purpose.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { TaskEvidence } from "./TaskEvidence";
import evidence from "../../../core/src/teams/fixtures/evidence.json";
import { teamView } from "./fixtures";
import type { TaskEvidenceView } from "../../store/teamEvidence";
import type { TaskView } from "./types";

const TEAM = evidence.team;
const product = evidence as unknown as TaskEvidenceView;
const task = { ...teamView.groups.flatMap((group) => group.cards)[0], node: 1, kind: "task" } as TaskView;
let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });
const settle = async () => { for (let n = 0; n < 8; n++) await Promise.resolve(); };
function show(read: (team: string, node: number) => Promise<TaskEvidenceView>, card: TaskView = task, revision = product.revision) {
  const host = document.createElement("div"); document.body.append(host);
  const [current, setCard] = createSignal(card); const [rev, setRevision] = createSignal(revision);
  dispose = render(() => createComponent(TaskEvidence, { team: TEAM, get card() { return current(); }, get revision() { return rev(); }, read }), host);
  return { host, setCard, setRevision };
}
const text = (host: HTMLElement, selector: string) => [...host.querySelectorAll(selector)].map((node) => node.textContent);

it("shows the member's report and the reviewer's verdict with its findings beside the decision", async () => {
  const read = vi.fn(async () => product);
  const { host } = show(read);
  await settle();
  expect(read).toHaveBeenCalledExactlyOnceWith(TEAM, 1);
  expect(host.querySelector("[data-evidence-report-by]")?.textContent).toBe("Report by m1");
  expect(host.querySelector("[data-evidence-commit]")?.textContent).toBe("0123456789ab");
  expect(host.querySelector("[data-evidence-commit]")?.getAttribute("title")).toBe(product.report!.commit);
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe(product.report!.text);
  expect(text(host, "[data-evidence-stage]")).toEqual(["Review, round 1"]);
  expect(text(host, "[data-evidence-verdict]")).toEqual(["Passed by m3"]);
  expect(text(host, "[data-evidence-findings]")).toEqual([product.stages[0].verdict!.text]);
});

it("handwritten_a_returned_review_says_who_returned_it_and_why", async () => {
  const returned = { ...product, stages: [{ ...product.stages[0], verdict: { ...product.stages[0].verdict!, pass: false, text: "The empty label still overflows." } }] };
  const { host } = show(async () => returned);
  await settle();
  expect(text(host, "[data-evidence-verdict]")).toEqual(["Returned by m3"]);
  expect(text(host, "[data-evidence-findings]")).toEqual(["The empty label still overflows."]);
});

it("handwritten_a_stage_without_a_verdict_and_a_task_without_evidence_say_so", async () => {
  const waiting = { ...product, report: null, stages: [{ ...product.stages[0], verdict: null }] };
  let mounted = show(async () => waiting);
  await settle();
  expect(text(mounted.host, "[data-evidence-verdict]")).toEqual(["No verdict yet"]);
  expect(mounted.host.querySelector("[data-evidence-report]")).toBeNull();
  dispose?.(); document.body.replaceChildren();
  mounted = show(async () => ({ ...product, report: null, stages: [] }));
  await settle();
  expect(mounted.host.textContent).toContain("No report or verdict yet.");
});

it("handwritten_cut_texts_and_more_stages_are_marked", async () => {
  const cut = { ...product, more_stages: true, report: { ...product.report!, text_truncated: true }, stages: [{ ...product.stages[0], verdict: { ...product.stages[0].verdict!, text_truncated: true } }] };
  const { host } = show(async () => cut);
  await settle();
  expect(host.querySelectorAll("[data-evidence-cut]")).toHaveLength(2);
  expect(host.querySelector("[data-evidence-more]")?.textContent).toBe("Earlier stages are not shown.");
});

it("reads nothing for a stage card, and says why a read failed with a way to read again", async () => {
  const read = vi.fn(async () => product);
  let mounted = show(read, { ...task, kind: "stage" });
  await settle();
  expect(read).not.toHaveBeenCalled();
  expect(mounted.host.querySelector("[data-task-evidence]")).toBeNull();
  dispose?.(); document.body.replaceChildren();
  const failing = vi.fn().mockRejectedValueOnce(new Error("hgs refused: Task does not exist in this team.")).mockResolvedValueOnce(product);
  mounted = show(failing);
  await settle();
  expect(mounted.host.querySelector("[data-evidence-error]")?.textContent).toContain("Task does not exist in this team.");
  mounted.host.querySelector<HTMLButtonElement>("[data-evidence-retry]")!.click();
  await settle();
  expect(failing).toHaveBeenCalledTimes(2);
  expect(mounted.host.querySelector("[data-evidence-report]")?.textContent).toBe(product.report!.text);
});

it("reads again when the task or the team's revision changes and never shows an earlier task's answer", async () => {
  let release: ((value: TaskEvidenceView) => void) | undefined;
  const read = vi.fn((_team: string, node: number) => node === 1
    ? new Promise<TaskEvidenceView>((resolve) => { release = resolve; })
    : Promise.resolve({ ...product, node, report: { ...product.report!, text: "Second task's report." } }));
  const { host, setCard, setRevision } = show(read);
  setCard({ ...task, node: 2 });
  await settle();
  release!(product);
  await settle();
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe("Second task's report.");
  setRevision(product.revision + 1);
  await settle();
  expect(read.mock.calls.map((call) => call[1])).toEqual([1, 2, 2]);
});

it("a refreshed view of the same task at the same revision reads nothing again", async () => {
  const read = vi.fn(async () => product);
  const { setCard } = show(read);
  await settle();
  setCard({ ...task, title: "The same task, projected again" });
  await settle();
  expect(read).toHaveBeenCalledOnce();
});

it("a new revision keeps the shown evidence until the new answer arrives", async () => {
  let release: ((value: TaskEvidenceView) => void) | undefined;
  const read = vi.fn()
    .mockResolvedValueOnce(product)
    .mockImplementationOnce(() => new Promise<TaskEvidenceView>((resolve) => { release = resolve; }));
  const { host, setRevision } = show(read);
  await settle();
  setRevision(product.revision + 1);
  await settle();
  expect(read).toHaveBeenCalledTimes(2);
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe(product.report!.text);
  // Kept readable but marked as being read again (review of #2531).
  expect(host.querySelector("[data-task-evidence]")?.hasAttribute("data-evidence-updating")).toBe(true);
  expect(host.querySelector("[data-evidence-reading]")?.textContent).toBe("Reading the evidence…");
  release!({ ...product, revision: product.revision + 1, report: { ...product.report!, text: "The newer report." } });
  await settle();
  expect(host.querySelector("[data-evidence-report]")?.textContent).toBe("The newer report.");
  expect(host.querySelector("[data-evidence-reading]")).toBeNull();
});
