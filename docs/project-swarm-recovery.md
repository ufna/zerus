# Project swarm recovery

The Next recovery UI reuses `swarm::sync::Controller`, the native organization
worker/writer and the existing `hgs swarm preview/join/resolve/sync` contracts.
The UI follows `design/webui/src/dialogs/setup/Swarm.tsx`; Qt's `SwarmDialog.cpp`
and `SwarmConflictReview.cpp` define behavior and exact conflict choices.
Opening or inspecting never joins, binds or resolves a catalog.

## Bounded view

`swarm::dialog::view(snapshot, revision)` projects the existing observed catalog
into the Computers tab's identities, local access and sync states. It copies at
most 512 computers and 1 MiB of text (plus fixed row overhead); individual labels
are at most 512 bytes and peer errors 4,000 bytes. Malformed, duplicate or
oversized data refuses the whole view, never a prefix. Conflict count is metadata;
conflict payloads remain in the existing native snapshot for explicit review.
There is no source read, poller or UI caller in this prerequisite.

## Catalog-operation reservation

`Controller::begin_external(revision)` accepts only the exact current revision
of an initialized, settled catalog, without applying/pending edits or a held
observation. The opaque `ExternalTicket` is bound to that controller by private
in-process identity, carries its immutable observed snapshot and has redacted
Debug output. It is never serialized or accepted from the webview.

While reserved, ordinary edits, another reservation and scheduled applies are
refused; source observations are held without adopting them. Dropping a ticket,
a source failure, timeout or source termination does not release the reservation.
Nothing retries a Join or Resolve.

`finish_external(ticket, observation, now)` accepts one bounded, validated,
initialized snapshot for the same local node. The future native caller must
serialize the external operation with this worker, invalidate pre-command source
reads and obtain this snapshot **after** that operation finishes, even when its
outcome is unknown. Neither a command error nor a cached topic snapshot proves
that completion. A failed fresh read keeps the reservation closed for review.
If an unknown delivery could still complete later, a single snapshot is also
insufficient: operation-specific settlement is required before release. The
future native integration must qualify that behavior rather than infer it from
the decoder, an exited wrapper or a timeout.

Completion discards observations held during the operation. The ordinary
10-second causal-head guard protects the result's versions, and the source must
also echo its completed catalog identity before normal observation admission
resumes. An old catalog identity is discarded, never queued for later admission;
elapsed time cannot qualify it. This helper does not establish source freshness
or perform that native source barrier itself.

There is no native dispatcher/caller in this first slice. These are GUI catalog
coordination primitives, separate from G1 production migration: no service lease,
role drain, source export, marker phase or effect permission is granted.
