import { createSignal } from "solid-js";
import type { TeamView } from "./teams";

export type TeamTaskSource = Pick<TeamView, "bar" | "routing_pipelines">;

/** One explicit request, consumed by the existing per-project New task form. */
export const [teamTaskCreation, setTeamTaskCreation] = createSignal<{ project: string; team: string; fromPage?: TeamTaskSource }>();
