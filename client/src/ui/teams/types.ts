// Optional pipeline facts are declared on the store's view types only; the UI
// keeps no parallel schema. Missing/null fields retain the M1 role-name fallback.
import type { TeamCard, TeamGroup, TeamMember, TeamStage } from "../../store/teams";
export type { TeamView } from "../../store/teams";
export type StageKind = "review" | "test" | "land";
export type StageView = TeamStage;
export type TaskView = TeamCard;
export type MemberView = TeamMember;
export type GroupView = TeamGroup;
export interface SessionView { machine: string; session_id: string }
