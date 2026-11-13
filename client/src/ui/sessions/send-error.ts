import { tr, trNoop } from "../../i18n";

export const NO_COLOR_SEND = trNoop("This agent was started with NO_COLOR, so Zerus cannot tell a queued message from a draft. Restart the session to fix sending.");

/** Translate the known refusal inside the direct or deferred CLI diagnostic. */
export function sendErrorText(text: string): string {
  return text.replace(NO_COLOR_SEND, tr(NO_COLOR_SEND));
}
