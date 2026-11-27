Tasks (hgs guide tasks)

Own project; REF: T-xxxxxx or PROJECT/T-xxxxxx. Text on stdin.

  hgs task list [--stage review|active|todo|backlog|done]
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
  hgs task target REF new-session|subagent --parent SESSION [--host ALIAS]
  hgs task worker REF --subagent ID --request-id SENT_REQUEST_ID
  hgs task send REF [--session SESSION [--host ALIAS]]
  hgs task handoff REF --session SESSION [--host ALIAS]
  hgs task cancel REF [--reason TEXT]
  hgs task cancel-attempt REF --attempt UUID [--reason TEXT]

Add queues; --start/start records work, no resend. Accept Done when the person asks.
Acknowledge once as prompted. Managed work uses begin-attempt instead of start.
Report with the pinned command and UUID.
--progress updates. Legacy start prints report command without UUID.
History-only: saved, not moved. No retry. Without a pin, ask for accept/resend.
Cancel your task only if asked; else report/handoff. Ownership moves on submission.
Recover unknown delivery, never replay. Abandon the exact attempt only if asked.

Plans launch none. Ask parent/Start dialog; bind live ID to receipt.
Parent reports; children never use parent identity.
Unknown launch: review, no retry. No task Pause.

Edit: To do/Backlog. Same UUID keeps edits; changed input refuses.
Files: --content-json; get first.
  hgs task review-owner REF person|technical|unspecified
Backlog/To do. Criterion on stdin except unspecified.

hgs task message REF --request-id UUID --mode queue|send|reopen|note
  --expect-dispatch ID --content-json
Get: dispatch_id/executor. Queue waits for Start; note saves. Reopen leaves
Review/Done after submission. Same UUID/mode/anchor/content: receipt lookup,
never repeated input. Coordinator receipts prove no child ownership.
Submitted: transport, not model-read; unknown stays unknown.

Enrolled technical reviewers receive a guarded notice for the exact report.
Inspect its original request, exact report and criterion. If independently
verified, `hgs task quick-review REF --json` with op verify, history_revision,
report_id, observation and evidence_reference records the verdict and Done
together. Ready text alone never authorizes automatic completion.
