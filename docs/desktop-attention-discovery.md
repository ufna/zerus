# Service attention discovery

`desktop::attention::Runtime` consumes only the service's own `FleetEvent`, after
successful `Broker::apply` and while the original ingress permit is held. The
service lifecycle owns activation, writer admission, native host and shutdown.
The discovery component creates none of them.

The actor calls `new(epoch)`, `observe(store, event)`, then
`poll(store, Option<&mut Outbox>)` while enabled or retiring. Initially disabled
discovery skips both observation and polling, retaining no reply backlog or new
attention storage. Disabling an enabled policy schedules an indexed, fixed-highwater
retirement walk of at most eight existing live edges per turn. The original outbox
revokes their callbacks/queued posts and reserves exact native withdrawals after
the original post receipt and resources settle. Retirement needs that outbox;
without it, status remains `retiring` and no native effect is inferred. Idle means
the original operations and callbacks settled, with the same host kept connected;
it is separate from host stop/join. Unknown outcomes are retained without replay.
The lifecycle calls `request_retirement()` when resuming its durable policy-off
state after restart. Re-enabling waits for retirement before restarting discovery.
Enabling resets the baseline; a fresh owned observation
consumes current history before later edges may post. An enabled baseline can
progress without a native host. `reject(computer)` invalidates a rejected source;
`request_stop`, `drained` and `status` fit the service lifecycle. The original
service supplies `Policy { enabled, optional_questions }` (both default false)
and refreshes foreground suppression, which expires after five seconds. These
Rust calls are not generic IPC settings or writer permissions.

The initial catalog snapshot and retention-gap replacements consume old edges.
Subsequent work follows indexed catalog deltas. A changed baseline key is
skipped for the delta walk only if its conversation has saved Tracker state;
a key-first journal index bounds the lookup. An empty saved Tracker counts as
known state. Without a prior record or a before-image, its first observed value
is consumed as baseline. A new edge first encountered on an unread startup page
can therefore be consumed during this bounded first-baseline window; the service
prefers that omission to posting an old unread edge as if it were new. Later
observations of that conversation catch up normally. Journal retention loss
restarts a baseline before any unproven row can be consumed. Foreground refresh
only suppresses observations while active and never restarts a baseline. One
actor turn processes at most four rows and eight dirty effects. The temporary
page has at most 128 rows and 1 MiB; only the 64 computer cursors remain resident. Each row passes through the
shared attention Tracker, preserving Mute, suppressed baselines, optional
questions and reply deferral for background jobs, subagents and independent
agent waits. Unknown/offline observations cannot grant a current source.
An unsent retained reply is checked against the current shared policy again
when a host arrives. A historical duplicate cannot overwrite a current row.

Schema 14 stores bounded Tracker state in SQLite (65,536 conversations / 64 MiB),
not a resident session cache. The permanent logical-edge index is limited to
262,144 entries / 128 MiB of charged identity/payload data. Per-row child and
question lists are limited to 64 each, Tracker state to 64 KiB. Quota or SQL
failure refuses new posting; old delivered/unknown identities are never evicted
to make an edge fresh again. These are encoded data/work limits, not physical
memory or kernel timing guarantees.

Intent IDs hash only the original target (computer, genuine run, conversation,
reply ID or component attention signature). The logical index also binds the
agent and keeps the first identity/token/payload across rename, resume, reconnect
and restart. Component signatures include the actual request identity, so an
unrelated optional question does not recreate a main approval banner. Mapping
is committed before ledger creation; partial SQL work cannot rebind a resumed
edge to a different run. Refused and unknown attempts are never retried.

`Outbox::offer_intent` builds its reservation from the original private Session.
`retire_intent` revokes callbacks and cancels retained queued work without
claiming a native close. Withdrawal waits for the original post receipt, uses
its exact native mapping and obtains its own reservation. Live restoration uses
`restore_live_head/page`: an immutable highwater of attention edge IDs, at most
16 indexed candidates per page, with the original callback guard still applied.
It does not scan retired attempt history or resend notifications.

A non-retryable offer or SQL failure stops discovery and is explicit in status
(`stopped_on_error`, `error_retryable: false`). Repairing a writer underneath the
runtime does not replay its failed offer; lifecycle recovery remains explicit.
