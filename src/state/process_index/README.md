# Inactive process-index foundation

The coordinator approved this first slice after the [design record](../../../docs/superpowers/specs/2026-11-14-process-index-person-authority-design.md).
There are no production callers, default state paths, publishers, bootstrap workers,
caller-policy changes, Person UI routes or task-control activation.

`Store` accepts an explicitly supplied SQLite connection. Its metadata scope and
persistent random epoch prevent accidental cross-store use; they do not qualify
that connection's filesystem/VFS custody. The future owner must retain the native
namespace, verify canonical records and precise process identities, establish every
writer's coverage/floor, and recheck the returned stamp after native observation.
`finish` records owner-declared scan/catch-up completion, not proof that a filesystem
inventory occurred. Neither `Candidate` nor `NoMatch` grants Person authority.

Intents are immutable and retained. Pending intents fence negative coverage;
settlement and projection changes commit together. Scan pages cannot overwrite a
post-scan change or a newer revision. No age expiry, repair, journal pruning or live
claim eviction is implemented. Journal admission is capped at 262,144 retained
intents; a new database has a 512 MiB SQLite page quota. Capacity refuses work.

Ancestry input is at most 64 distinct PIDs. Entries are at most 4 KiB, intents
16 KiB, and scan/read pages 128 rows and 64 KiB of row payload. All ancestry and
keyset queries pin their named indexes; missing indexes refuse instead of scanning.

The explicit `process_index_bench` example imports the exact module source. Its
synthetic private database measures only index work at 10k/100k sessions and 64
modeled ancestors, including anchor VM/full-scan steps and metadata reads. It does
not measure OS ancestry, canonical-source I/O or native caller qualification.
The same private-store controls also run under the normal root binary's test module.

Explicit checks and benchmark (use the shared heavy wrapper for managed runs):

```sh
cargo +1.99.1 test --locked -j 4 --example process_index_bench
cargo +1.99.1 test --locked -j 4 --bin hgs state::process_index::tests::
cargo +1.99.1 run --locked -j 4 --example process_index_bench
```
