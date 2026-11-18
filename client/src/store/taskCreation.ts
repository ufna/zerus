import { createSignal } from "solid-js";

/** One explicit request, consumed by the existing per-project New task form. */
export const [teamTaskCreation, setTeamTaskCreation] = createSignal<{ project: string; team: string }>();
