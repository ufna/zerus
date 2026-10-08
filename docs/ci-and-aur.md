# CI and Arch Linux publication

This document describes the checks, package layout and publication workflow.
[Zerus 0.37.0](https://github.com/ufna/zerus/releases/tag/v0.37.0) is the first
public Arch x86_64 bundle. Release builds and GitHub/AUR publication remain
separate manual workflows.

## Continuous integration

Hosted checks are **manual-only** to preserve the account's Actions quota. There
are no CI triggers for pushes, pull requests or schedules. The owner decides when
to run checks, including for Dependabot PRs. Do not dispatch hosted checks just to
validate a workflow change; run actionlint and relevant checks locally instead.

`.github/workflows/ci.yml` accepts manual dispatches and explicit calls from the
manual release workflow. All expensive suite inputs default to `false` in both
manual and reusable CI. A default dispatch uses **one Ubuntu job** for source
checks and Rust unit tests; it allocates no Arch or macOS runners.

| Job | Default manual CI | Selection | Checks |
| --- | --- | --- | --- |
| Quick checks | Always | One Ubuntu job | Privacy guard, Python/Bash syntax, version consistency, actionlint, publication contract tests, locked unit tests on Rust 1.85.0 |
| Linux CLI | Off | `run_linux=true` | Stable Rust unit tests, terminal smoke tests, all Python integration modules, release build |
| Arch desktop and package | Off | `run_arch=true` | Qt desktop build, all CTest suites, production build without test targets, pacman package, namcap, staged and installed bundle checks |
| macOS CLI and desktop | Off | `run_macos=true` | CLI checks and native Qt build with all CTest suites; two macOS jobs |
| CI gate | Only with extra suites | Automatic within a selected run | Requires quick checks and every selected suite to succeed; unselected suites must be skipped |

The expensive jobs start only after quick checks pass. The macOS CLI runner is
added to the matrix only when requested; disabling it does not allocate a runner
that merely skips its steps. CI gate fails on errors, cancellations and unexpected
skips. Quick-only runs omit this extra job to avoid another runner startup.

### Manual checks, including pull requests

In GitHub **Actions → CI → Run workflow**, keep the workflow branch on `main` to
use the current budget controls. Leave the test `ref` blank for that commit, or
enter a branch, full commit SHA or `refs/pull/NUMBER/head`. Select only the extra
suites needed for that change. This also checks older PR branches using the new
workflow definition. [GitHub manual workflow instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

```sh
# Quick source and minimum-Rust checks only:
gh workflow run ci.yml --ref main
# Check a PR's current head with the same inexpensive defaults:
gh workflow run ci.yml --ref main -f ref=refs/pull/123/head
# Full Linux CLI and Arch desktop/package coverage for a PR:
gh workflow run ci.yml --ref main -f ref=refs/pull/123/head \
  -f run_linux=true -f run_arch=true
# Explicitly opt into both macOS suites when native coverage is needed:
gh workflow run ci.yml --ref main -f run_macos=true
```

Quick checks resolve the target once. Every selected job checks out that exact
commit; the summary records its SHA and selected coverage, and the Arch artifact
name contains the tested SHA. A PR updated later needs a new deliberate dispatch.
Use the summary's tested SHA when reviewing an explicit test `ref`: GitHub's run
metadata remains associated with the workflow branch, not the checkout override.

Do **not** require CI gate as an automatic PR status check under this policy. A
manual run may intentionally omit it, and no check is produced until the owner
requests one. Review/merge policies are independent of hosted CI.

Arch is the Linux GUI build environment because the application uses Qt 6 and KDE
Frameworks 6. Ubuntu remains useful for portable CLI and minimum-Rust checks.
Linux CLI CI builds the pinned official tmux 3.7c archive with an exact SHA-256;
Ubuntu 24.04's tmux 3.4 lacks `bracket_paste_flag`, introduced in tmux 3.7. That
format is required by the existing verified agent input transport. Unknown input
mode must keep rejecting delivery; upgrading the test toolchain preserves that
guard. The package requires tmux 3.7+. A binary upgrade must not restart live
servers; see the [installation requirements](installation.md#requirements).
Current release packaging supports **Arch x86_64 only**. The macOS runner checks
build/test compatibility; it does not create a notarized macOS installer. Windows
and Arch ARM are outside this first package matrix.

All CI jobs have bounded timeouts. A newer manual CI run cancels an obsolete run
for the same requested ref and workflow. Workflows have read-only repository
permissions, checkout credentials are not persisted, official actions are pinned
to commit SHAs and Dependabot proposes weekly action/Cargo updates. Rust and C++ caches reduce repeated build
cost. Test artifacts expire after seven days; release candidates after fourteen.
Manually selected PR code receives no deployment credentials. There is no
push/tag/release-triggered publication or self-hosted runner attached to a
development machine. The owner explicitly starts each candidate and publication.

The Arch base image is digest-pinned, but `pacman -Syu` intentionally uses current
Arch packages. This tests rolling-distribution compatibility; it does not promise
bit-for-bit reproducibility months later. Rust dependencies are locked, fetched
in `prepare()` and built/tested with `--frozen` inside source packages. The CI
image, tmux and official DeepSeek fixture pins need periodic maintainer review.

### Test isolation and local commands

```sh
python3 scripts/ci/check-source.py
bash scripts/ci/cli.sh
bash scripts/ci/gui.sh
cargo +1.85.0 test --locked
# Arch only, as an ordinary user with the documented build dependencies:
bash scripts/ci/arch.sh
```

The CLI wrapper clears inherited `HGS_*` account/session selection before setting
its own `HGS_TEST_BIN`. Python integration modules run in separate processes so
module patches do not leak between suites. Terminal fixtures use private socket
paths and synthetic homes. No check runs against existing agent sessions.
Authenticated native-client tests remain explicit opt-ins; baseline CI needs no
model API keys. Linux installs the pinned official `@deepseek-ai/dsh@0.2.0-rc.2`
for its keyless local-model contract fixture. Native agent CLIs are not included
in the product package.

No blanket formatting/clippy gate was added over legacy source: it would reject
unrelated existing formatting rather than verify this change. Existing unit,
integration and Qt suites provide the initial gate. Add stricter linting with a
separate, reviewed baseline cleanup. Logs are under ignored
`artifacts/test-results/`; packaging outputs are under ignored `artifacts/` and
`dist/`.

## What yay actually installs

`yay` is an AUR helper. AUR hosts a public `PKGBUILD` and generated `.SRCINFO`,
which tell users how to obtain and build a pacman package. Binary release archives
live upstream; they are not uploaded into AUR Git.
[AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines),
[PKGBUILD manual](https://man.archlinux.org/man/PKGBUILD.5.en).

The owner selected this package family on **2026-10-08**:

| Name | Source | Purpose |
| --- | --- | --- |
| [zerus](https://aur.archlinux.org/packages/zerus) | Versioned source archive with SHA-256 | Stable release compiled locally |
| [zerus-ade-bin](https://aur.archlinux.org/packages/zerus-ade-bin) | Versioned Arch x86_64 binary archive with SHA-256 | Stable release without Rust/C++ compilation |
| [zerus-git](https://aur.archlinux.org/packages/zerus-git) | Public upstream `main`, version derived by `pkgver()` | Development build compiled locally |

Primary owner/maintainer: **ufna**, **Vladimir Alyamkin <ufna@ufna.dev>**.
All three public AUR package pages confirm the approved maintainer. The publisher
rechecks ownership and SSH write access before writing. All flavors install the
same files and conflict with one another. The binary/VCS flavors provide versioned `zerus`.

Open-source prebuilt packages require the `-bin` suffix. `zerus-bin` belongs to
an unrelated project, so our binary package uses `zerus-ade-bin`.
[AUR naming rules](https://wiki.archlinux.org/title/AUR_submission_guidelines),
[existing zerus-bin package](https://aur.archlinux.org/packages/zerus-bin).

Stable recipes are templates until candidate assembly inserts the release version,
archive hashes and binary library floors. The VCS recipe tracks `main` explicitly;
its seed version is updated by makepkg. `SKIP` is only for its Git source.
Generate `.SRCINFO` with `makepkg --printsrcinfo`; never edit it manually.
[Rust package guidelines](https://wiki.archlinux.org/title/Rust_package_guidelines),
[VCS package guidelines](https://wiki.archlinux.org/title/VCS_package_guidelines),
[.SRCINFO](https://wiki.archlinux.org/title/.SRCINFO).

### Dependency declarations

| Class | Packages | Reason |
| --- | --- | --- |
| Desktop runtime | `qt6-base`, `qt6-webengine`, `qt6-svg`, `kstatusnotifieritem`, `kwindowsystem`, `hicolor-icon-theme` | Widgets, embedded web UI, SVG plugin, tray/window integration and icons |
| CLI/runtime tools | `tmux>=3.7`, `openssh`, `python`, `curl`, `procps-ng`, `bash`, `tar` | Persistent sessions, remote access, compatibility helper, account HTTP, process inspection, installers and remote source transfer |
| Native library runtime | `glibc`, `libgcc`, `libstdc++` | Dynamically linked CLI/desktop binaries |
| Source build only | `rust`, `cmake`, `ninja`; additionally `git` for `zerus-git` | Locked Rust and production Qt builds; Git source/version discovery |
| Standard build environment | `base-devel` | Required by Arch's makepkg/AUR build convention; its tools are not repeated in makedepends |
| Optional features | `nodejs`, `npm`, `git`, `konsole`, `wl-clipboard`, `xclip`, `systemd` | Native agent installation/runtime, worktrees, external terminals, clipboard and user services/scopes |

The binary package has no Rust/CMake/Ninja build dependencies. Its Qt/KDE, C/C++
runtime and glibc dependency floors are generated from the **verified builder's
`.BUILDINFO`**, not the publication runner. `release-info.json` records these
versions. This prevents installing that binary against older libraries that may
lack symbols; Arch users should keep a fully updated system. A fresh binary/ABI
rebuild gets a new product version and immutable assets.

Node.js and npm are separate optional dependencies. Native agents are installed
separately through Accounts or their upstream installers; agent executables,
credentials and user configuration are never bundled. Systemd integration is
optional and detected at runtime. Packaging never enables services.

### Package contents and compatibility

Each flavor installs the same ADE bundle:

- `/usr/bin/hgs`, `/usr/bin/hgs-tray`, the `hgs_state.py` compatibility shim and
  the explicit `zerus-setup` migration helper.
- A desktop entry, existing application icons and system-provided **user** unit
  definitions for `hgs-tray`, `hgs-swarm` and `hgs-recovery`.
- Documentation and a sample tmux configuration under `/usr/share/doc/zerus`.
- Zerus MIT, vendored libvterm MIT and the exact host-target Cargo dependency
  license texts under `/usr/share/licenses/<package-name>`.

Qt, Qt WebEngine and KDE libraries are dynamically linked system dependencies.
`qt6-svg` supplies the dynamically loaded SVG image plugin used by menu and
combo-box arrows; a Qt test renders the actual bundled resources so a missing
plugin fails CI. The current Arch Qt base package supplies its Wayland display
plugin. [Arch Qt SVG package](https://archlinux.org/packages/extra/x86_64/qt6-svg/).
The runtime also declares curl for native account HTTP requests and procps-ng
for process inspection. Optional Node.js/npm enable agent installation from
Accounts; Git enables repository/worktree features. Arch's Node.js package does
not include npm, so both optional dependencies are listed.
The package does not bundle their distributions, native agent executables,
accounts, credentials or configuration. Package installation does not start or
stop services, edit user homes or replace the user's tmux configuration.

The product package version currently follows the desktop version: `VERSION`
and `tray/CMakeLists.txt` are **0.37.0** for the first public release. The independently versioned compatible
CLI remains **1.46.1**. CI rejects a product/desktop version mismatch; packaging
records both versions in `build-info.json`, with the source commit when built
from Git. For source-archive builds, the release manifest and archive SHA-256
provide the commit provenance.
Increase the product/desktop version for each new public bundle, including a
CLI-only bundle change, so published archive URLs and hashes remain immutable.
A recipe-only `pkgrel` bump can reuse immutable upstream assets. A newly compiled
binary needs a new product version under the prepared archive naming scheme;
do not replace an existing release archive during an ABI rebuild.

Existing runtime paths and IDs remain `hgs`/`HGS_*`/`hgs-tray`.
The desktop entry keeps the `hgs-tray.desktop` ID so existing launchers remain
associated with the same application. A local source-install entry can shadow
the system entry; inspect it deliberately during migration.
Remote SSH protocols call `~/.local/bin/hgs`. Pacman cannot install into arbitrary
user homes, so each user runs this once, **without sudo**:

```sh
zerus-setup
```

It creates compatibility symlinks to `/usr/bin` only after validating every
target and conflict. It is idempotent, preserves existing files and dangling
symlinks, and refuses root execution. An existing source installation needs an
intentional migration: inspect and move aside conflicting executables before
running it. Do not remove `~/.config/hgs` or agent/session state.

Old user unit files in `~/.config/systemd/user` can shadow packaged definitions;
inspect `systemctl --user cat hgs-tray.service` and the worker units before
switching. Back up old definitions deliberately. Enable background services only
when wanted:

```sh
systemctl --user daemon-reload
systemctl --user enable --now hgs-tray.service
systemctl --user enable --now hgs-swarm.service hgs-recovery.service
```

Packaged units and the current source installer template use `KillMode=process`.
A GUI restart must preserve native agents, tmux servers and DeepSeek hosts. Older
source-installed units
may still use systemd's control-group default. Before restarting one, add a user
drop-in with `[Service]` and `KillMode=process`, run `systemctl --user daemon-reload`
and verify `systemctl --user show hgs-tray.service -p KillMode`. Package upgrades
do not perform a restart. Review the old unit's kill policy before restarting an existing
installation; never use a broad process kill or restart all native clients.

## Manual build and publication

The public upstream is [ufna/zerus](https://github.com/ufna/zerus). There are two
independent manual workflows. Neither runs on pushes, pull requests, tags,
GitHub releases or a schedule.

### 1. Build and review a candidate

`release.yml` runs quick checks plus full Linux CLI and Arch validation from one
exact commit. macOS stays off unless explicitly selected. It consumes that run's
Arch package, verifies its embedded version/source commit, assembles source and
binary archives, rebuilds both stable recipes, runs namcap and seals the result.

```sh
# Only when the owner requests a hosted build:
gh workflow run release.yml --ref main -f version=0.37.0
# After this manual run succeeds:
gh run download RUN_ID --name zerus-0.37.0-candidate --dir /tmp/zerus-candidate
cd /tmp/zerus-candidate
sha256sum -c SHA256SUMS
```

The artifact contains source/binary archives, two checked pacman packages,
`release-info.json`, all three concrete `PKGBUILD`/`.SRCINFO` pairs, their sealed
`zerus-VERSION-aur.tar.gz` archive and `SHA256SUMS` covering every release asset.
Git export attributes omit tracker, agent instructions/skills, workflows and
prototype artwork from the source archive; runtime artwork and notices remain.

Review the source commit, package logs, dependency/license inventory and recipes.
`create_draft=true` additionally creates a GitHub draft; by default everything
remains in the candidate artifact. Candidate artifacts expire after fourteen days,
logs after seven. Old 0.36.2 artifacts predate the new names/schema and cannot be
published by the new workflow. Never substitute a package built from another SHA.

Equivalent local assembly on Arch, with a clean committed checkout and its exact
verified package, remains available:

```sh
python3 scripts/prepare-release.py --version "$(cat VERSION)" \
  --package artifacts/zerus-git-EXACT_VERSION-x86_64.pkg.tar.zst --output dist
bash scripts/check-release-packages.sh dist
```

The helper requires an empty output directory. Build artifacts are ignored and
are never committed to the application or AUR repositories.

### 2. Publish the reviewed candidate and update AUR

`publish.yml` takes the **successful candidate run ID** and its version. It uses
one Ubuntu runner, does no compilation and allocates no Arch/macOS runners:

```sh
# Only after review and an explicit owner publication request:
gh workflow run publish.yml --ref main \
  -f version=0.37.0 -f candidate_run=RUN_ID
```

The publisher verifies the same-repository manual run came from `main`, succeeded,
and contains exactly one unexpired versioned artifact. It checks GitHub's artifact
SHA-256, the manifest's exact source commit, all asset checksums, sealed AUR files
and public recipe URLs. Candidate source/recipe code is never executed with the
publication credential.

Before any GitHub release write it authenticates to AUR, checks ownership of all
names, prepares Git changes and refuses recipe downgrades. It creates or finishes
a draft for the exact commit, publishes it, verifies **every asset anonymously**,
then pushes the stable source, binary and VCS recipe updates. AUR Git receives only
`PKGBUILD` and `.SRCINFO`; release archives stay on GitHub.

The VCS flavor follows `main` automatically when users rebuild. Release-time sync
updates its recipe when dependencies/build/package logic change; pure `pkgver`
bumps (including the corresponding versioned provides) do not create AUR commits.
Arch's submission rules explicitly disallow version-only VCS updates.

Publication runs are serialized across versions. Existing public assets must
match exactly; missing or different public assets stop publication. Uploads never
use `--clobber`, tags never move and AUR pushes never force. An interrupted draft
can resume only with matching existing assets and the exact commit. If an AUR push
fails after GitHub publication, **rerun Publish release with the same candidate
run and version**. Completed recipes become no-ops; the remaining updates retry.
The two services are not an atomic transaction, so inspect the failed job's output.
Do not rebuild or replace published archives to recover a partial AUR update.

Publication retries require the same retained candidate artifact. After expiry,
recover the exact published assets/recipes for local review; do not regenerate
archives and assume identical checksums. A packaging-only stable update needs an
explicit `pkgrel` bump and a separately reviewed recipe change.

### Publication credentials

The dedicated AUR SSH public key is registered to **ufna** and SSH authentication
was verified on **2026-10-08**. Its private key is configured as the GitHub
**release environment** secret `AUR_SSH_PRIVATE_KEY`. That environment permits
only the `main` branch. CI/candidate jobs receive no AUR key. Publication jobs use
`actions: read` and `contents: write`; no separate GitHub personal token is needed.
The owner starts publication manually; environment reviewers may be added later.

`packaging/aur/known_hosts` pins AUR's Ed25519 host key, independently verified
against the [official AUR homepage](https://aur.archlinux.org). SSH uses strict
host checking, batch mode and only the dedicated identity. Rotate a host key only
after verifying a newly announced official fingerprint. Rotate the client key by
registering its replacement, updating the environment secret and revoking the old
public key. No password, private key or account session is stored in the repository.

### Release maintenance

For each new bundle, increase the product version, deliberately run a fresh
candidate, review its exact source/hash/library provenance, then publish that run.
Verify public GitHub downloads and all three AUR packages, including a fresh yay
installation, user setup and a process-preserving upgrade. Monitor AUR comments
and rolling-library changes. macOS remains an explicit hosted opt-in.

An authenticated release listing can find GitHub drafts that the tag endpoint
returns as 404. The publisher checks all pages for the exact tag and rejects
ambiguous matches and API errors before creating or resuming a draft.

Signing with an owner-controlled identity and appointing a backup maintainer are
optional follow-ups. Checksums and exact run/commit provenance are provided;
package/tag signing is not configured.

## Validation record

The hosted runs below preceded the manual-only budget policy. They record prior
platform/package validation; they do not imply automatic checks are enabled.
The budget change itself was checked locally with actionlint, source checks and
all eight suite selections, including 104 success/failure/skip gate scenarios.
No hosted run was dispatched to validate it.

On **2026-10-08**, the complete
[main CI run](https://github.com/ufna/zerus/actions/runs/37705887611) passed every
job at commit `548469a`, including both CLI platforms, both desktop platforms,
Rust 1.85 and the installed Arch bundle check. Linux CLI validation covered 135
Rust unit tests, 202 terminal smoke checks and 398 Python cases across 33 modules;
native authenticated tests remain opt-in. Desktop coverage includes 30 Linux
and 32 macOS CTest suites. The corrected account fixture also passed ten
consecutive runs on each platform.

The artifact-only
[release candidate run](https://github.com/ufna/zerus/actions/runs/37705888076)
also passed its full CI and rebuilt the stable source and binary packages with
makepkg. Downloaded results are available locally under the ignored
`artifacts/review-candidate/` directory and in the `zerus-0.36.2-candidate`
Actions artifact (fourteen-day retention):

- Both pacman packages are approximately 5.4 MiB. All four archive/package hashes
  pass `sha256sum -c SHA256SUMS`.
- All three `.SRCINFO` files are present and exactly match `makepkg --printsrcinfo`.
- The binary manifest pins `548469a20972ec24594130efdff2d4d35ba0c768`; versions are
  GUI/product 0.36.2 and CLI 1.46.1. The package contains 59 dependency notices.
- The source archive contains 433 entries and excludes Beads, agent instructions,
  workflow files and design prototypes while retaining runtime artwork/notices.
- The downloaded binary package was actually installed in a disposable Arch
  container. System binaries, repeated `zerus-setup`, compatibility links and an
  isolated terminal session in an ordinary non-Git folder passed.
- namcap reported no errors. Warnings about dynamic dependencies/interpreters
  remain in the separate `release-packaging-548469a20972ec24594130efdff2d4d35ba0c768`
  log artifact and local `artifacts/review-release-logs/`.

Production packaging uses `BUILD_TESTING=OFF`. Migration helper tests cover
idempotence, conflict preservation, dangling links, missing targets and root
refusal. Removing `qt6-svg` makes the actual resource-render test fail; restoring
it passes. The final handoff/tracker documentation may follow the pinned candidate
commit; regenerate the exact candidate after changing release source or URLs.
Those preparation checks did not establish public AUR installation. The first
public installation and deployment validation is recorded below.

On **2026-10-08**, the publication changes were validated locally with actionlint,
source checks, generated makepkg metadata and credential-free contracts covering
provenance, archive safety, library floors, immutable draft/public retries, version
downgrades and GitHub/AUR ordering. No hosted workflow was dispatched.

The **0.37.0 local review candidate** pins commit `11dff8f` and is available under
the ignored `artifacts/review-candidate-0.37.0-11dff8f/` directory. A clean isolated
checkout built the real `zerus-git` production package, then assembled the exact
source/binary archives and rebuilt `zerus` and `zerus-ade-bin` with makepkg. Rust's
135 release unit tests passed; all three package layouts/executable versions and
59 dependency notices passed validation. namcap reported no errors; dynamic-tool
warnings remain in the logs. Both stable packages are about 5.4 MiB. All six sealed
release assets passed SHA-256 verification and all three `.SRCINFO` files exactly
match makepkg output. Library floors were checked against the real `.BUILDINFO`.
This local packaging review does not replace the full exact-commit release checks.
Regenerate the hosted candidate from the selected final `main` commit before
public publication; later documentation/tracker changes have a different SHA.

### First public release: 0.37.0

On **2026-10-08**, the owner explicitly requested the latest build, required
checks and deployment. The [release candidate run](https://github.com/ufna/zerus/actions/runs/37830899221)
pins `998bc4e9565a645e59b26bec3e2ced187e3d1444` and passed quick/minimum-Rust,
full Linux and Arch checks. macOS hosted jobs were skipped. Linux covered 145
Rust unit tests, 212 terminal smoke checks and 427 Python cases in 35 modules;
24 authenticated/native opt-in cases were skipped. All 32 Arch CTest suites
passed. VCS and both stable packages passed layout/version/notice checks and
namcap with no errors; known dynamic-tool/plugin warnings remain in the logs.

Independent candidate review verified GitHub's artifact digest, all six sealed
asset hashes, 466 source-export entries, binary embedded source provenance and
all three canonical makepkg metadata pairs. Binary floors record Qt base/SVG
6.12.0, WebEngine 6.11.2 and KDE Frameworks 6.30.0, plus the builder's C/C++/glibc
versions. Both stable packages include 59 Cargo dependency notices and the
vendored libvterm/md4c notices.

The [successful publication run](https://github.com/ufna/zerus/actions/runs/37833745339)
resumed the same candidate after correcting authenticated draft discovery. It
published [v0.37.0](https://github.com/ufna/zerus/releases/tag/v0.37.0), verified
all seven download assets anonymously and synchronized the three AUR recipes.
The release tag and archive hashes remain unchanged. AUR HTML pages became
visible before RPC metadata refreshed; yay lookup succeeded after that refresh.

A disposable, fully updated Arch environment installed **all three flavors through
yay**: binary/stable source `0.37.0-1` and VCS `0.37.0.r60.ga2fa919-1`. Installed
binary checks, idempotent user setup and ordinary non-Git terminal sessions passed.
An actual X11 desktop GUI, its isolated tmux server and terminal process survived
binary package reinstallation and subsequent source/VCS flavor replacement.
No user services were enabled by package installation or setup.

Existing Linux and Mac source installations were also updated. Linux binaries
were built from the reviewed source against its installed Qt 6.11 libraries. Mac native validation covered 143 Rust tests and 34 Qt suites; the
terminal suite was rerun after correcting a test assumption about rounded native
font metrics. Runtime code was unchanged by that correction. The Mac bundle was
verified with its existing ad-hoc signing procedure. Both managed GUIs activated
Sessions after claiming their sockets; existing persistent process identities and
state access were preserved. These local Mac checks used no GitHub macOS runner.
Authenticated model-turn tests remain opt-in and were not run for this release.
