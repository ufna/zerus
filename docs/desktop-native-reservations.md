# Native reservation data for the desktop actor

`zerus_desktop_ledger::native::inspect` is a dormant read-only prerequisite for
the Linux Post/Withdrawal worker. It reads an existing reservation; it never
reserves, dispatches, settles, retries or creates a lease. There is no production
caller, worker, SDK operation, migration or positive desktop command in this
slice. The existing [Linux native port](desktop-linux-notifications.md), GUI
poster and schema1 saved identities keep their current behavior.

The actor supplies the complete original `Reserve`. Under one deferred read
transaction, the unique request-ID lookup requires its exact reservation
fingerprint and stored intent/revision, writer epoch, service epoch, adapter,
lease and optional retry binding. The writer must still be unrevoked at those
epochs. The attempt must be reserved without a receipt, and the intent must be
current, unwithdrawn and not a historical/focus-suppressed baseline. Only session
Post and Withdrawal identities are supported; Badge and Presence are refused.

The identity is at most4096 UTF-8 bytes and must encode to the exact original
identity and intent digest. The saved JSON payload is at most64 KiB. SQL caps
both before materializing either string into Rust. Invalid JSON and trailing
bodies refuse. The snapshot retains the exact saved payload bytes and their
SHA256, including opaque payload fields. These are database intent bytes at that
revision, not a claim about original import or native wire bytes. The existing
`put_intent` contract forbids changing a revision's payload. No original source
file, import, token owner, category or native endpoint is inferred from it.

The returned `Snapshot` has private immutable fields, redacted Debug and no
Clone or serde-Serialize implementation. Its bounded accessors expose only the
saved request, sequence, identity, payload and digests. The read transaction ends
before returning. A later intent or writer change does not mutate an already
returned snapshot; it also does not keep that snapshot current.

Re-reading a reserved row is never permission to send. It may already have
reached the OS, or be awaiting an unknown outcome. Inspection cannot reproduce
the original `fresh=true` reservation response, renew a lease, grant profile or
frontend-drain proof, or resolve lost delivery. A future actor must separately
bind the original fresh reservation to its live configuration/profile, current
writer/service/adapter leases, expected agent owner, bounded payload, exact
daemon namespace and actual worker lifetime. It must recheck these at dispatch.
Native-ID mapping, activation tombstones, per-owner categories/actions and
legacy release remain separate qualified prerequisites. An imported marker,
serialized phase or this data snapshot supplies none of those permissions.

After revoke or withdrawal this lookup refuses a new snapshot; existing
`finish` still settles only the exact original already-reserved receipt under
its existing immutable fingerprint rules. This helper never changes reservation,
receipt, recovery or rollback behavior. Parent `.66.3.24` and authenticated
frontend drain `.66.3.23` remain incomplete and fenced.

Isolated SQLite tests cover the complete request and stored binding, epochs,
revoke/withdrawal/baseline refusal, terminal attempts, corrupt/oversized data,
exact-byte retention, no schema/data writes and unique indexed query plans.
They qualify this read helper only, not an actor/worker, native API or live lease.
