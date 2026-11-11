import { Show } from "solid-js";
import { tr } from "../../i18n";
import { accountLabel } from "./members";
import { memberStatus } from "./team-model";
import { roleStyle } from "./roles";
import { MemberActions } from "./TeamActions";
import type { MemberView } from "./types";

// One member of the team, as the Team tab's column shows it: its role and
// colour, its state, the agent and account it runs on and its bound session.
// The operations are the M1 ones only (pause, resume, stop, remove).

export function MemberCard(props: { member: MemberView; busy: boolean; onAction: (kind: string, member: string) => void }) {
  const status = () => memberStatus(props.member);
  const role = () => props.member.role || props.member.id;
  return (
    <div class="team-member-card" classList={{ "team-lead": props.member.lead }} data-member={props.member.id} data-state={status().state}>
      <div class="team-member-line">
        <span class="team-member-mark" style={roleStyle(role(), props.member.lead)} aria-hidden="true" />
        <b class="team-member-name ell">{props.member.id}</b>
        <span class="role-badge" style={roleStyle(role(), props.member.lead)}>{tr(role())}</span>
        <Show when={props.member.lead}><span class="team-member-lead">{tr("lead")}</span></Show>
        <span class="tm-state-badge" data-tone={status().tone}>{status().label}</span>
      </div>
      <div class="team-member-main dim">
        <span>{props.member.agent}</span>
        <span class="team-account">{accountLabel(props.member)}</span>
      </div>
      <Show when={props.member.session}>
        {(session) => <div class="team-member-session mono" title={session().session_id}>{session().session_id}</div>}
      </Show>
      <MemberActions member={props.member} busy={props.busy} onAction={props.onAction} />
    </div>
  );
}
