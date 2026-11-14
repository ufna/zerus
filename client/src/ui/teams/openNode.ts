import { createSignal } from "solid-js";

// A node a Team tab should select when it opens: Waiting for you's "Open task"
// sets it before switching to the team's lead session, and the workspace takes
// it once. A card's own decision belongs to the Waiting page; the decision on
// reported work belongs to the board.
export const [openTeamNode, setOpenTeamNode] = createSignal<{ team: string; node: number } | null>(null);
