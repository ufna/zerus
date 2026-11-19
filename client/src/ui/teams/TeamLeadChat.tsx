import { Show } from "solid-js";
import { tr } from "../../i18n";
import { selected, subagentOpen } from "../../store/ui";
import { machineCards } from "../../store/machines";
import { composer } from "../../store/composer";
import { Composer } from "../sessions/Composer";
import { leadChatCause } from "./lead-chat";
import type { TeamView } from "./types";

/** Reuse the selected lead's actual composer, including its saved draft,
 * questions, exact identity guards, durable send UUID and uncertain outcome.
 * Switching drawers neither copies a draft nor constructs a second sender. */
export function TeamLeadChat(props: { view: TeamView }) {
  /** Why the chat cannot send now, named with its fix (zerus-63rru.199 C4). */
  const cause = () => {
    const row = selected();
    return leadChatCause(props.view, row, machineCards[row?.computer ?? ""]?.machine_id, composer.view()?.key, !!subagentOpen())
      ?? (composer.view()?.loaded ? undefined : tr("The lead's conversation is still loading."));
  };
  const ready = () => cause() === undefined;
  return <section class="team-lead-chat" aria-label={tr("Lead chat")}>
    <Show when={ready()} fallback={<p class="dim" data-lead-chat-cause>{tr("The lead's current conversation is not available for sending.")} {cause()}</p>}>
      <p class="dim">{tr("Messages go to the lead's current conversation. Its saved draft is shared with Activity.")}</p>
      <div class="cmp-col"><Composer /></div>
    </Show>
  </section>;
}
