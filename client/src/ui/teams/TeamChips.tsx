import { Show } from "solid-js";
import { tr, trc } from "../../i18n";
import { Icon } from "../lib/icons";
import { AgentTile } from "../sessions/SessionCard";
import { agentKind } from "../../store/catalog";
import { roleStyle } from "./roles";
import { stageLine } from "./task-line";
import { roundDisagrees, targetDisagrees } from "./pipeline-view";
import "../styles/team-chips.css";
import type { StageView, TaskView } from "./types";

export function roleIcon(role: string, lead = false): string {
  if (lead) return "crown";
  const words = role.toLowerCase().split(/[^a-z0-9]+/);
  if (words.some((word) => ["lead", "coordinator"].includes(word))) return "crown";
  if (words.some((word) => ["reviewer", "review"].includes(word))) return "search-check";
  if (words.some((word) => ["test", "tester", "qa"].includes(word))) return "flask-conical";
  if (words.some((word) => ["frontend", "ui", "web"].includes(word))) return "panels-top-left";
  if (words.some((word) => ["backend", "server"].includes(word))) return "server";
  return "code";
}
export const stageIcon = (stage: Pick<StageView, "kind">) => stage.kind === "test" ? "flask-conical" : stage.kind === "land" ? "git-merge" : stage.kind === "review" ? "search-check" : "workflow";
// A stage chip names the stage as a noun; the shared "Review" and "Test" read
// as verbs in Russian, so the chip uses its own disambiguation (zerus-rzvct G7).
export const stageLabel = (stage: Pick<StageView, "kind" | "role">) => stage.kind === "review" ? trc("team stage", "Review") : stage.kind === "test" ? trc("team stage", "Test") : stage.kind === "land" ? tr("Landing") : roleLabel(stage.role);
/** A role chip's words: the built-in roles in the interface language, any
 * other role as its catalog ID. The chip's title keeps the ID. */
export function roleLabel(role: string): string {
  switch (role) {
    case "lead": return tr("Lead");
    case "developer": return tr("Developer");
    case "reviewer": return tr("Reviewer");
    case "tester": return tr("Tester");
    default: return role;
  }
}

export function RoleChip(props: { role: string; lead?: boolean; title?: string }) {
  return <span class="role-badge" classList={{ "team-member-lead": props.lead }} style={roleStyle(props.role, props.lead)} title={props.title ?? props.role}><Icon name={roleIcon(props.role, props.lead)} size={11} /><span>{roleLabel(props.role)}</span></span>;
}
export function TeamAgent(props: { agent: string }) {
  return <span class="team-agent" aria-label={props.agent}><AgentTile agent={agentKind(props.agent)} size={20} /></span>;
}
/** Why a stage's facts contradict each other or the card it is shown on, or "". */
export function stageDisagreement(stage: StageView, rounds?: number | null, on?: Pick<TaskView, "node" | "kind">): string {
  if (on && targetDisagrees(on, stage)) return tr("This stage names task #%1, not this one.", stage.target!);
  if (roundDisagrees(stage, rounds)) return tr("This stage names round %1; the pipeline has %2.", stage.round!, rounds!);
  return "";
}
/** A stage's chip; `on` is the card it is shown on, when it is nested in one. */
export function StageChip(props: { stage: StageView; rounds?: number | null; on?: Pick<TaskView, "node" | "kind"> }) {
  const round = () => props.stage.round;
  const disagreement = () => stageDisagreement(props.stage, props.rounds, props.on);
  // A round the pipeline does not have is shown as unknown, never as rN/M.
  const roundText = () => !props.stage.kind || !round() || round()! <= 0 ? ""
    : roundDisagrees(props.stage, props.rounds) ? tr("r?/%1", props.rounds!)
    : props.rounds && props.rounds > 0 ? tr("r%1/%2", round()!, props.rounds) : tr("Round %1", round()!);
  return <span class="team-stage-chip" data-stage={props.stage.node} data-kind={props.stage.kind || "role"} data-state={props.stage.state}
    data-inconsistent={disagreement() ? "true" : undefined} style={roleStyle(props.stage.role)}
    title={disagreement() ? `${stageLine(props.stage)}\n${disagreement()}` : stageLine(props.stage)}>
    <Icon name={disagreement() ? "triangle-alert" : stageIcon(props.stage)} size={11} /><span>{stageLabel(props.stage)}</span>
    <Show when={props.stage.state === "accepted"} fallback={<Show when={roundText()}><span>{roundText()}</span></Show>}><Icon name="check" size={11} /></Show>
    <Show when={props.stage.state === "pending" || props.stage.state === "waiting"}><Icon name="clock" size={10} /></Show>
  </span>;
}
export function TeamChip(props: { name: string }) {
  return <span class="team-identity"><Icon name="users" size={12} /><span class="ell">{props.name}</span></span>;
}
