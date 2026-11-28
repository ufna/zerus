Tasks (hgs guide tasks)

REF: T-xxxxxx or PROJECT/T-xxxxxx. Text on stdin.

  hgs task list --stage S --limit 1..200 [--query TEXT] [--after CURSOR]
  hgs task get REF
  hgs task add [--backlog] [--position N] [--start] [--title TEXT [--request-id UUID]]
  hgs task start REF
  hgs task begin-attempt REF --attempt UUID
  hgs task control plan|execute|status --json
  hgs task report REF [--progress]
  hgs task report-attempt REF --attempt UUID [--progress|--ready] [--considered REV]
  hgs task comment REF
  hgs task rename REF
  hgs task edit REF
  hgs task move REF --position N
  hgs task stage REF todo|backlog|done
  hgs task target REF SESSION|any [--host ALIAS]
  hgs task send REF [--session SESSION [--host ALIAS]]
  hgs task handoff REF --session SESSION [--host ALIAS]
  hgs task cancel-attempt REF --attempt UUID [--reason TEXT]

Add queues; --start/start records work, no resend. Done needs the person.
Technical: hgs task add-technical --start --criterion 'Acceptance check'
Request on stdin; use a verifiable technical criterion.
ACK once as prompted. Managed: begin-attempt, then pinned report command/UUID.
--progress updates. Legacy start prints report command.
History-only means saved, not moved: no retry. No pin: ask for accept/resend.
Cancel only if asked; submission transfers ownership.
Recover unknown delivery, never replay. Abandon the exact attempt only if asked.

Plans launch none. Use parent/Start dialog; bind live ID to receipt.
Children never report as parent. Unknown launch: review, no retry. No Pause.

Edit: To do/Backlog. Same UUID keeps edits; changed input refuses.
Files: --content-json; get first.
  hgs task review-owner REF person|technical|unspecified
Backlog/To do. Criterion on stdin except unspecified.

hgs task message REF --request-id UUID --mode queue|send|reopen|note
  --expect-dispatch ID --content-json
Get: dispatch_id/executor. Queue waits for Start; note saves. Reopen leaves
Review/Done on submission. Same UUID/mode/anchor/content: receipt lookup only.
Coordinator receipts never prove child ownership.
Submitted: transport, not model-read; unknown stays unknown.

Reviewers check the notified scope, report and criterion.
hgs task quick-review REF --json with op verify,
history_revision, report_id, observation and evidence_reference records an
independent verdict and Done together. Ready text alone cannot complete a task.
