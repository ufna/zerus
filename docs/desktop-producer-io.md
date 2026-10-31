# Bounded producer control-line I/O

The dormant internal `src/desktop/producer_io.rs` helper reuses the existing
`protocol::line` and owned desktop stream/reader. It adds no production caller,
controller, thread, listener, resolver, FD clone, semantic frame, command, store
or positive API. Existing desktop and passive feed framing/timeouts are unchanged.

Producer control lines are capped at 16 KiB including the final LF. A write
requires one nonempty line; it never splits, retries or truncates an oversized
payload. Reads preserve following bytes in the original BufReader rather than
discarding/replacing it or opening another reader. The host still bounds that
reader and validates UTF8, its closed schema, sequence, identity and phase.

Every operation takes one absolute monotonic deadline, with at most fifteen
seconds remaining (the agreed producer idle limit). It checks before each
partial read/write and after completion, and applies only the remaining time
to each syscall. Partial progress cannot renew the deadline. Already buffered
data is rejected after expiry without consumption. The dedicated control stream
retains its last timeout setting; each later admitted operation must set its own
deadline. Windows refuses these new operations before I/O pending qualification.

A successful write means local bytes were written, never peer acceptance,
current ownership, a native outcome or a durable receipt. A failed/late write
may have sent a prefix or a complete frame; it is no replay or resume permission.
An EOF means this connection ended, not that no instance exists or resources
drained. Errors leave ownership with the host; this helper never closes a gate,
releases a charge, checkpoints a store or restores GUI writers. Future control
must retain the actual reader/writer/worker through completion and apply the
existing unknown/prepared/frozen/revoke rules and original-ID reconciliation.

Private Unix-pair tests cover exact/over-bound lines, retained buffered tails,
expired buffered data, EOF/truncation, slow progress, blocked writes and refusal
before payload I/O. They are framing coverage only, not a producer handshake,
authenticated current lease, role proof or controller/native qualification.
