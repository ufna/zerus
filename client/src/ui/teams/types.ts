// The Team tab's view shapes. The core projects them once (client/core's
// teams view) and the store keeps the latest; this module aliases them for the
// views, so there is one definition of each.
import type { TeamCard, TeamGroup, TeamMember, TeamStage, TeamView } from "../../store/teams";

export type { TeamView } from "../../store/teams";
export type MemberView = TeamMember;
export type TaskView = TeamCard;
export type StageView = TeamStage;
export type GroupView = TeamGroup;

/** A member's session, as the core sends it. */
export interface SessionView {
  machine: string;
  session_id: string;
}
