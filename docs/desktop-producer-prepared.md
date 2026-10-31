# Durable prepared producer fence

The internal `src/desktop/producer_prepared.rs` stores negative preparation in
the existing desktop SQLite owner, schema 9. No production producer/controller
calls it yet. It neither closes a role nor creates a Frozen, Drained, Complete,
abort-completed or writer admission receipt.

The future sole service owner consumes its authenticated `Session` into an
owned preparation Context, retaining the original request UUID, existing handoff
lock and presence fence through a fresh canonical identity probe and preparation.
See [startup exclusion](desktop-startup-exclusion.md) for its ordering and the
actual frontend startup consumer. The helper checks both locks'
configuration and the Store's current service epoch inside the transaction,
and the session's exact clean live frontend/configuration/producer association.
It binds the fixed background-v1 definition and fresh prospective writer,
drain and import UUIDs. These are identities only, with no role permission.

One fixed row holds at most 8 KiB of closed JSON and its domain-separated SHA256.
The original request and all identities are immutable. The existing FULL SQLite
commit precedes directory sync and acknowledgement. Failure may follow commit;
retain the original request and inspect it, never infer non-delivery from a lost
reply or retry under another request. An exact same-session duplicate returns
only the existing snapshot, with no new mutation key. Changed request/session
identity refuses without replacing the row. Service restart never clears or
rebinds its original epoch.

The only later mutation records `abort_requested` under that exact original
key and current service epoch. It remains fenced, including after restart.
There is no clear or completed-abort API; neither timer expiry, connection loss,
ordinary writer revoke nor an abort request restores frontend writers. Returning
ownership needs separately qualified exact durable reconciliation and a new
explicit owner/gate decision. Old gates never reopen.

Readback takes at most two bounded rows. Extra membership, malformed/unknown
fields or phases, digest mismatch, newer binding versions and changed identity
stay unknown. A missing table in a schema that promises it is an error. Existing
legacy presence admission treats either retained phase as fenced under its
original presence lock, even when ordinary writer policy is revoked. Schema 8
and older retain their existing behavior. Windows preparation/abort mutation
refuses before I/O pending native durability qualification.

A read snapshot, including absence, supplies no startup authority. The actual
[replacement frontend consumer](desktop-startup-consumer.md) now retains the
same boundary through canonical startup and refuses unsettled state before GUI
writers, including when the service is absent. Producer preparation still has
no production caller; persisted-original reconciliation remains required first. Current profile/known-zero Presence proof,
continuous negative invalidation, actual SDK/resource/store barriers and whole
profile publication/rollback remain separate required gates. This journal does
not change the existing handoff-v1 marker or relabel historical Staged evidence.
