# Agent Teams

Agent Teams are a preview. They are off on every computer until a person turns
them on, and this guide describes the first milestone: a team that takes tasks
from a queue. The commands, their replies and their codes are in the
[Agent Teams reference](reference.md#agent-teams).

## What a team is

A team is a lead session and member sessions that work one queue of Zerus
tasks in one Git project. You send a task to the team. The Zerus desktop
service hands it to a free member of the right role, runs the review or test
stages of its pipeline after the member reports, brings the result to the
lead, and merges accepted work into the team's integration branch. You are
asked only for what is yours to decide.

A team task is an ordinary Zerus task. It stays on the project's task board,
keeps its history and attempts, and is reported with `hgs task` like any
other. The team adds who works on it, which stages it passes and who decided
it.

Use a team when a stream of independent tasks in one Git project should each
be implemented, reviewed and collected on one branch without you handing out
every task. For one task, an ordinary session is simpler. A team needs a Git
folder: every member works in its own worktree.

## Switching the preview on

Open **Settings**, section **Agent Teams**, and turn on **Agent Teams
(preview)**. The setting "Applies to this computer. Nothing runs until a team
is created." From a terminal outside any agent session, `hgs team preview on`
does the same, and `hgs team preview status` shows the state.

Turning it on adds a marker file to this computer's `hgs` state directory and
makes the desktop show **New team** and **Waiting for you** in the rail, the
**Team** tab on a lead's session, **Send to team** on the task board and a team
chooser in the New task form. An `hgs task` command that touches a team, with
`--team` or on a task whose team key guards it, then proves its caller as
`hgs team` does: a command without session markers counts as yours only when
no live agent session runs above it, and a command inside a sandbox with its
own user namespace is refused. Every other task command behaves as with the
preview off. The team store is created only when the first team is created.

Turning it off asks "Turn Agent Teams off?": "Teams on this computer stop
being served. Nothing is deleted." With the preview off, `hgs team` answers
`teams_inactive`, the entry points disappear, and a team's tasks on this
computer behave as ordinary tasks. A task whose team lives on another
computer still refuses agents.

## Starting a team

Press **New team** in the rail. The dialog starts from a **Template**:

| Template | Roster besides the lead | Default pipeline | A queued task starts |
|---|---|---|---|
| Feature delivery | two developers, a reviewer, a tester | `code` | after the lead routes it |
| Bugfix | a developer, a reviewer | `reviewed` | at once, with a free developer |
| Small | a developer, a reviewer | `reviewed` | after the lead routes it |

Fill in **Team name**, **Goal** and **Project**. Under **Settings** the dialog
shows the template's **Roster**, its **Pipelines** and the settings it brings:
**Integration**, **Plan approval**, **Acceptance**, **Queue**, **Routing** and
**Land stage**. Under **Project folder**, press **Check folder**: Create team
stays blocked until the folder is a Git folder ("A Git folder at %1, base
commit %2."). That commit is the team's base. **Integration branch** proposes
`zerus/team/` followed by the team's name; you can change it.

"Nothing is created until you press Create team." **Create team** records the
team, and the desktop service then:

- creates the integration branch at the base commit, without checking it out;
- launches the lead, then each member in its own generated worktree, linked
  as a child session of the lead where its agent allows that;
- picks each session's account at launch: by default the signed-in account of
  the role's agent with the most room left in its usage windows.

A launch that is refused, or whose result is unknown, becomes a card in
Waiting for you and is never repeated by itself. **Save as template** keeps
the chosen template under a name of yours in your own catalog.

A new team starts with every delegation on: the lead may accept results
(`accept_results`) and answer members' questions (`answer_members`), the
service confirms folder trust (`confirm_trust`) and resumes members that exited
(`resume_exited`), and accepted results are merged (`merge_accepted`). This
form does not show them; a person changes them with `hgs team settings`.

## The Team tab

The lead's session has a **Team** tab. Its bar shows the team's status, its
active members, open inbox items and open cards, and the team's actions:
**Pause** or **Resume**, **Stop**, and **Archive** after a stop. Destructive
actions ask for confirmation first.

**Members** lists each member with its role, what it works on ("Working on
#%1", "Reviewing #%1", "Testing #%1") and whether its session is linked. A
member's actions are **Pause**, **Resume**, **Stop** and **Remove**; "The
team's lead cannot be removed."

The **Board** groups the team's tasks by state, with the queue as its own
group: **Queue**, **In work**, **Submitted** for reported work, a group for
each stage such as **In review** or **In test**, **Accepted** and
**Cancelled**. A card shows the task's role, pipeline, member, round and
stages; a stage card says which task it checks ("checks the work of %1").
The card's decisions are **Admit**, **Accept**, **Return**, **Hold** or
**Release**, and **Cancel task**; Return and Cancel ask for a reason. **Add
task** opens the project's task board.

## Sending a task to a team

- On the task board, a task in Backlog or To do has **Send to team: %1** in
  its menu, one entry per team of its project.
- The New task form has a **Team** chooser ("Send the new task to a team's
  queue").
- From a terminal: `hgs task add --project PROJECT --team TEAM < prompt.txt`
  for a new task, `hgs task target REF team TEAM` for an existing one.
  `--role` and `--pipeline` route the task as you send it.

The task stays an ordinary task with a `team` key that names its team and
home computer. A task you send, or the lead sends, is admitted at once. A task
sent by a member or by another agent waits until the lead admits it. The queue
is taken in the order of the board's To do column, and each member works on
one task at a time.

When the team refuses a new task, for example because its queue is full, the
task is still created but not queued: it stays on the board as an ordinary
task without the team key. The desktop says "The task was created, but %1 did
not queue it: %2" and offers **Send to team** again or **Dismiss**; the CLI
says "Task REF exists and was not queued (CODE): REASON". Only a task in
Backlog or To do can be sent.

## Waiting for you

**Waiting for you** in the rail counts the open cards of every team on this
computer. The page lists "Cards only you can answer, oldest first." Each card
shows its kind, its team, its age, its text and its decisions; **Open task**
opens the task on the team's board.

| Card | When it appears | Your choice |
|---|---|---|
| **Integration** | Work was accepted, and merging it waits for you because the merge delegation is off | **Approve**: the service merges it and finishes the task. **Reject**: nothing is merged; the task stays in review for a new decision |
| **Phase acceptance** | Every stage of a pipeline whose acceptance is yours has passed | **Approve**: the task is accepted, and merged where the team integrates. **Reject**: the task goes to the lead |
| Every other kind | A member's launch was refused or its result is unknown, a standing confirmation needs you, or a step of the service keeps failing | **Acknowledge** closes the card |

The page also lists **Requests of team sessions**: a question or a permission
request of a lead or a member that only a person can answer. Choosing one
opens that session.

## The lead's decisions

The lead reads its inbox and decides each report the pipeline handed to it.
A task has one decision at a time.

- **Accept.** With an integration branch, the task is Done once the service
  has merged it, or once you approved its Integration card. A merge conflict
  returns the task to its member with the list of conflicting files. Without
  an integration branch the service moves the task to Done at once.
- **Return**, with a reason. The service reopens the task to the same member
  with the reason, which starts a new round.
- **Reassign**, to another active member of the same role, with a reason. The
  service hands the task over to that member.
- **Hold** and **Release.** A held task is not handed out.
- **Cancel**, with a reason. Running work is cancelled, and its member gets
  the ordinary cancel notice of a task. A task with nothing running (Backlog,
  To do or In review) leaves the team and stays in its stage as an ordinary
  task. A task whose delivery is unknown, and a Done task, are left as they
  are, and the lead gets an inbox item about it.

A lead can accept only work its pipeline handed over, and cannot accept or
cancel a stage: a stage ends by its holder's verdict or by you. You can make
every one of these decisions yourself on the Team tab.

## Pipelines and stages

A pipeline says what happens between a member's report and the end of a task.
`code` has a review by a reviewer and a test by a tester, then the lead
decides; `reviewed` has a review, then the lead decides; `docs` has a review
and is accepted when the review passes; `check` goes straight to the lead.
Each stage is a task of its own, handed to a free member of the stage's role
and never to the member who implemented the work.

The stage's holder answers with a verdict: pass, or return with findings. A
return reopens the task to its implementer without the lead. After the
pipeline's rounds, three unless it sets another number, the task goes to the
lead. Every reopen counts as a round, whoever caused it.

## The integration branch

The integration branch collects accepted work. The service merges with Git
plumbing and moves the branch only if it still points where the merge
started. It never checks the branch out, never runs `reset`, never writes the
primary checkout or another branch, and never pushes. No hook runs on these
merges. Integration is unavailable while the integration branch is checked
out in a worktree, with Git older than 2.38, and in a repository that has
submodules, is a partial clone, or declares a hook or a merge driver in its
own Git configuration; the service then merges nothing, and the third failed
attempt opens a card in Waiting for you. Your own merge driver and signing
settings are honoured. Landing the branch on your base branch, or pushing
it, is your own step.

The delegation to merge accepted results (`merge_accepted`) decides who
starts a merge: the
lead's Accept while it is on, your approval of the Integration card while it
is off. Switching it off takes back every accepted result that has not
merged yet and that you did not accept or approve yourself: each goes back to
waiting for you with an Integration card.

## Pause, stop and archive

- **Pause** stops hand-outs and sends nothing to any session. A paused member
  takes no new task. The lead can pause and resume the team and its members,
  but cannot resume what you paused or stopped.
- **Stop** a member: it takes no new task, and neither the lead nor the
  service resumes it. **Remove** takes it out of the team.
- **Stop** the team: every unfinished task is cancelled as after a lead's
  Cancel. Running work is cancelled with the ordinary notice; a task with
  nothing running stays in its stage; a task whose delivery is unknown, and a
  Done task, are left as they are. Every task of the team then loses its team
  key and stays on the board as an ordinary task.
- **Archive** follows a stop and closes the team's open cards.

Zerus never stops a team's agents by itself. The sessions stay, and you end
them as any other session. Quitting a member's agent by hand is not a stop of
the team or of the member: with the resume delegation on, the member is
resumed. Use Stop on the member instead.

## Project limits

A hand-out is written with the lead as its actor and the member as the
executor, so the project's task settings for agents apply to it as to any
agent. With **Agents may start and cancel tasks in other sessions** off, the
service can hand out no task at all. With a limit of tasks started by agents
per hour, a hand-out over the limit is refused. A refused hand-out is never
forced: the task waits in the queue, the service tries again later, and the
third refusal opens a card in Waiting for you.

## What the service confirms by itself

Two standing delegations of the team, `confirm_trust` and `resume_exited`,
and nothing else, let the service answer for you:

- **Folder trust.** The startup folder trust prompt of a Claude, Codex or Kimi
  member whose worktree Zerus created for this team is answered at once.
- **Resume.** A member whose agent exited by itself is resumed in the same
  conversation, at most three times an hour, with a short brief that points
  it at `hgs team current`.

Each answer is checked against the exact run, conversation and prompt,
recorded in the approval journal before delivery, and never repeated when its
delivery is unknown. Any other prompt, a folder Zerus did not create, and a
member you stopped go to you. These confirmations do not turn on automatic
tool approvals.

## Limits of this milestone

- Linux and macOS only.
- On macOS the system hides the environment of its own programs, among them
  the shells and `ssh`, from other processes. A person's command started below
  such a program, for example in `tmux`, over `ssh` or in a terminal inside
  another application, is refused: "On macOS a person's team command cannot
  be checked below a program that hides its environment, usually a system
  shell; run it from the Zerus window." Commands that a lead or a member runs
  are not affected.
- A team lives on one computer: its lead, its members and its Team tab.
- Only a queue: no planning from a goal, no phases and no Graph yet.
- On Linux, `hgs team` from a shell inside a sandbox with its own user
  namespace is refused, whether a person or an agent runs it.
- A person's command is recognized by an interim rule: no live agent session
  may be among its process ancestors. Every reply to such a command carries a
  note that states what this rule cannot detect.

## Origin

Agent Teams were designed and first built by WhiteWh in the `WhiteWh/zerus`
fork; pull request #322 is its first version. Zerus keeps its concepts, its
working process and its desktop views, and builds the runtime on Zerus tasks
and the desktop service. Files imported from the fork keep WhiteWh as their
Git author. See "Origin and credit" in the
[design](superpowers/specs/2026-11-11-agent-teams-design.md#origin-and-credit).
