# macOS native host admission boundary

Status: G1/G4 structural agreement after the read-only peer helper in PR #674.
There are no positive exports or executing handshake yet. This document adds
no command, capability, schema, caller or execution authority. G1's `.66.3.24`
is Linux-only. Mac actor/native/bootstrap/activation integration belongs to
`.66.3.25`, dependent on the current producer/profile drain in `.66.3.23`.
The passive prerequisite `.66.3.25.1` owns trusted installation selection,
reciprocal connection and launch-origin qualification before any host caller.
G2 `.66.4.2` preempts this work when `.66.3.22` becomes executable.

This specializes the approved [notification design](desktop-macos-notifications.md)
and the [producer-drain contract](desktop-producer-drain.md). G1 owns the actual
wire schema, trusted installation selection, journal, admission and transport.
The names below identify the proposed boundary; they are not available APIs.

## Connection and trusted identity

The signed installed app connects directly to the existing private desktop-v1
Unix endpoint. It does not use a child `hgs` stdio proxy for this connection:
that would make the kernel peer the proxy rather than the signed app. No new
listener, daemon or independently selected socket is needed. Endpoint discovery
uses the pinned configuration and existing private-state rules.

G1 checks same-user endpoint/peer identity and retains the accepted connection.
An accounted off-actor worker calls `macos_notify::peer::observe` with the
**G1-selected installed designated requirement**, never bytes from a hello,
response, peer path or PID. Its observation is point-in-time evidence, not an
exec fence, lease, current drain or trusted requirement source. G1 keeps the
trusted installed selection and verified upgrade continuity separately from the
supplied-requirement digest. Failure or unsupported audit identity refuses;
there is no PID/path or same-UID-only substitute for the native signed check.

A service challenge, configuration, canonical app instance, clean production
build and current profile ownership must also match for the canonical host.
The current `.23` nonce/cache is **diagnostics only**, not authenticated
producer/profile ownership. Reuse its fields only after the actual `.23`
controller is qualified; never infer ownership from a socket probe.
The ordinary delegation hello, `show`, `sessions`, instance lock/socket and
updater discovery remain byte-compatible. The host must authenticate the
current service through G1's connection/challenge mechanism; a path, same UID
or echoed service epoch alone is not that proof. `.25.1` must select **both**
expected app and service identity from trusted installation policy and bind
the retained current connection, fresh challenge, configuration and actual
actor state. The installed hgs CLI has no guaranteed app-style designated
signing requirement today. Do not assume the app requirement authenticates
hgs, use peer-provided policy or add signing/installer changes to bypass this
gap. Unsupported or ambiguous service proof refuses. No executing handshake
may ship before that pinned contract is agreed.

Proposed handshake: `desktop-native-macos-v1`, version 1, carried as an additive
operation on desktop-v1. A closed `HostHello` describes the already selected
configuration, optional canonical instance, build and intended role; a
service-issued `Challenge` carries a fresh nonce, service epoch and ingress
epoch. The ten-second one-use challenge deadline
matches `.23`. `LeaseBinding` contains configuration, service/writer epochs,
canonical instance, adapter, lease and the G1-owned installation-binding ID.
All these values are checked against current actor state, not accepted because
they decode. A lease is never reconstructed from an imported record. The ABI
name, version and message names remain design names, not committed wire ABI.

The agreed roles are separate. `NativeHost` requires the exact live canonical
context to receive jobs. An early LaunchServices `ResponseIngress` is
response-only under a fresh current service/ingress binding: exact pinned
production configuration, trusted installed app and a per-connection transient
epoch. It never fabricates canonical ownership; with no canonical host,
`canonical_instance` is absent. Original Post epochs/instance remain immutable
and separate from current ingress. The actor finds the exact Delivered mapping
only in that **already selected configuration** and current active ownership.
Transient input cannot select an endpoint/configuration, scan other
configurations, receive a job/lease, present a target, change policy or spend a
bootstrap. Missing/ambiguous trusted local selection or actor binding refuses.
Durable actor deduplication handles duplicate acceptance; there is no app spool.

## Proposed typed boundary

G1 supplies the authority-bearing local types; their serialized forms are data
only. G4 implements the signed host's native method and callback handling.

| Proposed type/message | Exact binding and permitted result |
| --- | --- |
| `NativeJobBinding` | Full existing native `Reserve` fields: request ID, intent ID, revision, writer/service epochs, adapter, lease and retry-of. Add configuration, canonical instance, installation-binding ID, original Post request ID, OS request ID and per-post correlation. Retain the immutable original Post target, binding and payload digest; current ingress is separate. |
| `VerifiedMacJob` | Nonserializable G1 construction only after current authenticated profile drain, intent/tombstone/lease checks and committed `fresh=true` reservation. One `Post` or `Withdrawal`; no Badge/Presence or G2 operation kind. Existing reservation/status never creates this type. It is never a G4 decoded type. |
| `PostPayload` | Frozen original Qt-compatible request ID/token, correlation, title, body and default-sound choice. Reuse the pure `Envelope`; title/body each at most 16 KiB and token at most 8192 bytes. No executable, arbitrary path or session alias. |
| `NativeFinish` | Exact original binding plus before-call refusal, framework acceptance or Unknown. Allowlisted receipt metadata only; no title/body, raw signing identity or system error text. |
| `ResponseCandidate` | Default-action envelope from the pure parser: original request ID, token and correlation. It is untrusted input; G1 looks up the original Delivered mapping and current intent before durable acceptance. |
| `ActivationClaim` | G1-owned activation ID, original Post binding, exact stable computer/run/conversation, current consumer instance and claim revision. Persisted before presentation; no free-form command or latest-session fallback. |
| `ActivationAck` | Same activation/claim/consumer identity and bounded presentation outcome. No agent input, independent read settlement, new target or new claim on lost acknowledgement. |
| `BootstrapAttempt` | Actor-owned launch attempt ID, configuration, installed host binding, ownership/cutoff/burst identity and original eligible fresh edge. Separate from a Post reservation, native lease or callback ingress. |

UUIDs are canonical and non-nil; hashes are fixed lower-case SHA256. Use the
existing target validators and preserve identities across rename. Revision
checks and exact frozen payload comparison refuse changed content under a
previous identity. Private provenance is stored separately from live ownership.
No report may adopt an arbitrary OS request ID, rewrite an original signing
binding or turn immutable Unknown into Delivered.

Proposed client calls are `hello`, `next_job`, `finish`, `submit_response`,
`claim_activation`, `ack_activation` and exact `status`. These are protocol
operations for review, not CLI syntax or Tauri commands. Each requires its
current bound connection and role. Positive control/status never starts a
service, imports a profile, grants permission or launches another worker.
G1 defines exact exports/encoding and the Mac actor dependency before code.

## Ordered execution and completion

G1 admits one native method globally for this configuration, with one queued
job. The host checks the same current lease immediately before issuing the
method. A refused check before issue can report Refused. Once the call might
have been issued, malformed/error/timeout/disconnect is Unknown; there is no
app-side replay. Framework acceptance means scheduled, not visibly displayed.
An old exact result can finish only its still-reserved original attempt;
revoke or a later receipt cannot overwrite immutable Unknown.

G1 commits withdrawal and activation invalidation first. The host removes only
the mapped request from pending and delivered notifications. Both calls are
void and asynchronous: `NativeFinish` records which calls were issued and
always finishes Unknown. No center enumeration, retry, replacement request or
claim of visual removal. A late Post completion cannot undo the tombstone.

Default response acceptance is durable before acknowledgement. Only an exact
Delivered, unwithdrawn original mapping can create a deduplicated activation.
The current canonical consumer persists its claim before presentation, then
rechecks intent/revision/tombstone and the exact live target immediately before
selection. Duplicate/lost acknowledgements reconcile that same claim, never
blindly present again. Missing/replaced/offline outcomes use the existing
notice bus and the approved wording in the notification design. They grant no
alias fallback or new pending UI. Legacy token-only banners remain suppression
only: no activation, removal or reposting.

A transient early delegate must be installed before application launch finishes
and invoke the OS completion exactly once after a bounded submission attempt.
That callback completion is not durable acceptance or proof of OS redelivery.
Service outage is incomplete; the app keeps no durable callback/post spool.

## Bootstrap and user exit

Before a launch side effect, G1 durably spends one bootstrap attempt for the
exact ownership, installed host, cutoff and attention burst. Only a fresh
hgs-recorded edge after the last explicit Quit can wake the host; startup,
reconnect, re-read, old intent and legacy records never do. At most one launch
per burst and at least 30 seconds between attempts. Unknown launch/crash never
creates a replacement attempt for that burst. Host hello reconciles the same
attempt; it cannot turn spending the permit into another launch allowance.

Use only a non-killing start of the maintained LaunchAgent. Qualify launchd
and LaunchServices races with the existing canonical socket. The signed host
is windowless/accessory with no Dock icon, menu-bar takeover or focus steal.
It remains that way after posting. A valid claim or `--sessions` promotes the
same instance; plain `show` changes nothing. A crash-only restart with a spent
bootstrap and no current admission exits cleanly; KeepAlive is unchanged.

The exact host-mode response and launch-origin proof belong to G1 `.25.1`.
It must qualify the OS origin and actual canonical frontend-ready path before
clearing suppression after the disabled setting and explicit Quit. Argv,
environment, a `hello.manual` flag,
reconnect or login bootstrap never proves manual launch. G1 alone returns the
future host-mode decision from durable policy; the app never self-selects
permission or chooses a pending bootstrap. Pre-handoff ordinary app launching
remains unchanged. G1 owns the second-Quit cutoff and policy revision. `Notify when Zerus is closed` defaults on; off plus Quit blocks
this flow until a manual launch. Its future macOS-only Settings row follows
the existing approved placement. No outage changes the stored value.
Preserve `.9.30` Exit/`exit_work` fallback and updater-only `quit_with_flush`.

## Bounds, resources and implementation gates

Control/receipts are at most 16 KiB. Private native payload and callback frames
are at most 64 KiB including framing, with eight queued callbacks and at most
64 KiB aggregate callback bytes. Do not duplicate a maximum-length token and
request ID into a 16 KiB control receipt: only the bounded private frame holds
them; control refers to exact actor-owned IDs/digests. One outstanding control
request, one method and one queued job; indexed activation pages at most 128.
Pressure refuses before acceptance; accepted pending identities are not evicted.
No unbounded per-session cache, history scan or center enumeration.

Each native/control wait is positive and at most five seconds; window readiness
has the existing 30-second startup bound. Actual connection, check worker,
native completion blocks, response callbacks, readers/writers and queued
results retain their admission until completion. Timeout, cancellation, EOF,
dropped receiver or a bounded stop is not drain. No per-request detached
thread or second runtime is introduced; G1 owns worker/connection accounting.

G1 retains `src/desktop/*`, root manifests/locks, SQL/journal, trusted policy,
admission/reserve/finish and actual worker/FD accounting. G4 retains the
existing app-native implementation and pure `macos_notify` data/peer helper.
No new app startup, listener or control caller is admitted by this agreement.

Independent G4 work may add documentation or pure **untrusted** codec/model
vectors over the agreed fields, without a G1 authority constructor, native
caller or runtime transport. Executable transport/host integration waits for
`.25.1`, `.23` and `.25`, then pinned G1 and webui review of exact wire/exports,
reciprocal proof, ingress and launch-origin semantics. Subsequent slices cover
signed host lifecycle and early delegate, and finally qualified native/activation
integration after profile handoff. Startup, transport, signing, window,
capability, manifest or packaging changes need pinned pre-merge review.

Fake-center and private signed-peer cases cover reserve-before-issue, lease
revocation, immutable Unknown, source versus live identity, two configurations,
upgrade continuity, callbacks after timeout, withdrawal races, duplicate claims,
pressure, failed bootstrap and second Quit. Private native Mac qualification
uses the owned heavy lock and no Notification Center/owner app or LaunchAgent
changes. Real posting, closed-app click and visual behavior need a separate
owner-given display window via design-ac. A headless mechanism proof alone
never claims those acceptance results.
