import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "../bridge";
import type { ServiceNote } from "./teams";

// The evidence beside one task's decision (zerus-5gcbh): the core's projection
// of one bounded `hgs team evidence --team ID --node N` read. Nothing is cached:
// the inspector reads it for the task it shows and reads again when the task or
// the team's revision changes.
export interface EvidenceVerdict { generation: number; member: string | null; pass: boolean; text: string; text_truncated: boolean }
export interface EvidenceStage { node: number; kind: string | null; round: number; state: string; verdict: EvidenceVerdict | null }
export interface EvidenceReport { generation: number; member: string | null; commit: string; text: string; text_truncated: boolean; notes: ServiceNote[] }
export interface TaskEvidenceView { team: string; node: number; revision: number; report: EvidenceReport | null; stages: EvidenceStage[]; more_stages: boolean }
export type EvidencePort = (team: string, node: number) => Promise<TaskEvidenceView>;

export const readTaskEvidence: EvidencePort = (team, node) => {
  if (!inTauri()) return Promise.reject(new Error("Teams need a live hgs event stream."));
  // An answer of no shape is a failed read, never empty evidence.
  return Promise.resolve(invoke<TaskEvidenceView>("teams_task_evidence", { team, node }))
    .then((answer) => answer ?? Promise.reject(new Error("The evidence answer is empty.")));
};
