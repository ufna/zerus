# Canonical frontend startup exclusion

The Linux/macOS live switch bundle claims the existing canonical instance before
starting its GUI writers. Forwarded Show/Sessions launches return through the
existing instance path and start no helper. Replay and other bundle identities
do not enter this production bootstrap. Windows remains outside this path.

The claimed instance starts one `hgs desktop startup --json` helper through the
selected local CLI. It first checks the exact `Desktop startup ABI:
desktop-startup-v1` help marker. Missing or unsupported CLI capability refuses
before GUI startup; no unknown subcommand or legacy fallback is attempted.
Installations must update CLI and frontend together. Errors use the existing
startup stderr channel and create no new UI flow.

The closed protocol has only `hold`, `ready`, `release` and `released`, version
1 and ABI `desktop-startup-v1`, with a canonical caller nonce. Ready/release/
released bind the helper's exact configuration digest. Each line is at most
4 KiB; no private app body is transported. The helper has one fixed ten-second
input/setup/hold deadline, established before its first read. The frontend has
one ten-second budget established before worker handoff, including help. Partial
input, repeated API calls and cleanup never renew either deadline.

The helper uses the [existing negative exclusion](desktop-startup-exclusion.md).
For existing private passive state it retains the actual handoff lock until its
exchange ends. An unsettled preparation, abort request, policy, marker or unknown
state refuses before Ready, including with an offline service. Genuine absence
creates no desktop state and owns no lock. That absence observation alone is not
a permit: safety also depends on the actual canonical startup ordering below.
No service is started and no producer journal is created, changed or reconciled.

The responder's configuration stays unset from canonical claim through writer
registration. Thus a stale producer Context cannot match this new canonical
instance, including if the bootstrap helper exits early or service state appears
after an absence observation. The new frontend cannot bind a configured producer
Session either. Only after setup registers its writers does it request release.
Publication requires exact released metadata, helper exit zero, actual stdin
writer/stdout/stderr completion, original worker join and disappearance of all
its resource copies. An acknowledgement or process-exit flag alone is
insufficient. A late/unknown result keeps configuration unavailable; it never
retries setup or grants a producer connection. This does not assert that the
GUI's role resources are drained or its current profile is instrumented.

The existing runner now permits its explicit input callback alongside an output
decoder; decoders with no input retain their prior null-stdin behavior. One
bootstrap worker and its actual pipe resources remain owned through completion.
Dropping the outer owner requests cancellation but cannot join on its behalf or
release unverified resources. A descendant retaining a pipe keeps the resource
copy alive; an unresolved outer drop pins it rather than reporting completion.
The frontend retains the Bootstrap in one existing Tauri blocking-pool job
while polling and joining; no new executor or timer feature is introduced. This
job and its captures are not a whole-profile quiet proof. No second bootstrap
is installed in the same canonical startup.

Private protocol tests cover exact release, missing/old CLI capability, malformed
and oversized/foreign/duplicate frames, EOF, child failure, fixed deadlines,
held inherited pipes, unknown results and dropped owners. Real CLI tests cover
actual lock contention and both durable preparation phases. The built app must
also prove refusal before any watch/store startup and retain ordinary Show and
Sessions forwarding. No owner display, bus, Notification Center or installed GUI
is used by those fixtures.

There is still no production producer prepare/controller. Persisted original
reconciliation without an in-memory Ticket remains mandatory before that caller,
as do current profile/known-zero Presence proof, continuous negative consumption,
actual native/resource/store barriers and whole-profile G3 capture/publication.
The helper does not reopen an old gate or turn a read snapshot into authority.
