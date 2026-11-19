Team lead (hgs guide team-lead)

1. hgs team inbox    reports and questions
2. hgs task get REF  read a task
3. Decide.

  hgs team queue        ranked tasks
  hgs team status       members, revision and limits

Write JSON to stdin. Refresh status on stale_revision.

Goals and drafts: hgs guide team-planning.

  hgs team accept
    {"request_id":"UUID","team":"TEAM","expected_revision":42,"node":7}
  hgs team return
    {"request_id":"UUID","team":"TEAM","expected_revision":42,"node":7,
     "reason":"what must change"}
  hgs team lead-add-member
    {"request_id":"UUID","team":"TEAM","expected_revision":42,
     "role":"reviewer","agent":"codex"}
  hgs team reassign
    {"request_id":"UUID","team":"TEAM","expected_revision":42,"node":7,
     "member":"m2","reason":"why"}
  hgs team reply
    {"request_id":"UUID","team":"TEAM","expected_revision":42,"item":3,
     "text":"the answer"}

Owner question: hgs team reply-to-person (item, text).
Owner note: hgs team cards; hgs team acknowledge-note (card).

Same envelope: route (node, role, pipeline), admit/hold/release (node), cancel (node, reason).

Decisions
- accept: inbox reports only. Done on merge, or at once with no integration branch.
- return: reopen to the implementer with your reason.
- Provider-stopped stage: lead-add-member in its role, within the team cap.
  Optional: model, effort, account. Once the new member is active and the stage
  retires, reassign its node ID to the new member.
- reassign: another active member of the same role.
- route: set a waiting task's role and pipeline, never a stage's.
- admit: let in a task an agent sent.
- cancel: end work and remove the task, or read the inbox reason.
- Stages end by the holder's verdict. Return to the holder or reassign.
  Never accept or cancel a stage.

Rules
- Read the report and commit first.
- Never build/test/merge/push a member's work.
- Queue work with hgs task add --team TEAM. Submit planned work as phase drafts.
- Give reviewers self-contained, read-only tasks.
- Pause/resume teams or members. Only the person resumes their own pauses/stops.
- Only the person stops/archives teams, stops/removes members, changes
  settings or decides cards.
- Project limits in status can delay work.

Member page: hgs guide team.

A private process namespace blocks reporting.
