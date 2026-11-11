import { For, Show, onCleanup } from "solid-js";
import { tr } from "../../i18n";
import { menu, setMenu, type MenuEntry } from "../../store/ui";
import { Icon } from "../lib/icons";
import { memberActions, taskButtons, type TaskButton } from "./actions";
import { barActions } from "./actions";
import type { MemberView, TaskView, TeamView } from "./types";

// The Team tab's action buttons. They only name the operation and call the view's
// one action callback; the store sends it and the core decides the outcome.

export function TeamBarActions(props: { view: TeamView; busy: boolean; onAction: (kind: string) => void }) {
  return (
    <span class="team-bar-actions" data-strip-actions>
      <For each={barActions(props.view)}>
        {(action) => (
          <button type="button" class="btn small" classList={{ danger: action.danger }} disabled={props.busy || action.disabled}
            data-strip-action={action.id} onClick={() => props.onAction(action.id)}>{action.text}</button>
        )}
      </For>
    </span>
  );
}

/** The decisions a card offers; a reason button asks for its text first. */
export function TaskActions(props: {
  card: TaskView;
  busy: boolean;
  onAction: (kind: string) => void;
  onReason: (button: TaskButton) => void;
}) {
  return (
    <span class="team-finding-actions" data-task-actions>
      <For each={taskButtons(props.card)}>
        {(button) => (
          <button type="button" class="btn small" classList={{ danger: button.danger, primary: button.id === "accept" }}
            disabled={props.busy} data-task-action={button.id}
            onClick={() => (button.reason ? props.onReason(button) : props.onAction(button.id))}>{button.text}</button>
        )}
      </For>
    </span>
  );
}

/** A member's operations in its card. */
export function MemberActions(props: { member: MemberView; busy: boolean; onAction: (kind: string, member: string) => void; onDetails?: () => void; detailsOpen?: boolean }) {
  let items: MenuEntry[] | undefined;
  onCleanup(() => { if (items && menu()?.items === items) setMenu(undefined); });
  return (
    <Show when={memberActions(props.member).length || props.onDetails}>
      <span class="team-member-actions" data-member-actions>
        <button type="button" class="icon-btn small team-member-more" aria-label={tr("Member actions")} title={tr("Member actions")} aria-haspopup="menu" disabled={props.busy}
          onClick={(event) => {
            const trigger = event.currentTarget, box = trigger.getBoundingClientRect(), memberId = props.member.id;
            items = [
              ...(props.onDetails ? [{ label: tr("Member details"), icon: "info", checked: props.detailsOpen, action: () => { trigger.focus(); props.onDetails?.(); } }] : []),
              ...memberActions(props.member).map((action) => ({
              label: action.text, icon: action.id.startsWith("pause") ? "pause" : action.id.startsWith("resume") ? "play" : action.id.startsWith("stop") ? "square" : "trash",
              danger: action.danger, disabled: props.busy || action.disabled, hint: action.reason,
              action: () => { if (props.busy || action.disabled) return; trigger.focus(); props.onAction(action.id, memberId); },
            }))];
            setMenu({ x: box.right - 200, y: box.bottom + 4, items });
          }}><Icon name="ellipsis" size={14} /></button>
      </span>
    </Show>
  );
}
