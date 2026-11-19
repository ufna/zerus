import { Show } from "solid-js";
import { tr } from "../../i18n";
import { page, selected, subagentOpen } from "../../store/ui";
import { machineCards } from "../../store/machines";
import { orderedRows } from "../../store/catalog";
import { composer } from "../../store/composer";
import { Composer } from "../sessions/Composer";
import { leadChatProblem, type LeadChatProblem } from "./lead-chat";
import { openTeamLead } from "./lead-navigation";
import { rowIsSession } from "./session-binding";
import type { TeamView } from "./types";

/** Each refusal's cause and the way out (audit C4). */
const cause = (problem: LeadChatProblem | "loading") => ({
  no_row: () => tr("Open the lead's session to talk to it."),
  subagent: () => tr("Close the subagent view to talk to the lead."),
  archived: () => tr("The lead's session is archived. Restore it to talk to the lead."),
  no_conversation: () => tr("The lead's conversation has not started yet. Try again when the session runs."),
  no_lead: () => tr("The team's lead has no session yet."),
  unknown_computer: () => tr("The computer of the lead's session is not identified yet. Try again when it is connected."),
  other_computer: () => tr("The lead's session runs on another computer. Talk to it there."),
  other_session: () => tr("The team's lead session has changed. Open it from Teams again."),
  composer: () => tr("The lead's conversation is still loading. Select the lead's session again if this stays."),
  loading: () => tr("The lead's conversation is still loading. Select the lead's session again if this stays."),
})[problem]();

/** Reuse the selected lead's actual composer, including its saved draft,
 * questions, exact identity guards, durable send UUID and uncertain outcome.
 * Switching drawers neither copies a draft nor constructs a second sender.
 * On the team page (zerus-63rru.198) it is never the lead's composer: it
 * leads to the lead's session instead. */
export function TeamLeadChat(props: { view: TeamView }) {
  const problem = (): LeadChatProblem | "loading" | undefined => {
    const row = selected();
    return leadChatProblem(props.view, row, machineCards[row?.computer ?? ""]?.machine_id, composer.view()?.key, !!subagentOpen())
      ?? (composer.view()?.loaded ? undefined : "loading");
  };
  return <section class="team-lead-chat" aria-label={tr("Lead chat")}>
    <Show when={page() !== "team"} fallback={<PageLeadChat view={props.view} />}>
      <Show when={!problem()} fallback={<p class="dim" data-lead-chat-reason>{cause(problem()!)}</p>}>
        <p class="dim">{tr("Messages go to the lead's current conversation. Its saved draft is shared with Activity.")}</p>
        <div class="cmp-col"><Composer /></div>
      </Show>
    </Show>
  </section>;
}

/** The team page's Lead chat: the lead's own session, when it is listed. */
function PageLeadChat(props: { view: TeamView }) {
  const lead = () => props.view.members.find((member) => member.id === props.view.bar.lead && member.lead)?.session ?? undefined;
  const row = () => { const session = lead(); return session ? orderedRows().find((candidate) => rowIsSession(candidate, session)) : undefined; };
  return <>
    <p class="dim">{tr("Lead chat lives in the lead's session. Open it to talk to the lead.")}</p>
    <Show when={row()} fallback={<p class="dim" data-lead-chat-reason>{lead() ? tr("The team's lead session is not on the session list.") : tr("The team's lead has no session yet.")}</p>}>
      {(found) => <div><button type="button" class="btn small" data-open-lead-session
        onClick={() => { const session = lead(); if (session) openTeamLead({ id: props.view.bar.id, lead_session: { machine: session.machine, session_id: session.session_id } }, found()); }}>{tr("Open lead session")}</button></div>}
    </Show>
  </>;
}
