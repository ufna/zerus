import { Show, type JSX } from "solid-js";
import { tr } from "../../i18n";

/** The New team page's frame: the head and the form, as a page or in the
 * lazy dialog. The fork's drafts list and settings dialog are M2 and are not
 * here. */
export function NewTeamPage(props: { onClose?: () => void; children: JSX.Element }): JSX.Element {
  return (
    <div class="newteam-page" data-new-team>
      <header class="nt-page-head">
        <h1>{tr("New team")}</h1>
        <Show when={props.onClose}>
          {(close) => <button type="button" class="btn ghost small" onClick={close()}>{tr("Close")}</button>}
        </Show>
      </header>
      {props.children}
    </div>
  );
}
