# Source baseline validation

Validated on Linux on 2026-10-07 for the fresh repository snapshot:

| Check | Result |
| --- | --- |
| `cargo test --locked` | 133 passed |
| `cargo build --locked` | Passed |
| CLI shell smoke suite | 202 passed |
| Installer transport/configuration tests | 6 passed with isolated fake SSH and HOME |
| Account Python suite | 26 tests, one platform skip, no failures |
| Qt GUI build | Passed |
| Sequential CTest GUI suite | 30 suites passed |
| Source privacy guard | No findings; positive/negative probes verified |
| Gitleaks source and fresh Git history | No findings |
| Gitleaks fresh Beads revisions, events and memories | No findings |
| Binary asset metadata review | No personal paths or credential markers found |
| Independent Git + Beads clone | Source identical; fresh issues and memories restored |

The parallel GUI run exposed a timing-sensitive account-loading test; it is tracked
as `zerus-v1p`. The source snapshot also corrects a stale test that previously
expected a missing native account to enable launch, contrary to the current
explicit-account behavior. No product account-selection logic changed.

macOS builds and native-agent integration suites were not rerun for this source
migration. The installer configuration changes were tested with fake transports;
no live SSH configuration, agent session or installed GUI was changed by migration.
The original source baseline had separate platform validation before migration.

This records the snapshot review, not future commits or a public binary release.
