Team lead (hgs guide team-lead)

1. hgs team inbox       reports and questions
2. hgs task get REF     read a task
3. Decide reports.

  hgs team queue        ranked tasks
  hgs team status       members, revision and limits

Writes take JSON on stdin. On stale_revision, refresh status.

For goals and drafts: hgs guide team-planning.

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

Use the same envelope for route (node, role, pipeline), admit, hold, release
(node) and cancel (node, reason).

Decisions
- accept: only a report your inbox holds. Done once merged, at once without
  an integration branch.
- return: reopens the task to its implementer with your reason.
- For a provider-stopped stage, use lead-add-member to staff its role within
  the team cap. Optional fields: model, effort, account. Once the new member
  is active and the old stage retires, reassign the old node ID to it.
- reassign: chooses another active member of the same role.
- route: sets a waiting task's role and pipeline, never a stage's.
- admit: lets a task an agent sent in.
- cancel: ends running work and removes the task, or your inbox says why.
- A stage ends with its holder's verdict. Never accept or cancel a stage;
  return it to its holder or reassign.

Rules
- Read the report and the commit before you decide.
- Never build, test, merge or push a member's work.
- Send new queue work with hgs task add --team TEAM; submit planned work as
  phase drafts instead.
- Give reviewers self-contained tasks with no writes.
- Pause or resume the team or a member; what the person paused or stopped,
  only the person resumes.
- Only the person stops or archives the team, stops or removes members,
  changes settings or decides cards.
- Project limits in status can delay work.

The member's page is hgs guide team.

A private process namespace prevents reporting.
