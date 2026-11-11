import { Show, type JSX } from "solid-js";
import { tr } from "../../i18n";
import { Icon } from "../lib/icons";

/** The New team page's frame: the head and the form, as a page or in the
 * lazy dialog. The fork's drafts list and settings dialog are M2 and are not
 * here. */
export function NewTeamPage(props: { onClose?: () => void; children: JSX.Element }): JSX.Element {
  return (
    <div class="newteam-page" data-new-team>
      <header class="nt-page-head">
        <Icon name="users" size={18} /><h1 id="new-team-title">{tr("New team")}</h1>
        <Show when={props.onClose}>
          {(close) => <button type="button" class="icon-btn" aria-label={tr("Close")} onClick={close()}><Icon name="x" size={16} /></button>}
        </Show>
      </header>
      {props.children}
    </div>
  );
}
