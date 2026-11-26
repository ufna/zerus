import { tr, trNoop } from "../../i18n";

export const NO_COLOR_SEND = trNoop("This agent was started with NO_COLOR, so Zerus cannot tell a queued message from a draft. Restart the session to fix sending.");
export const REFUSED_SEND = trNoop("Message was not sent. hgs refused before typing: %1");
const PREFIX = REFUSED_SEND.replace("%1", "");
const EARLIER_INPUT_REFUSALS = [
  "previous input has not been confirmed yet: another message is being entered into this session; try again in a few seconds",
  "previous input has not been confirmed by the agent; a busy agent may keep it queued until it accepts it. Send again once it does, or inspect Terminal",
  "previous input has not been confirmed by the agent; inspect Terminal. If its input is empty, use Release unconfirmed input there, or hgs acknowledge-input from a terminal outside Zerus",
];

export const EARLIER_INPUT_PENDING = trNoop("Wait for the agent to accept the earlier message, or check Terminal.");
export const EARLIER_INPUT_CLEARED = trNoop("The earlier input is no longer pending. Your draft is kept.");
export const EARLIER_INPUT_UNOBSERVED = trNoop("Check Terminal. Send and Retry stay paused until this host confirms the earlier input cleared. Update hgs on older hosts.");
export const DRAFT_KEPT = trNoop("Your draft is kept.");

export function earlierInputRefusal(text: string): boolean {
  return text.startsWith(PREFIX) && EARLIER_INPUT_REFUSALS.includes(text.slice(PREFIX.length));
}

/** Presentation only. The caller must already have a definite refusal. */
export function earlierInputDraftStatus(text: string, pending: boolean | undefined): string {
  if (!earlierInputRefusal(text)) return "";
  if (pending === false) return tr(EARLIER_INPUT_CLEARED);
  if (pending === true) return tr("%1 %2", tr(DRAFT_KEPT), tr(EARLIER_INPUT_PENDING));
  return tr(DRAFT_KEPT);
}

const NATIVE_REFUSALS = [
  trNoop("Codex is waiting for your sandbox setup choice. Choose an option in Activity or Terminal before sending a message."),
  NO_COLOR_SEND,
  trNoop("terminal has a draft, dialog, or unrecognized prompt; open Terminal and return to an empty agent prompt"),
  trNoop("agent busy: terminal queue prompt is unrecognized or contains a draft; wait for the agent or open Terminal; nothing was submitted"),
  trNoop("Claude is waiting for tool or safety approval. Review the request in Activity or Terminal before sending a message."),
  trNoop("Claude is waiting for a conversation resume choice. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Claude is waiting for a usage-limit choice. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Claude is waiting for folder trust approval. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Codex is waiting for folder trust approval. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Codex is waiting for hook trust review. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Codex is waiting for an update choice. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Codex is waiting for your rate-limit model choice. Choose an option in Activity or open Terminal before sending a message."),
  trNoop("Codex is waiting for your paused goal choice. Choose an option in Activity or Terminal before sending a message."),
  trNoop("Kimi is waiting for folder trust approval. Choose an option in Activity or Terminal before sending a message."),
];

/** Translate the known refusal inside the direct or deferred CLI diagnostic. */
export function sendErrorText(text: string): string {
  if (text.startsWith(PREFIX)) {
    const reason = text.slice(PREFIX.length);
    const known = NATIVE_REFUSALS.find((source) => reason === source);
    return tr(REFUSED_SEND, known ? tr(known) : reason);
  }
  return text.replace(NO_COLOR_SEND, tr(NO_COLOR_SEND));
}
