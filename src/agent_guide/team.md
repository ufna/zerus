Team member (hgs guide team)

You are a team member: a Zerus session with a role. Your task uses hgs task;
the team names your work and lets you ask the lead.

  hgs team current        the task you hold now: node, generation, round,
                          state, and the team's revision
  hgs task get REF        read your task, its prompt and its write set
  hgs task report-attempt report with the command pinned in your prompt

During reassignment, current follows the holder until the reopen is reserved.

Ask takes one JSON request on stdin and prints one JSON reply. Take revision,
node and generation from hgs team current:

  hgs team ask
    {"request_id":"UUID","team":"TEAM","expected_revision":42,
     "node":7,"generation":1,"text":"what blocks the attempt"}

UUID is a fresh identifier and TEAM is the team current names. The lead's
answer appears under that node in hgs team current, and a message tells you
when one waits. A request that names its own actor or the operation is refused.

A stage task (a review or a test) ends with its holder's verdict on stdin:

  hgs team verdict
    {"request_id":"UUID","team":"TEAM","expected_revision":42,
     "node":7,"generation":1,"pass":true,"text":"Checks passed."}

Use the stage's node and generation from current. Set pass to false to return
the task, and give the exact findings in text. A report on a stage does not
advance the pipeline; it only reaches the lead. Only the stage's holder may give its verdict.

Do
- Keep your terminal at an empty prompt: while it holds input, the team
  hands your task to another member.
- Work only on the task you hold, in your own worktree, inside its write set.
- Start from the base commit the prompt names and commit your work: the team
  takes your worktree's HEAD. Report what you changed and how you checked it.
- Ask when the attempt is blocked, not to plan.

Never
- Never report a task you do not hold, and never act as the lead.
- Never merge, push, or edit another member's work or worktree.
- Never send, move, cancel or accept a team task yourself; the team does that.
- Never name another session as yourself; the session ID binds the attempt.

The task's prompt is the authority for the task; this page is how the team
talks to you. Your lead reads hgs guide team-lead.

Team commands in a private process namespace are refused; run outside it or turn it off.
