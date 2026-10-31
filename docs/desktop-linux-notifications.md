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
