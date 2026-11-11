import { For, Show } from "solid-js";
import { tr } from "../../i18n";
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
export function MemberActions(props: { member: MemberView; busy: boolean; onAction: (kind: string, member: string) => void }) {
  return (
    <Show when={memberActions(props.member).length}>
      <span class="team-member-actions" data-member-actions>
        <For each={memberActions(props.member)}>
          {(action) => (
            <button type="button" class="btn small" classList={{ danger: action.danger }} disabled={props.busy}
              data-member-action={action.id} onClick={() => props.onAction(action.id, props.member.id)}>{action.text}</button>
          )}
        </For>
      </span>
    </Show>
  );
}
