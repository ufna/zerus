# Linux notification method port

`client/core/src/linux_notify.rs` contains pure saved-ID compatibility policy.
Its default build has no transport dependencies or OS calls. The explicit
`linux-native-notify` feature adds `linux_notify::transport` on Linux only,
using the same zbus 5.19, async-io 2.6, futures-lite 2.6 and Tokio 1 versions
already used by the Tauri client. Tokio supplies only a runtime-context probe;
this port never creates a runtime. The root CLI does not enable this feature.

The method port has no production caller. The existing Tauri poster retains its
filesystem and Qt readers, Saved pending barrier, persistence and OS authority.
This extraction neither hands off that writer nor admits service execution.
In particular, its strict post-call Unknown classification does not replace the
GUI adapter's existing known-refusal behavior.

## API and ownership

`Transport::connect` takes an explicit bus address, budget and cancellation flag.
`namespace` reads the current authenticated connection's GetId and the daemon's
unique owner. `post` takes that expected namespace, title and body; `withdraw`
takes a `NativeId` containing the namespace and positive `NonZeroU32` ID.
`shutdown` returns `Pending` or `Drained` while leaving the transport owned.
All methods require exclusive access; no cloneable connection escapes.

`Namespace` and `NativeId` are data only. G1 must supply current authenticated
producer drain, writer/service epochs, adapter lease, fresh reservation and the
exact native job before any call. G1 binds a result to that original ticket and
owns its indexed bus/owner/ID/token/revision mapping and durable activation
tombstones. A withdrawal requires tombstoning first, then the OS close. Neither
imported state nor a method result establishes those permissions.

## Bounds and outcomes

Each budget must be positive and at most five seconds. A method's absolute
deadline includes its fresh GetId/GetNameOwner preflight and native call.
Cancellation is checked before admission to the call and while driving I/O,
with a ten-millisecond polling interval. Title and body are each at most 16 KiB
of UTF-8. Payload text escaping, app name, icon, actions, hints and timeout match
the existing GUI adapter.

The native destination is always the freshly checked unique owner, never the
well-known name after checking it. Bus lookup replies must have the bus sender;
successful native replies must have the exact daemon sender. A missing daemon,
namespace mismatch or failed lookup refuses before the native call. Every
possibly-issued error, including a daemon MethodError, malformed/zero identity,
timeout, cancellation or disconnect, is `Unknown`. No automatic retry exists.
Outcome enums contain allowlisted metadata, never private daemon error bodies.

The connection's zbus executor thread is disabled. The caller's own thread drives
the executor, with one queued message and no added worker, callback or result
queue. zbus still enforces its protocol frame limit of 128 MiB; the 16 KiB text
limit is a request limit, not a claim of a smaller raw incoming frame limit.
This port does not install a signal stream or dispatch activation.

The port refuses construction and methods inside an active Tokio context, and
keeps shutdown pending there. This also covers dependency-feature unification:
zbus can otherwise choose Tokio, whose executor `is_empty` does not account for
its tasks. Connections are created and driven outside Tokio, so actual async-io
task accounting remains available. Tests cover both zbus backends being enabled.

## Actual resource drain

Shutdown first disables calls. `Pending` retains the port and execution charge;
the caller may continue bounded shutdown attempts. A successful close or expired
deadline alone is not completion. `Drained` requires the connection to be dropped
and its owned executor to have no unfinished connection/reader tasks. Dropping a
port is not an actor-authorized release receipt.

The later G1 adapter must also account for its signal stream, callbacks and
queued results before releasing admission. Those resources, service revoke and
restart/reboot custody, durable native receipts and positive IPC are not supplied
by this method-only slice. The existing single-instance/SNI/menu routes and
Exit/`exit_work` versus updater `quit_with_flush` boundary stay unchanged.

## Passive owned-identity codec

`linux_notify::owned` adds data validation and bounded encoding only. It has no
caller, I/O, mutating transition, collector or native effect. Schema1 `Saved`,
`decode_saved` and the current GUI's authority remain unchanged. The future
optional typed store is the sibling `linux-notifications-owned.json`; it must
never rewrite `linux-notifications.json`, so a GUI rollback can still read the
original schema1 bytes.

The version2 shape has `version`, `bus_id`, `owner`, `tokens`, `uncertain` and
`legacy_uncertain`. Each positive canonical decimal u32 token-map key names an
explicit `{owner, token}` record; uncertain entries use the same record shape.
The only private owner tags are `agent_attention` and `software_update`.
`native_notice::Owner` itself stays without serde. `ScopedToken::new` checks a
caller-supplied expected owner against the existing token validator; it never
classifies an ownerless token. Exact token spelling, including accepted padding,
is retained. Namespace strings are retained data, never a current bus/lease
proof. Both namespace strings empty is an explicit absence and permits only
suppression. A nonempty namespace requires a 32-hex bus ID and a valid colon-led
unique daemon name, and can retain positive IDs without authorizing their use.

`decode_owned(bytes)` checks the 16 MiB input bound, canonical IDs, duplicate
native-ID members, duplicate known identity fields, explicit tags and the raw
combined 8192-entry bound before map/set coalescing. `quarantine_legacy(bytes)`
reads raw schema1 members without a coalescing ID map: distinct tokens under
repeated raw IDs all become unbound suppression. All legacy numeric bindings
lose action/removal authority even when their old lineage matches. The raw
legacy import is capped at 4096 inside 8192, reserving headroom; refusal must
happen before any ownership change and preserve the existing whole GUI profile.
An implicit role fallback after service handoff is forbidden.

`OwnedSaved::from_legacy(Saved)` is a validated typed-data convenience, not the
raw quarantine path: `Saved` has already coalesced raw members. The raw decoder
cannot be replaced by that convenience, and neither output is original-source
evidence. Original bytes, hashes, order, opaque fields and source counts remain
G3's evidence. G3 owns a future distinct sibling-file source family; no Qt-v2
store or source reader is invented here.

Inspection exposes bounded binding, scoped-uncertain and legacy iterators,
namespace data, the pre-coalescing input count and `has_unknown`. Unknown fields
are skipped without retaining a second private-body tree; the caller retains
the original bytes. `OwnedSaved` is not serde-Serialize. Its explicit `encode()`
refuses `has_unknown` and caps serialized output while writing, including JSON
escape expansion. A future writer must check the existing sibling record and
refuse opaque extensions instead of overwriting them with a fresh projection.

No release or reservation/result/withdrawal transition exists in this slice.
A later pure transition must bind its exact original instance/reservation,
expected owner/token and namespace generation, invalidate both sides of a native
ID collision (including pending withdrawals), retain unresolved evidence and
refuse pressure before admission. These data tokens are not G1 effect tickets.
The typed path cannot become a default until G1 qualifies exact original-edge
or original-ID release, recurring-token notification liveness, headroom and
rollback. An owner-scoped clear must never release unbound suppression.

Before a runtime two-owner Poster, the coordinator's owner/category/action
isolation contract still needs a pinned agreement. Current schema1 category
`im.received`, IDs, texts and default actions stay unchanged. Shared software
update writes do not establish whole-file quiet; any profile capture still needs
its applicable writer proofs and exact fresh-byte checks. No imported phase or
this codec admits native posting, withdrawal, activation or positive service IPC.

### Read-only combined inspection

`owned::inspection::inspect(Inputs { agent, update, legacy })` accepts only
caller-supplied optional byte slices. Its immutable, redacted `Inspection` checks
the structural subset: explicit expected-owner rows, no copied legacy quarantine
inside either owner envelope, unknown/incomplete inputs, combined 16 MiB/8192 raw
bounds counting the one legacy home once, and load-time cross-owner namespace/ID
collisions. Hex-case ambiguity is refused without normalizing original data.
A rejection never discards or rewrites the supplied evidence.

Getters return bounded decoded data, raw counts and SHA256 values of those exact
supplied bytes. A reported absence is not a verified missing file. Hashes are
not source freshness or writer proof; the actual handoff must securely read the
then-current legacy home, bind its presence/hash, and refuse/pin any later change.
An old cached legacy view must never be reused across rollback and re-entry.
The existing legacy owner must checkpoint retained Qt identities when required;
this helper discovers nothing and creates no seed store.

This inspection is not the full optional preparation. Provenance, recorded
invalidation entries, possible issued results, cross-file crash/commit ordering,
logical proof invalidation, negative permission fencing and current native lease
remain governed by the G1/G3 contract in `desktop-producer-drain.md`. The minimal
codec currently rejects unknown future fields for preparation. Inspection success
never permits activation, removal, posting, handoff or a typed default. No path,
reader, writer, mutable transition, source adapter or runtime caller is added.

### Passive provenance row codec

`owned::data::{decode_claim, decode_invalidation, Rows}` validates supplied
individual row bytes with an explicit expected owner and `Placement`. It exposes
immutable, redacted `Original`, `NativeBinding`, `AllowedActions`, `RetainedClaim`
and `Invalidation` data. Public types implement no serde serialization. The
explicit row encoders refuse unknown fields; G3 retains original bytes, ordering
and opaque fields separately. This is not an envelope reader or full preparation:
`decode_owned` is unchanged and still refuses to encode these unintegrated fields.

An `original` object records `request_id`, canonical non-nil `poster_instance`,
positive `post_nonce`, original `bus_id`/`daemon_owner`, positive
`namespace_generation` and nonempty duplicate-free `actions` (`default` and/or
`open-session`). Request text is nonempty, at most 512 UTF-8 bytes and has no
control characters. UUID spelling is checked before normalization; the existing
canonical validator is reused without a new UUID dependency. Original namespace
fields never inherit a later envelope namespace. Actions are retained metadata;
they do not choose a payload or authorize a callback.

The private wire tags are closed:

| `state` | `native_id` and placement |
| --- | --- |
| `possible_post` | No ID; uncertain only |
| `delivered` | Positive ID |
| `withdrawal_pending` | Positive ID |
| `unknown_post` | Optional positive ID; without an ID, uncertain only |
| `unknown_withdrawal` | Positive ID |

A tokens-map row must carry that same explicit positive ID; its map key never
supplies a missing ID. The future envelope decoder must validate canonical decimal
keys before passing `Placement::Tokens`. All positive uncertain, withdrawal and
unknown claims are included by `Rows::positive_claims`, not only delivered rows.
`Rows::new` rejects duplicate map IDs and repeated original Poster-instance/nonce
placement, including changed request, token, state or namespace. It checks raw
row count and bytes before any coalescing. Both envelopes, legacy and prepared
output still need the combined G1 preparation budget; this subset cannot do that.

Old owner/token-only rows remain explicitly `Historical` with
`missing_provenance`; no Delivered state or Original is synthesized. Partial,
null, conflicting or duplicate known provenance/state fields refuse. Every
invalidation contains the full exact agent `target` (owner, token, original and
positive native ID), plus one of `native_id_collision`, `namespace_changed`,
`withdrawal_conflict`, `unverified_result`. Only an explicitly selected update
envelope accepts invalidations. Each raw/encoded invalidation is at most 16 KiB.

There are no state-advance, reservation, result, withdrawal or release methods.
Loaded possible/unknown records never become Fresh. G1's namespace pinning after
a possible issued result without a durable ID, pre-issue barrier, cross-file
ordering, sticky logical invalidation, negative fencing and actual resource proofs
remain prerequisites for later mutable/runtime integration. No default, file,
source adapter, SDK, feature, lock or native authority changes here.

### Explicit owner envelope integration

`decode_owned_for(bytes, expected_owner)` selects the provenance-aware mode of the
same bounded raw-member decoder. `decode_owned` and schema1 quarantine retain
their ordinary behavior: new metadata is still unknown to the ordinary mode and
prevents its encoder from discarding it. The only dependency change is the pure
`serde_json/raw_value` feature in client/core; all four locked package graphs and
lockfiles stay unchanged. No native feature is enabled.

The selected mode borrows `&RawValue` from the bounded input slice and measures
each invalidation's original lexical value before decoding it (at most 16 KiB).
Internal whitespace and escapes count; canonical output length is not a raw
input limit. Outer and separator padding remain in the shared 16 MiB source cap.
No borrowed raw value escapes parsing and no scanner or boxed raw-value store is
added. Raw IDs, fields and original placements are checked before coalescing.
The selected mode exposes retained `rows()`; its bounded explicit encoder keeps
all recognized provenance, claim and invalidation fields and refuses unknowns.
The caller-selected Agent mode refuses root invalidations, including an empty
field. Only Update mode may carry complete exact Agent targets. Both selected
owner envelopes refuse copied legacy quarantine.

`inspection::inspect_selected` requires both supplied owner slices and counts
both envelopes and the single raw legacy home through that same parser before
row decoding or set projection. It bounds combined raw entries and original
bytes, then prepared encoded bytes separately. Missing provenance, unknowns,
possible/unknown posts without durable IDs, conflicting original identities and
all cross-owner positive-ID collisions refuse. Collision keys use original bus
ID, unique owner and server ID, including uncertain and withdrawal claims;
envelope namespace changes never relabel original provenance. Until the separate
verified release overlay exists, an unresolved exact invalidation conservatively
refuses this optional inspection as well.

Inspection hashes remain hashes of supplied bytes. Successful inspection is
still not full current-source preparation, quiet, a current mapping, lease or
numeric-ID permission. No mutable method, file access, source adapter, writer,
default/runtime caller or G1 ticket is added. All later commit, negative-fencing,
resource, crash, recurrence and rollback gates in the G1 contract remain open.

### Pure post admission and barrier acknowledgements

The first `owned::transitions` slice implements only supplied-data construction,
post-only admission, one prepared barrier image and exact host acknowledgements.
There is no native proposal/finish, clear, withdrawal, release, file access,
runtime caller or execution stub. The later full batch must carry exact
`NativeBinding` clears; token-only selection cannot authorize a loaded mapping.
G1 owns production identity construction, leases, effect tickets and the negative
permission consumer. The current `Notice` still supplies only token/title/body.

`Original::new(expected_owner, OriginalInput)` shares the wire identity checks:
the host explicitly supplies the original request, canonical non-nil Poster
instance, positive nonce, original observed namespace/generation and action set.
Nothing is minted from a loaded row, token hash, alias or current namespace.
The production host/factory is absent. Within a Model, nonces must increase
strictly, seeded from retained rows for that host; an admitted nonce never wraps or resets after
a write refusal. Request reuse within that host is refused too, using a bounded
(8192) exact set seeded from retained rows; it never evicts keys to admit more
work. The host must preserve its nonce/request origin across Model replacement.
Keys additionally bind a unique in-memory Model allocation, not merely that
host UUID and a counter. Loading creates evidence, never fresh preparations.

`Model::from_selected` consumes structural supplied-byte inspection, not a source
receipt. Its initial images keep exact supplied presence, SHA256 and byte length;
canonical encoding is never relabelled as original source evidence. Derived
indexes retain explicit scoped tokens, all original agent namespaces and shared
read-only legacy suppression. Original namespace spelling is preserved; only
collision lookup folds hex case conservatively.

`prepare_batch` validates the entire expected-owner `PostBatch` before changing
state, including original host/nonce, suppression and pressure. Bounds are 64
retained preparations, 128 posts/64 KiB of input UTF-8 per batch, and 4 MiB queued
input. The accounting includes all input strings and decimal counters. Combined
16 MiB/8192/legacy4096 limits include raw retained evidence. Before admission it
conservatively reserves the positive-result delta and all four static invalidation
reasons for every potentially affected agent binding, using full encoded targets
and the maximum positive server-ID width. Fixed closed-schema margins cover
positive-ID/state/key and namespace-header growth. Pressure cannot evict evidence.
This deliberately conservative headroom is not permission to issue anything.

Queued no-ID posts immediately suppress numeric routes in their original
namespace, before any future native completion. `namespace_blocked == false`
is not routing permission. `next_write` hands out at most one plan, with exact
selected owner, Model/step key, previous presence/hash/length, and next bytes,
hash and length. Repeating it cannot reissue the plan. There is one current
serialized buffer, not a file snapshot per queued preparation.

`ack_write(&mut WritePlan, WriteOutcome)` checks the exact Model, owner, step
and complete attempted images before consuming any current work. Foreign, stale
or mismatched acknowledgements leave the valid plan intact. The disposition is
host-reported `Saved`, `RefusedBeforeWrite` or `Unknown`; none proves physical
durability or source freshness. Saved/refused acknowledgement retires the plan's
payload handle too, so acknowledged images do not accumulate. A saved barrier
remains an unsettled preparation, with its namespace blocked: there is no native
dispatch API in this slice. Verified refusal remains review-only, without retry.

Unknown first sets sticky logical invalidation, then retains the exact pending
buffer and both last-acknowledged and attempted-unconfirmed images. It cannot be
retried, treated as unchanged, late-settled to success or cleared by a later write.
Counter saturation cannot revive the invalidated context. Lost handles leave
pending work; they do not demonstrate drain. A crash image containing the no-ID
barrier fails the existing selected inspection instead of creating fresh keys.

`close_agent_inputs` is sticky but admitted barrier continuations keep their
original identities. The first `freeze_agent_writes` conservatively refuses all
pending preparations, including updates, and retained uncertain/withdrawal
evidence. Its `FenceData` binds the Model/fence incarnation, negative generation
and reported agent image only. After a policy freeze, new agent admission is
closed; only update batches whose original namespaces are disjoint from retained
agent mappings may prepare writes. An unknown update commit invalidates the old
fence while leaving agent bytes unchanged. No diagnostic, key, acknowledgement,
closed flag or fence is an actual resource/current-source/lease receipt.

The accepted full policy still requires one native proposal at a time, original
kind/binding/namespace checks, terminal Unknown (positive IDs remain uncertainty),
cross-owner collisions including pending withdrawals, update invalidation durable
before dependent release/pre-fence agent change, and no post-fence agent writes.
Withdrawal acknowledgement records issuance, not visual removal. Those result
and clear paths are not implemented here. Actual persistence, child/reader/sink
and callback destruction, negative fencing, exact release, recurrence and rollback
need separate qualification before any typed default or profile ownership.

### Pure noncolliding post-result continuation

The next policy subset adds `take_next_native`, optional
`PostProposal::{original,title,body}`, fallible `report(&self, namespace,
PostOutcome)` and `finish_post(&mut PostProposal, PostReport)`. These are private-
field, redacted, nonserializing data handles, not native jobs or receipts. There
is no OS/file port, runtime/default caller or production identity factory.

A proposal binds its Model allocation, owner, preparation sequence, item,
complete original digest (including original namespace/generation and actions)
and exact acknowledged pre-issue Attempt. Only a fresh admitted item with a
Saved barrier is eligible. One proposal/result/write chain blocks every other
proposal or write until its required result acknowledgement. The result image
has priority over unrelated queued barriers. Lost handles retain their pending
chain and capacity; taking again never reissues them.

Before proposing and again on result handling, the selected envelope namespace
must match the original. A truly empty namespace may be bound in its first
barrier image only when no retained positive, uncertain, withdrawal or other
potentially active claim exists; it never relabels an original. Selected Model
construction refuses same-owner positive-ID ambiguity too. The source decoders
continue retaining those rows as evidence.

Report identity is checked before consumption. Foreign/stale/wrong-original or
barrier reports preserve the valid handle and current work. Namespace validation
precedes outcome classification: contradictory or malformed namespace cannot
become RefusedBeforeCall. Bounded valid observations are retained exactly;
malformed namespace input becomes a bounded unverifiable marker. An accepted
terminal report clears only the public proposal payload. The Model still holds
its exact original/payload/capacity through required acknowledgement or review.

Only unchanged-namespace, noncolliding `Delivered(positive_id)` can stage a
result image. Every retained positive claim from both owners participates,
including unknown and withdrawal claims. An occupied numeric map key refuses
even across different original namespaces. The result plan replaces exactly one
complete possible-post row, preserving all other rows and original provenance.
A reported ID is held in the collision set throughout the finish-to-result-ack
gap; no activatable route is returned. The precise result image's Saved
acknowledgement is required before another proposal can be handed out.

Collision, rotation, incompatible slot, RefusedBeforeCall and Unknown (including
an observed ID) instead invalidate the policy context first, retain the complete
bounded report/original/payload as terminal review, and produce no result write,
release, overwrite or synthetic pair commit. The durable no-ID pre-issue image
continues to refuse selected restart. Full update-first invalidation persistence
is still mandatory before any runtime/native port can use this restricted model.

A failed or lost result write follows an already observed native outcome. Its
original ID and payload remain held, with sticky invalidation on a reported
refusal/unknown and no second finish/proposal or late success. Any outstanding
plan is potentially changed disk, even before an outcome; `ImageReport` now
shows that attempted image immediately. Diagnostics and host Saved reports still
prove neither physical disk state nor durability/currentness.

Payload, preparation-slot and queued-input accounting retire only after every
item has noncolliding Delivered and its required result write is Saved. Retained
rows, request/nonce anti-reuse and required result/invalidation headroom remain
accounted, with no fictional FILE_LIMIT refund. Retired BatchKeys become stale,
not new identities. Proposal admission rechecks combined headroom; full G1
ownership preparation must also check it in the no-new-post case. Closed inputs
preserve original continuations, and freeze refuses unresolved/cross-owner work.
Post-fence uncertainty invalidates the earlier fence before dependent change and
never writes the agent file. Retirement is not worker, callback, sink or FD drain.

Tests here are fake-port policy vectors only. Actual resource/write-failure/kill
qualification, full cross-file collision commits, the G1 negative permission
consumer, exact original-edge release, recurrence and rollback remain required.
No `.22/.23/.24/.25` positive admission or typed default follows.

### Pure update-first collision continuation

The optional Model now retains a restricted negative write chain for a current
positive-ID result that collides with retained claims in its unchanged original
namespace and affects an agent original. Both Delivered and Unknown observations
with a positive ID qualify; the latter retains its Unknown outcome. Existing
Delivered, UnknownPost, WithdrawalPending and UnknownWithdrawal claims all
participate. Update-only collisions, changed or malformed namespaces, no-ID
results and post-fence overlaps remain terminal review without a continuation.
An occupied numeric map slot in the incoming owner's envelope must retain valid
original provenance in that same namespace. A foreign or ambiguous slot remains
terminal even when another claim supplies a same-namespace collider.

`finish_post` invalidates the context and advances its sticky negative generation
before exposing a collision plan. No new batch, native proposal, ordinary write
or fence can pass. `WritePhase` and `Attempt::phase` distinguish the pre-issue,
ordinary result, collision-invalidation and collision-result images. Exact attempt
matching also binds the original digest, including namespace generation and
allowed actions, alongside Model/owner/preparation/item and complete before/after
images. A barrier does not invent an individual original digest.

The first collision image always selects SoftwareUpdate. It appends full exact
agent targets with `native_id_collision`, preserving every prior claim and
namespace. If the incoming original belongs to SoftwareUpdate, that same image
changes only its exact possible-post uncertain row to UnknownPost with the
observed positive ID. If it belongs to AgentAttention, only an exact Saved
acknowledgement of the update image enables the dependent agent image. That
second attempt retains the actual internally acknowledged predecessor attempt;
its result likewise changes only the exact possible-post row to UnknownPost.
Neither image overwrites an occupied positive map slot or matches by token alone.

Only this exact chain may write under sticky invalidation, with one outstanding
buffer. Unknown, refused and lost writes stay pinned, with no replacement plan,
late-success settlement, rollback or replay. Saved retires the public plan bytes
but retains the original report, payload, preparation and capacity. Even after
the final Saved, the context stays invalidated and Review. There is no successful
native completion or pair-atomicity claim. Invalidations reserve their full
escaped worst-ID sizes before a proposal, including loaded agent originals;
each row must fit 16 KiB within the existing combined limits.

Pure tests feed old, barrier, first, final and partial image pairs through selected
inspection and then Model construction when inspection succeeds. Same-owner
positive duplicates require the Model check as well. These are structural restart
controls, not filesystem failure/kill, actual durability, permission-consumer,
native resource, current-source, lease, release, recurrence or rollback proof.
The G1 negative permission consumer remains a separate prerequisite. No runtime
caller, source API, default, native wire behavior or ownership changes here.

### Dormant startup resource prerequisite

The crate-private `platform::linux_notify::startup` path has no production
caller. Its explicit local Unix address and NativePosting charge construct one
startup worker, one connection with the internal zbus executor disabled, and
two logical readers over a shared raw queue of 16. Both AddMatch acknowledgements
precede either raw reader's activation. The installed schema1 Poster keeps its
existing constructor and behavior. Future installation must atomically select
one constructor and preserve unresolved owners; this helper supplies no global
installation exclusivity.

`OwnedControl` is weak and close-only. Close clears readiness and fences inputs
before cancellation/wake, performs no native I/O or join, and never renews the
original cleanup deadline. The setup deadline starts before worker handoff;
setup and cleanup each have one total budget of at most five seconds. Native
construction and polling run outside Tokio on the worker. No mutex poison
recovery, stopped flag, timed wait or empty Tokio executor establishes completion.

`OwnedStartup` retains the original worker JoinHandle and armed accounting
completion. Both raw readers and borrowed messages drop before any RemoveMatch.
Exact removal acknowledgements, connection close/drop, executor task completion
and disappearance of the worker's registered wake captures precede its private
CleanExit. Only off-GUI `try_finish` joining that exact successful worker can
complete the accounting guard. Finished is terminal for this resource slice;
failure, lost handles, late completion, panic, poison and uncertain cleanup pin
the original fixed group. A positively known no-native-work path still requires
the actual worker join. Weak controls cannot manufacture a finished owner.

The locked async-io reactor and driver are once-only shared process infrastructure
outside per-startup accounting. They remain alive; Finished claims no process or
whole-role quiet. Each startup still owns its registrations, sockets, futures,
wakers, timers, messages, captures and executor tasks. Private qualification
combines exact peer/capture destruction and worker joins with warmed socket/thread
baselines and repeated starts. Counts alone do not establish resource custody.

Queue 16 is an item bound. The locked SDK can receive frames up to 128 MiB before
body allocation and has no smaller Builder receive-size knob. Startup code does
not decode, copy or route bodies, or retain them beyond the SDK queues and current
borrowed read. It installs no app sink and supplies no production bounded-byte
ingress or memory qualification. That gate remains before native method/callback
adoption.

There are no Notice, PostBatch, cleared-token, native-ID, Original, Model, path,
QtSeed, source/store, writer/ack or Desk/main-thread sink inputs. No Notify or
CloseNotification is sent. G3 Staged bytes cannot activate this constructor.
Agent-only completion while updates stay live, posted/results/write/sink work,
GTK observation, clear/withdrawal/release, recurrence, rollback and replacement
under load remain separate gates. The G1 prepared-startup exclusion/current
profile/negative consumer and whole-profile qualification remain required before
any production caller or ownership transition.
