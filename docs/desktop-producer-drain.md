# Frontend producer admission and current drain

This is the implementation contract for G1 `zerus-5wmh.66.3.23`. The existing
delegation hello, passive feed, Staged publisher and revoke receipt do not
implement it. There is no positive producer/drain command or capability yet.
Do not invoke proposed commands against an older selected hgs.

G1 owns the authenticated control connection, current runtime proof, durable
handoff journal, marker phases and effect admission. G3 owns existing source
decoders, bounded Bundle construction and source identity rechecks. G2 owns
operation policy. Existing settings, layout, drafts, cache contents and native
recovery/voice services retain their stores and owners until their specific
handoff is implemented and qualified.

## Versioned ownership profiles

The coordinator's 2026-10-30 ruling admits per-role handoff. The initial fixed
`g1-background-v1` profile contains Fleet, Attention, NativePosting, Badges,
ReadDelivery and Presence. Attention, NativePosting and Badges always move
together. AutomaticApprovals remains GUI-owned and off until its own later
qualified profile; it is excluded from v1. Operations, Drafts, Providers,
Downloads and Cleanup also retain their current GUI owners. G2 Operations
requires its own profile, current drain and the G1 .22 execution guards.

The hgs journal binds the profile ID, its hash and every role's owner and
permissions. One journaled transition moves the entire profile after fresh
drain of every member. Revoke and rollback return that whole profile to its
previous GUI owners in one transition, never role by role. Failure before a
completed move leaves all previous logical owners intact and admissions
fenced until exact reconciliation; it never automatically reopens a gate.
A partially moved profile is never a resting state.

The marker payload stays unchanged and grants no role permission by itself.
Its internal Complete phase records only the journal-bound profile's handoff.
Internal descriptions and status say **g1-background-v1 handed off**, list
service-owned and GUI-owned roles, and never imply G2 or Qt retirement is
complete. Live legacy Qt or an unknown owner of any required role refuses
handoff. G3 captures all families required by that profile; unrelated history
remains Staged. These rules still need executable integration and qualification.
Because ImportRecord evidence applies to the entire record, out-of-profile
families use separate immutable Staged bundles and import UUIDs. A Drained
record may contain only the selected profile's exact required family allowlist.
Original shared-store bytes can prove source hashes, never additional role
authority. The allowlist and profile digest bind the initial Drained receipt.

## Identity and bounded control

The future exact help line is `Desktop producer ABI: desktop-producer-v1`.
The selected local CLI and configuration must be pinned and revalidated before
any new command. The proposed `hgs desktop producer --json` is a single owned
stdio proxy to the existing same-user desktop-v1 endpoint, never a second
daemon or fleet reader. The service admits one producer per configuration;
the caller's UUID alone proves nothing. A missing service creates no state.

Admission binds the canonical configuration hash, production application
`com.hgdev.hgs-tray`, clean build identity, live/non-replay mode and exact
frontend instance UUID. The service supplies a fresh challenge UUID and its
service epoch. The frontend's background controller installs that exact
challenge in a single cached slot in its canonical instance responder and
acknowledges readiness over the producer connection. The service then probes
the canonical endpoint off its actor thread, with a fresh challenge on each
admission. Only a reply from the same OS user, matching the pending challenge,
build, configuration and instance, can bind the connection. The challenge is
one-use, expires after ten seconds, and is never an effect permit.

The additive query `desktop-producer-v1 UUID\n` fits the existing 64-byte
instance line. Replies remain bounded to 4096 bytes and one total one-second
socket deadline. The ordinary delegation-v1 reply, show/sessions activation,
canonical socket ownership and updater discovery stay unchanged. No per-probe
child or GUI-thread I/O is allowed. Absent, legacy, dev, replay, stale,
mismatched, late, unsolicited and oversized replies stay passive. A connected
older app ignoring the query is live unknown, never proof of absence.

The cached responder now implements this query, but no production controller
installs a challenge and no producer command/help capability is advertised.
Its closed reply contains `abi`, `version`, the unchanged delegation `hello`,
an optional `challenge` (nonce, service epoch, producer epoch), and optional
`read_delivery` diagnostics. An unmatched query returns diagnostics with no
challenge and does not consume the pending offer. A matching query consumes
the offer once; it expires at ten seconds and cannot be replaced while live.
Binding requires the exact pending challenge and the previously observed
frontend build/configuration/instance identity, not merely a valid reply.
The receiver must also retain and check its own monotonic pending deadline;
an expired reply cannot bind. Fresh challenges always have new random UUIDs.
The ordinary delegation reply has no added fields.

Control messages use a closed versioned schema, canonical UUIDs and at most
16 KiB per line. There is one outstanding request and one response per producer,
with no private bodies in control or status. A server-generated producer epoch
identifies this connection. It is distinct from the frontend instance UUID,
service epoch, prospective writer epoch, drain UUID and import UUID. An exact
duplicate is status-only; it never repeats a freeze, capture or effect.

## Closing admissions and retaining resources

The control order is bind, durably prepare handoff, close frontend admissions,
stop/cancel owned work, verify actual resource completion, flush the existing
stores, and only then begin a fresh source capture. A prepared handoff must be
visible to a replacement frontend before it admits writers. Service death,
connection loss or a missing reply must never restore legacy writers while a
handoff's outcome is unknown.

Use explicit fixed roles, with no free-form role names or per-session cache:

| Role | Resources that must end or cross an ordered durability barrier |
| --- | --- |
| Fleet | Existing shared supervisor, workers, connections, readers, queued callbacks and source generations; no parallel reader starts |
| Attention/native posting/badges | Detector inputs and timer, queued native calls/results, native connection/executor, persistence and badge offers; stale clicks are fenced |
| Read delivery/presence | Read admissions and owned children plus stdin/stdout/stderr, pending-mark writer, presence samples and writes |
| Creation/task/session operations | Existing queue admissions, workers, callbacks, journal writes, frozen original input and unknown outcome records |
| Draft/provider operations | Composer and GUI voice jobs, manifest writes, submitted/unknown callbacks and provider operations; native recovery/voice services are not stopped |
| Download/cleanup operations | Transfers and their callbacks, cache worker writes, the owned trim timer and exact cleanup outcomes; no content removal or replay during capture |
| Automatic approvals | Discovery/admission, private audit reservations and native-input workers; remain off throughout migration and require separate new local consent |

These are ownership groups, not a claim that the present GUI can drain them.
An uninstrumented or unsupported required role refuses the handoff. Partial
coverage never authorizes that profile's handoff or another profile's roles.
UI-only preferences may remain client-owned; their source snapshots still
require exact fresh rechecks and cannot prove operation quiescence.

Core's `desktop::drain::Gate` provides only atomic close and bounded lifetime
accounting. It holds twelve fixed role counters and at most 1024 resource-group
admissions (the host may choose a smaller cap). Admission and close share one
lock. An already admitted resource stays charged until its last guard is
dropped, including copies retained by queued callbacks,
the owned child, input writer and both output readers. Cancelling or returning
from a bounded join does not drop those copies. Pressure refuses new admission;
unknown pending resources are never evicted. Poisoned accounting stays unknown.
`Closed::wait_until` bounds waiting with an absolute monotonic deadline; a
Pending result keeps charges and admissions closed. A gate with zero charges
does not attest that all writers were instrumented, that stores flushed successfully,
or that the service granted any authority.

The frontend's integration must stop each role's producers before waiting on
its charges and must execute ordered store barriers after all late callbacks.
An exit flush, timer, timeout, claimed count or imported record is insufficient.
The existing Exit versus updater Quit ordering remains unchanged. No gate close
automatically signals an agent, process group or unrelated service.

The read-delivery integration retains its existing one loop and four delivery
slots. App state holds a dormant `ReadDelivery` control handle; only a future
explicit stop closes that loop's gate, wakes it and cancels its owned reads.
There is no producer-control caller yet. Each callback and its actual child,
stdin writer and output readers retain the same admitted group. A returned
outcome releases neither a blocked pipe nor its capacity. The selected read
argv, payload, local/peer deadlines, settlement and backoff stay unchanged;
stdout is now bounded to 1 MiB. Native delivery Idle alone is not the pending-store durability barrier or a
handoff proof.

Four fixed resource slots now retain that same child/input/output/callback
charge. If any copy survives the callback, one bounded log line names only
`role=read_delivery` and `slot=0..3`. Slot release still requires the last
resource guard; diagnostics never reclaim it. Cached health contains a
monotonic revision, capacity 4, in-flight and pinned counts, and stopped state.
The existing notice bus warns once per app instance when all four slots pin.
This is a delivery-stall notice, not an individual read success/failure notice.

`hgs desktop status --json` reports `frontend_read_delivery`: the matching
frontend instance and its read-delivery counts, or explicit absent/unknown
coverage. With a service, one off-actor probe supplies a bounded cache, spaced
at least ten seconds apart and stale after fifteen seconds. A blocked probe
cannot stack workers. Without a service, status makes one read-only canonical
probe; it never creates desktop state. A legacy reply, invalid counts, missing
identity or unsupported platform supplies no health. This field is GUI-owned
resource visibility and always has `authority:false`; it is not the service's
read queue, a complete role drain or a takeover permit. An orphan remains
charged until its actual holders exit, or until that GUI process is restarted.

The existing `ReadMarks` store has an explicit `checkpoint()` prerequisite.
Its first internal Tauri caller is the dormant ordered barrier described below;
no producer command invokes it. Call it off-thread under the store mutex, only
after all mark-writer admissions and their callbacks have actually drained.
Its caller must retain the role charge across blocked filesystem I/O; a timed
out wait cannot turn it into a successful barrier. Normal GUI APIs retain their
existing format and behavior. The checkpoint separately remembers failed saves,
refuses an unreliable initial load or changed source bytes, and enforces the
same 16 MiB/50,000-entry bounds as G3, including original rows before duplicate
key coalescing. Serialization streams the ordered entries into a bounded buffer
without collecting another reference list.

Unchanged source bytes and unknown fields are synced without rewriting. Pending
in-memory changes use the existing private atomic writer, followed by file and
parent-directory sync. Linked, public, foreign or symlinked leaves refuse. A
directory-sync error can follow a completed rename and still returns failure;
neither the file's presence nor a successful legacy flush proves durability.
The returned hash/length/count is source metadata only, never a complete role
drain or capture context. Native Windows checkpoint durability is unqualified
and refuses before checkpoint I/O. The store method does not close or activate
anything itself.


## Attention store checkpoint prerequisite

The existing Desk owner observes its original attention-observed load and
successful compact JSON saves through `attention_store::Observation`. This
adds constant-sized hashes and sticky failure flags, with no second tracker,
state copy, detector, source reader or timer. Normal load/Qt fallback and save
behavior stay with the existing owner. A malformed, unreadable or oversized
initial source, or any failed save, remains unreliable for that owner's
lifetime even if a later save succeeds.

The dormant `Attention::checkpoint()` must run off-thread only after the future
Desk/native callback barrier has closed every writer and drained its actual
resources. The host must retain the I/O charge through an outer timeout or
receiver drop. It verifies at most 16 MiB and 50,000 observed entries, the
current in-memory serialization against the last loaded/saved view, and the
fresh private regular source leaf against its original-byte digest. It reuses
the ReadMarks checkpoint file checks. Changed, public, linked or unsupported
sources refuse. Windows refuses before checkpoint I/O.

An unchanged file is synced without rewriting its whitespace or unknown
fields. An originally absent file may be written only through the existing
owner's writer, then reread and checked against the exact intended bytes. File
and parent-directory sync must both succeed. Failure remains sticky, including
a late sync failure after bytes have appeared. The result contains only source
digest, byte length, host and entry counts; it proves neither Qt fallback
source integrity nor a full Attention drain, native posting outcome, profile
ownership or permission to publish Drained evidence. There is no production
freeze caller or positive producer API in this prerequisite.

## Ordered Desk Attention retirement prerequisite

The existing Desk mailbox now has two private one-use controls. No producer,
IPC, startup or profile controller calls them. The app retains one fixed
Attention resource charge before the original background load (including Qt
fallback), through saves and the final checkpoint/reply. Weak controls do not
retain a finished resource group or keep the input channel connected. Ordinary
input sends, lazy batches, the 250 ms pace and 10 s sweep are unchanged.

The first ordered barrier stops detector observations and the sweep, after the
preceding batch has been applied. Desk continues projecting menu/UI changes.
Already admitted agent delivery failures can still update the original tracker
and save their retry state, without observing or posting another edge. The
second barrier may be enqueued only after the host has independently fenced
and actually drained agent-native work and every completion sender. It closes
agent completion/click mutation, checkpoints the sole original Attention value
on the existing Desk thread, and retires that value only after successful sync
and reply delivery. The UI thread remains with `attention=None`; there is no
second detector, store, worker or queue.

A timeout never reopens observations. Failed/late checkpoints and lost replies
keep the same frozen value charged; unverified destruction or panic pins one
fixed charge until process exit. The caller waits for actual resource release
with the same absolute deadline. Unsupported Windows control refuses before
closing anything. An absent Attention owner is unavailable, never a known-zero
producer proof. The returned checkpoint is source metadata, not native drain,
current ownership, profile activation or permission to publish Drained evidence.

NativePosting in `g1-background-v1` covers agent attention posts, withdrawals
and activation only. GUI software-update notices and menu projection remain
outside it, with their existing IDs/actions/authorization. This mailbox slice
does not close the shared native notifier. Future native integration must
isolate per-owner IDs, categories, action routes and immutable mappings,
including legacy/unbound suppression, and test both directions. Shared native
connections/executors may remain for GUI work only after every agent-owned
queued call, future, completion and reference has actually drained. Neither
shared lifetime nor this local stop token supplies that proof. Badges and all
other profile roles still require their separate qualification.

## Optional notifier completion accounting

The existing Tauri `Notifier` has an optional `new_accounted` constructor for a
host-supplied NativePosting charge. The current Mac GUI still uses `new`, with
no resources or control. No system center, native method, delegate, listener,
worker, timer or writer is added by this prerequisite.

The optional path retains the same host group through admitted delivery and
Center calls, native completion ownership, queued main-thread Hop work and
failure sinks. Even a synchronous completion cannot release the still-running
Center call. A weak close-only control stops later admissions; a late callback
can update its existing in-memory delivery result but schedules no new native
step after closure. Already admitted work retains its actual lifetime. Closing
or retaining the control itself never proves a full role drain: the host must
close/wait its Gate off-thread and separately cover every real platform
connection, delegate, callback ingress, Desk writer and badge resource.

A completion or queued work item dropped without execution, or a panic before
normal completion, is unverified. It pins exactly one copy of the fixed host
charge until process exit and logs one bounded static role/slot diagnostic;
additional failures retain no extra pin. Dropping every notifier/control cannot
release that unknown charge. The local snapshot reports closed/pinned only,
with no serialization or authority; an absent scope is unavailable, not idle.
The future host must expose a pinned/unavailable refusal and never retry a lost
native outcome or infer completion from a timeout. There is no production
accounted constructor or freeze caller yet, and no service/native ledger job is
admitted by these local resource helpers.

## Optional agent/update notifier partition

The internal `notifier::owners::Partitioned` constructor reuses the existing
Notifier and completion guards, with separate agent-attention and GUI-update
Delivery state and failure sinks. Only the agent side takes the host-supplied
NativePosting charge. The current SDK adapter still uses `Notifier::new` and
has no partition/control caller; no native permission or GUI behavior changes
in this prerequisite. Ordinary `deliver` keeps its former void semantics.

The shared pure `native_notice::Owner::{AgentAttention,SoftwareUpdate}` has no
serde or wire authority; its validator checks an explicitly supplied expected
class, never the owner of a legacy/native ID. Each partition validates the
complete batch before any effect: agent tokens
use the existing bounded attention decoder; update tokens use the existing
exact versioned update validator. Cross-owner posts/withdrawals and malformed
legacy identifiers refuse in both directions. Tokens and Qt-compatible request
IDs are never normalized or re-encoded. A shared native authorization request
may fan out permission data to at most two fixed owner callbacks, one per
Delivery; it is never a post receipt or authority to touch the other owner's
state. No native call or sink runs under the fanout lock. An abandoned check
retains the original callbacks and their charges; it cannot create a false Idle
or authorize another native retry. Agent close leaves update posts, withdrawals
and outcomes live, even when they share the same backend.

A successful local delivery return means only that routing/admission ran, not
that the framework accepted, displayed or withdrew anything. This helper does
not verify an original Delivered mapping, create categories/actions, process a
click, adopt a native ID, enumerate banners, or infer binding from a valid token.
Valid historical/unbound tokens still require the agreed suppression policy at
the future admitted adapter. That caller must separately bind owner, original
request ID, category/action and immutable post/correlation evidence before any
native effect or activation; it needs its own pinned review. Shared backend
lifetime is not agent work after its actual calls and callbacks are drained.
Platform/delegate/queued activation/badge lifetimes, authenticated producer,
profile transition and G3 Drained publication remain unqualified here.

## Admitted notifier input retirement prerequisite

The separate internal `notifier::retiring::Retiring` constructor closes new
agent inputs while allowing their already admitted continuations to finish.
`Partitioned::new_retiring` composes it with the shared two-owner authorization
fanout and the unchanged GUI-update partition. Existing `Notifier::new` and
`new_accounted` keep their reviewed strong-close behavior; the SDK adapter still calls `new`. This mode has no production caller,
new worker, runtime, source reader, persistent cache or public controller.

`prepare` takes ownership of the original immutable batch before the host's
existing main-thread dispatch. At most 64 preparations can be queued/running,
with at most 128 combined notices/tokens and 64 KiB of original UTF-8 string
bytes per batch. Validation reuses the expected AgentAttention token class.
Refusal returns the complete unchanged private batch before any native effect;
nothing is truncated, evicted or retried. Debug output omits its private data.
The host must handle pressure before wiring this constructor into production.

`InputControl::close` closes preparation admission before releasing the parent
native scope charge. Prepared work and each derived authorization/add/remove,
callback, Hop and failure sink retain forks of the same original Completion,
without fresh admission. A queued original withdrawal can still cancel its
pending original post. Unknown drops/panics pin the qualified fixed scope.
Immediate callback execution cannot release a still-running native call or its
queued payload reservation. Unsupported Windows close refuses before mutation.

The returned Closed gate covers preparations only. The host must also close
and wait its supplied NativePosting gate to the same absolute deadline, then
enqueue the ordered Desk checkpoint after every failure send has returned.
The composition test retains a native add callback, proves Pending, delivers its
failure, verifies the original Desk save, then checkpoints the exact digest.
Neither local Idle nor prepared work is a native lease, visual outcome, signed
instance proof, role transfer or Drained publication permit. Native identity,
category/action/correlation/legacy policy, activation, badges and the actual
frontend/profile controller still need their separate qualified integration.

## Ordered ReadDelivery store barrier

Actions now owns the same fixed four native resource slots as the existing
read-sync worker. A direct `run_action` MarkRead reserves one of those slots
before saving its intent, and runs the unchanged read invocation through
`run_accounted`. Its local/peer timeout, input, empty parent environment,
settlement and retry semantics are unchanged. The direct path shares the
worker's 1 MiB stdout cap. A direct result callback, cancelled future, child or
pipe cannot release another resource's admission. No second loop is added.
Worker startup is one-use; a failed native-worker startup still leaves the
pending-store queue admitted, preserving intents for a later application run.

A separate fixed 64-group admission gate covers every Actions pending-store
mutation: queue/merge, worker and exit flush, snapshot settlement, orphan removal
and direct MarkRead persistence. Charges span their synchronous change listeners
as well as file writes. Queue refusal is an error, not accepted intent; the
existing frontend error path reloads the actual Rust pending list. Snapshot or
orphan work refused after closure cannot mutate the frozen store. Read-only
pending-list snapshots remain readable. Pressure refuses new admission without
evicting pending intent. Test-only store helpers are not production entry points.

The internal `Actions::freeze_reads` handle is single-use and non-serializing.
It closes pending-store admissions first, then the native gate and cancellation
flag, and wakes the existing loop. The handle's background caller waits for
both gates with one absolute deadline before taking the store mutex and calling
its checked checkpoint. Poisoned or busy store access, unreliable source state,
failed sync and expired deadlines refuse. A timeout never reopens admissions or
reclaims resources. A separately charged, one-use checkpoint group remains held
through actual filesystem I/O, even if the controller abandons its wait; a sync
completing after the deadline still cannot return success. The future controller
must own the handle inside that background task, not detach its charge at timeout.
Windows refuses freeze before closing any admission because native checkpoint
durability remains unqualified.

There is still no production caller of freeze, positive producer command,
profile transition, G3 capture context or service read writer. The local
checkpoint result is source metadata only. Every other role in the selected
profile still requires its own qualified current drain and store barriers.


## Fleet supervisor resource accounting

Core's additive `stream::spawn_accounted` starts the same existing supervisor
with one already-admitted host resource group. Ordinary GUI `stream::spawn`
keeps its existing path, output, controls, arguments and deadlines. There is
no GUI caller of the accounted constructor yet. Raw source mode reuses its
existing admitted worker group; no new reader or event producer is created.
The caller must bound source groups and keep its router and consumer callback
lifetimes separately. Neither constructor nor a resource count grants authority.

One primary slot covers a help probe or watch child and all of its pipe holders.
A prior probe/watch holder refuses another primary attempt until actual release.
The existing bounded legacy subscriptions, organization poller and one-shot
worker-refresh read share 35 callback slots per source; a retained callback or
pipe cannot be evicted to admit another. There is no per-session projection cache
or new polling loop. Closing the scope is sticky. Control handles hold only a
weak reference; an idle retained handle does not charge a completed worker.

Callbacks retain their original group through result delivery and decoding.
Child/stdin/stdout/stderr retain actual resources after a bounded stop or returned
runner outcome. Only verified owned-child exit releases its child guard; unknown
exit pins that admission until host exit. The decoded runner now also retains
its admitted reader group on unavailable child exit. It still requires successful
owned-child exit, both real pipe completions and a successful decoder before
returning data. Existing native recovery, voice and swarm services retain their
own authority; this tracks the existing CLI helper reads, never those services.

Unix probe/watch helpers start in their own process group. Before signalling
that group, `waitid(WNOWAIT)` verifies the still-unreaped owned child; only then
can the supervisor signal its exact group and reap the leader. This also cleans
up an ssh/ProxyCommand child when its hgs parent exits first, without signalling
a reused PID/PGID. Unavailable child ownership never permits a group signal.
Detached children (including a detached ControlPersist master) are not chased;
any inherited pipe they retain stays charged. Group signalling is limited to
these read helpers, never agents, terminals or caller-selected/shared groups.
Actual reader/helper completion still releases admission, not signal delivery.
Primary pressure stays an ordinary reconnecting state with one bounded static
role/operation log line, never internal resource text in the computer status.

### Fleet and GUI consumer barrier

`Fleet::start_accounted` now passes the same host resource group through the
existing router and supervisors. It admits at most 64 live or retired source
groups in total. Removing a peer releases neither its slot nor its host group
until the detached stop callback and actual supervisor/helper/pipe resources
end. Pressure reports the existing reconnecting state; explicit Retry now may
admit a still-listed missing source after capacity becomes available. It never
stacks another poller. The old unaccounted constructors retain their shutdown
behavior.

A weak `FleetControl` closes source admissions and requests cancellation without
charging a finished graph. Accounted Fleet shutdown bounds joins to one shared
one-second wait; unfinished threads keep their guards after the handle returns.
The GUI's existing source pump retains the same group through channel EOF,
already queued changes, organization observation, mirror/tap processing and the
creation observer callback. An external sink is not drained by a returned
`StreamHandle::stop` or Fleet stop. Work scheduled by another role's observer
remains that role's responsibility; this does not transfer G2 ownership.

The dormant internal `freeze_fleet` barrier closes a fixed 64-group GUI control
gate before stopping sources, so already admitted starts cannot recreate a
source after close. It waits off-thread for those controls, takes the source
without blocking or poison recovery, stops its accounted Fleet and waits for
actual source/pump resources against the same absolute deadline. A returned
stop or late completion never supplies successful drain. Busy/poisoned state,
replay, pending I/O and expired deadlines refuse, leaving admissions closed.
The one-use barrier charge remains held by its worker while stop is executing;
dropping a controller receiver is not completion. Windows refuses before
closing admissions because the native integration is unqualified there.

The Unix GUI now uses this accounting for its existing single Fleet and pump;
argv, source selection, subscription/callback ordering and ordinary rendering
remain the same. No production caller freezes it yet. This supplies only the
Fleet role prerequisite: no positive producer command, profile handoff, fresh
capture, writer delegation or cross-role quiescence is implemented here.

## Fresh capture and publication boundary

G1 supplies G3 an opaque, non-serializing capture context only after the exact
instance's required roles are closed, resources ended and barriers succeeded.
Its closed wire binding contains version 1, configuration, service epoch,
prospective writer epoch, producer epoch, frontend instance UUID, drain UUID,
import UUID and a digest of the fixed required-role profile. G3 neither mints
nor renews this binding and never treats its serialization as authority.

The context remains bound to the live control connection. Heartbeats every
three seconds have a fifteen-second idle limit and a ten-minute absolute limit
from drain preparation. Slow drain has a bounded 120-second attempt; timeout
is unknown, never a success. Expiration or EOF fences G1 activation and new
publication. A frozen frontend remains closed until an exact durable abort or
handoff result is reconciled; it never resumes because a lease expired.
Even after a verified abort, an old Gate stays closed. Any replacement requires
a new explicit ownership decision and a distinct gate; old callbacks retain
their original charges.

After drain, G3 invokes the existing source readers anew, creates an untouched
Bundle with the retained import UUID, and performs each source's fresh identity
and byte-hash recheck before the final record. A pre-drain Bundle, a previously
published Staged import and a historical source phase cannot be promoted.
Memory-only rows/outcomes require a bounded frozen runtime export from their
existing owner; they do not acquire invented source files.

The ordinary Publisher remains Staged-only and unchanged. A future separately
advertised drain-publication mode consumes a fresh Bundle under the exact
capture context, using the existing bounded import framing and limits. G1
alone verifies the current control/role proof and constructs the initial
immutable Drained record during that new publication. It never edits or
relabels an existing Staged record. G3 validates the exact derived receipt,
record digest, configuration, import identity, profile and complete original
source/page/snapshot membership, digests, lengths, counts and fixed references;
lost delivery uses only retained-ID status, never an automatic new capture.

Slow input precedes the short publication lock. G1 verifies the capture again
under handoff.lock before immutable commit and before any marker transition.
Revocation fences new admission first, cancels publication, waits for its real
resources, and checkpoints before settling its receipt. A partial publication
or disconnected producer never authorizes a writer. Final Complete and any
later effect still need fresh canonical-owner checks, the held ownership locks,
current epochs and the exact effect journal. Automatic approvals remain off.

## Implementation and proof order

1. Agree this contract and qualify the fixed-size close/lifetime gate, without
   positive IPC or authority.
2. Add the authenticated one-producer control and durable prepared/frozen/abort
   journal, with replacement-frontend startup refusal for an unsettled handoff.
3. Instrument and qualify actual role drains and store barriers. Missing roles
   keep production admission refused, including newly added GUI workers.
4. Connect G3 fresh capture and initial Drained publication, then qualify marker,
   revoke, reconnect, restart and lost-result ordering before writer takeover.

Each slice is a separate reviewable PR. Private fixture tests must cover close
versus concurrent admission, retained callbacks/pipes, failed barriers, frontend
replacement, service/producer loss, stale epochs, capture expiry and partial
publication. Socket/setup changes additionally need a built-app private D-Bus
and Xvfb smoke preserving Show and --sessions, with no second instance. Native
Windows custody and production acceptance remain separately unqualified.

## Optional Linux GUI badge resource scope

`platform::linux_tray::install_badge_accounted(app, conf, Charge)` is an
internal, optional replacement for the ordinary tray installation. It requires
`Role::Badges` and refuses a duplicate installation before native setup. There
is no production caller, service control endpoint or takeover permission. The
ordinary `install()` path keeps its existing SNI, menu and launcher behaviour.
The returned weak `BadgeControl` exposes close and closed/pinned/ready diagnostics;
none is a role-drain receipt or current ownership proof.

The supplied charge covers startup before the detached installer runs, both
shell-owner subscriptions, coalesced desired work (one), the one worker's actual
badge sends, and at most sixteen admitted Query dispatch futures. A Query guard
wraps zbus 5.19's generated `DispatchResult2` future, including serialization and
`Connection::reply().await`; returning the Rust Query value is not completion.
Pressure and closed requests return a static error without an old badge payload.
Unissued coalesced work can be cancelled locally. A native error, discarded
future, panic or unverified cleanup pins one fixed charge until process exit.

Close fences badge offers, Query admission and periodic republishing first. The
existing worker removes only the LauncherEntry interface, stops both bounded raw
shell streams, acknowledges each exact RemoveMatch, waits for admitted Query
futures, and issues at most one clear of the old nonzero badge. Both raw receivers
are dropped before either RemoveMatch: otherwise their bounded queues could
block the socket reader carrying the removal reply. Their charges survive until
the acknowledgements. The clear guard survives its actual send; neither its
return nor a successful RemoveMatch claims visual removal. Failed cleanup is
unknown, never a retry. Calls have the existing five-second method deadline;
an outer wait or stopped flag never releases retained native work.

SNI/menu/primary activation, watcher registration and the shared GUI connection
remain alive after badge-only closure. Those residual resources do not establish
badge quiet by themselves: the badge subscriptions, generated replies, queued
work and final send must each complete. A whole-window quit may still stop the
UI worker using the existing exit path. There is no second worker, executor,
reader thread, daemon, detector or ledger. Notification agent/update ownership
and the GTK attention focus timer remain separate qualification work.

Private-bus tests exercise the actual generated Query future and actual socket
backpressure, explicit match-removal success/refusal, startup versus closure,
pressure, dropped/panicking work, and one clear with no late badge republish.
The backpressure fixture suspends only its own private D-Bus daemon and resumes
it with a scope guard. Existing SNI/menu activation and the default badge path
are tested separately. No owner display, session bus or desktop service is used.
