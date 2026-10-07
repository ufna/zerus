# CI and Arch Linux publication

This document describes the implemented checks, package layout and first-publication
handoff. Repository documentation is English. No AUR package has been published by
this preparation, and release automation defaults to private review artifacts.

## Continuous integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main` and manual
dispatches. The release workflow reuses the same checks.

| Job | Pull requests | Main / manual | Checks |
| --- | --- | --- | --- |
| Source checks | Yes | Yes | Privacy guard, Python/Bash syntax, version consistency, actionlint |
| Rust minimum | Yes | Yes | Locked unit tests on Rust 1.85.0 |
| Linux CLI | Yes | Yes | Stable Rust unit tests, terminal smoke tests, all Python integration modules, release build |
| macOS CLI | Yes | Yes | Same CLI checks on the macOS 15 ARM runner |
| Arch desktop and package | Yes | Yes | Qt desktop build, all CTest suites, production build without test targets, pacman package, namcap, staged and installed bundle checks |
| macOS desktop | No | Yes | Native Qt build and all CTest suites |
| CI gate | Yes | Yes | Requires every applicable job to succeed |

Use **CI gate** as the branch protection required check. Its explicit result
handling fails on errors, cancellations and unexpected skipped jobs. The macOS
desktop skip is allowed only for pull requests. Expensive desktop coverage on
`main` keeps native platform coverage without paying for a second desktop build
on every PR update.

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

All CI jobs have bounded timeouts. A newer CI run cancels an obsolete run on the
same ref. Workflows have read-only repository permissions, checkout credentials
are not persisted, official actions are pinned to commit SHAs and Dependabot
proposes weekly action/Cargo updates. Rust and C++ caches reduce repeated build
cost. Test artifacts expire after seven days; release candidates after fourteen.
Fork PRs receive no deployment credentials. There is no automatic AUR push, public
release publication or self-hosted runner attached to a development machine.

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

The proposed package family is:

| Name | Source | Purpose |
| --- | --- | --- |
| `zerus-ade` | Versioned source release with SHA-256 | Recommended stable source package |
| `zerus-ade-bin` | Versioned Arch x86_64 binary archive with SHA-256 | Convenient installation without Rust/C++ compilation |
| `zerus-ade-git` | Anonymous upstream Git, `pkgver()` derived from history | Optional development channel |

The names are recommendations awaiting the owner's choice. All three were absent
from the AUR RPC lookup on **2026-10-08**; recheck immediately before submission.
`zerus-bin` already belongs to an unrelated crates.io offline-mirror tool, so it
must not be reused. [Existing zerus-bin package](https://aur.archlinux.org/packages/zerus-bin).

Stable source and binary recipes are templates until candidate preparation inserts
the release version and exact archive hashes. The VCS recipe is ready for build
validation; its initial version is a seed and `pkgver()` updates it during build.
`SKIP` is used only for VCS sources. Candidate assembly regenerates the metadata
for all three recipes. Never hand-edit `.SRCINFO`.
[Rust package guidelines](https://wiki.archlinux.org/title/Rust_package_guidelines),
[VCS package guidelines](https://wiki.archlinux.org/title/VCS_package_guidelines),
[.SRCINFO](https://wiki.archlinux.org/title/.SRCINFO).

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
The runtime also declares curl for native account HTTP requests and procps-ng
for process inspection. Optional Node.js/npm enable agent installation from
Accounts; Git enables repository/worktree features. Arch's Node.js package does
not include npm, so both optional dependencies are listed.
The package does not bundle their distributions, native agent executables,
accounts, credentials or configuration. Package installation does not start or
stop services, edit user homes or replace the user's tmux configuration.

The product package version currently follows the desktop version: `VERSION`
and `tray/CMakeLists.txt` are **0.36.2**. The independently versioned compatible
CLI remains **1.46.1**. CI rejects a product/desktop version mismatch; packaging
records both versions and the source commit in `build-info.json`.
Increase the product/desktop version for each new public bundle, including a
CLI-only bundle change, so published archive URLs and hashes remain immutable.

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

The packaged GUI unit uses `KillMode=process`. A GUI restart must preserve native
agent processes, tmux servers and DeepSeek hosts. Package upgrades do not perform
a restart. Review the old unit's kill policy before restarting an existing
installation; never use a broad process kill or restart all native clients.

## Release candidate workflow

The repository was **private** when this preparation started. A private upstream
cannot support a normal anonymous AUR build. The owner must choose either public
upstream or an audited public source/release mirror, then update the recipe URLs
and release destinations together. Do not put GitHub tokens or personal SSH
credentials in a `PKGBUILD`. Until then, private Actions artifacts can be reviewed
and installed directly with pacman; they are not a public yay distribution.

The manual workflow defaults to artifact-only preparation:

```sh
gh workflow run release.yml --ref main -f version=0.36.2 -f create_draft=false
gh run list --workflow release.yml
# Substitute the selected run ID:
gh run download RUN_ID --name zerus-0.36.2-candidate --dir /tmp/zerus-candidate
```

It checks the requested version, reruns the complete CI, takes the tested Arch
package from that exact run and verifies its embedded source commit. It creates:

- `zerus-0.36.2-source.tar.gz` from the exact Git commit. Git export attributes
  omit tracker, agent instruction/skill, workflow and design prototype directories.
  Runtime artwork, vendored sources and their notices remain included.
- `zerus-0.36.2-arch-x86_64.tar.gz`, containing the system-installable `usr/` tree.
- Tested stable source and binary `.pkg.tar.zst` packages, `SHA256SUMS` and
  `release-info.json`.
- `aur/zerus-ade`, `aur/zerus-ade-bin` and `aur/zerus-ade-git`, each containing
  a concrete `PKGBUILD` and generated `.SRCINFO`.

The source and binary recipes are actually rebuilt against the local candidate
archives with makepkg as an unprivileged user. namcap errors and staged executable,
desktop entry or license checks stop the candidate. Public recipe URLs stay
unchanged; local fixture sources exist only in disposable copies.

Arch CI also installs the built package inside its disposable container and tests
system binaries, repeated per-user setup and a terminal-only session in a private
home/socket. This does not exercise a live desktop's tray or an authenticated
native agent. namcap warnings about dynamically invoked tools and interpreter
references are retained in the logs; runtime dependencies are declared explicitly.

`create_draft=true` additionally creates a **draft** GitHub release under the
`release` environment with narrowly scoped write permission. Configure that
environment's reviewer policy before enabling draft automation if the account
plan supports it. This workflow does not publish the draft or push AUR repositories.
The draft uploads source/binary archives, pacman packages, checksums and manifest.

Equivalent local steps after the checkout is committed and its Arch package built:

```sh
python3 scripts/prepare-release.py --version "$(cat VERSION)" \
  --package artifacts/zerus-ade-git-EXACT_VERSION-x86_64.pkg.tar.zst --output dist
bash scripts/check-release-packages.sh dist
cd dist
sha256sum -c SHA256SUMS
```

Run these inside an Arch build environment with the package dependencies. The
helper rejects packages from another source commit/version. Rebuild after changing
release source, even if the changes are documentation only. Candidate directories
and source archives are not committed to the application repository.

## Owner questionnaire

Answer the required rows first; the recommendations already guide the prepared
implementation. An unanswered recommendation is not authorization to publish.

| ID | Required decision | Recommendation / implications |
| --- | --- | --- |
| Q1 | May the application source become public? Public upstream or a separate public release mirror? | Public audited upstream is simplest. Review Git history **and** `refs/dolt/data` before changing visibility. A mirror needs its own reviewed source, assets and matching recipe URLs. |
| Q2 | Which package names should we publish? | `zerus-ade` and `zerus-ade-bin`; add `zerus-ade-git` only if maintaining a development channel is useful. `zerus-bin` is occupied. |
| Q3 | Which AUR account owns the packages, and who can co-maintain them? | Owner-controlled account with a designated backup maintainer. Register/login manually; do not share login credentials in Git or chat. |
| Q4 | Which public maintainer name/contact and Git email should appear in AUR? | Choose a public project contact or privacy-preserving attribution deliberately. AUR Git history is public. Replace the recipe Maintainer placeholder before the first push. |
| Q5 | Is 0.36.2 the first public bundle release version? | Keep the implemented desktop/product version and CLI 1.46.1, unless a deliberate release version bump is wanted. Product `VERSION` and CMake must match. |
| Q6 | Are standard GitHub-hosted runner usage and the main/PR matrix acceptable? What spending limit? | Use included quota first, inspect the first run's usage, keep macOS desktop main-only. Do not enable paid overages or attach personal development machines as runners by default. |
| Q7 | Should installs require explicit per-user setup and opt-in autostart? | Yes: `zerus-setup` once per user, GUI/workers enabled deliberately. This preserves existing source installs and active sessions. |
| Q8 | Is x86_64 the supported first Arch architecture? | Yes, it is the validated target. Add aarch64 only with a real build/test runner and dependencies verified. |

Optional policies can be decided independently:

| ID | Policy | Recommended initial answer |
| --- | --- | --- |
| Q9 | Who approves stable release publication? | Owner reviews the candidate, notes and anonymous download checks, then publishes. Set `release` environment reviewers where supported. |
| Q10 | Which main branch protection/review policy? | Require **CI gate**; decide independently whether reviews are mandatory and whether owner/admin bypass is allowed. |
| Q11 | Do we need signed release tags/pacman packages/archives now? Which existing signing identity? | Prefer an owner-controlled signing key for stable distribution if available; checksums alone do not establish publisher identity. Never invent or store a private signing key in the repository. |
| Q12 | Should AUR updates be automated later? | Begin with manual reviewed Git pushes. Automation requires a dedicated AUR SSH key, verified host key, narrowly scoped GitHub environment secret and explicit publication policy. No AUR secret is needed for current CI. |
| Q13 | Who maintains stable releases, dependency/image updates and Arch rebuilds? | Name a primary and backup maintainer; rebuild/test after relevant Qt/KDE/Arch ABI changes. Update package `pkgrel` for packaging-only rebuilds. |
| Q14 | Should authenticated native-agent/model tests be scheduled? | Keep them outside public PR CI. If needed, decide providers, spending cap, isolated accounts and trusted-branch-only access first. |
| Q15 | Is a separate CLI-only package needed? | Start with the requested complete ADE bundle. A separate CLI package is useful for headless hosts but requires split-package ownership/conflict design. |
| Q16 | When do we need macOS distributable bundles/notarization? | Separate work: Apple Developer identity, signing/notarization policy, Qt dependency distribution review and installer update strategy. Current macOS CI is build/test only. |
| Q17 | Do release archives need retention/mirroring beyond GitHub? | Keep published versioned assets immutable. A mirror must preserve exact hashes; do not replace an existing release archive in place. |
| Q18 | Are existing agent machines running tmux 3.7+ servers? If not, when can their owners coordinate migration? | Check the running server version. Keep live sessions intact; plan older-server migration after deliberate session saving rather than restarting servers during a package/GUI update. |

## Owner actions for first publication

1. Answer Q1–Q8 and choose the public maintainer attribution. No credentials need
   to be pasted into the questionnaire.
2. Inspect the candidate and source/public-history audit. The repository's privacy
   guard is only a first check: run a complete history secret scan, inspect Beads
   data/history and review artwork/dependency notices before changing visibility.
3. Make the selected source and release location anonymously accessible, or create
   the reviewed public mirror. Update URLs before creating the final candidate.
4. Check GitHub Actions quota/billing and the first run results. Configure main
   branch protection with **CI gate** and the chosen release environment policy.
   [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
5. Create/login to the AUR account, verify its contact details and add a dedicated
   SSH **public** key. Keep the private key outside the repository. Verify AUR's
   SSH host-key fingerprints from the official submission instructions before
   accepting a connection; do not disable host-key checking.
6. Dispatch the artifact-only release workflow for the selected version. Review
   `release-info.json`, source contents, notices, package logs and all three recipe
   directories. Check `sha256sum -c SHA256SUMS`. Decide which package flavors to
   submit and fill their Maintainer attribution.
7. Create a reviewed draft/tag for that exact commit, add release notes and
   publish only after approval. Test every recipe's source URL anonymously from
   a machine without GitHub authentication. Recheck its downloaded SHA-256.
8. Prepare one AUR Git repository per selected flavor, following the commands
   below. Inspect the staged diff and regenerate `.SRCINFO`. Push only the reviewed
   package recipe and metadata; never archives, application source or build logs.
9. On a disposable clean Arch desktop, test `yay -S zerus-ade-bin` or
   `yay -S zerus-ade`, `zerus-setup`, launching the ADE, local/remote sessions and
   optional service enablement. Check that an upgrade preserves existing native
   processes and state. Initial publication is not complete until this public
   install roundtrip succeeds.
10. Record the public release/AUR URLs and maintainer responsibility, then update
    the installation documentation to advertise the live packages. Monitor AUR
    comments/out-of-date notifications and keep stable hashes immutable.

Example submission, **after** account/key setup, public release availability and
the owner's package-name decision:

```sh
# Use a dedicated temporary publication workspace outside the application tree.
git clone ssh://aur@aur.archlinux.org/zerus-ade-bin.git /tmp/zerus-aur-bin
cp /tmp/zerus-candidate/aur/zerus-ade-bin/PKGBUILD /tmp/zerus-aur-bin/
cd /tmp/zerus-aur-bin
# Edit only the public Maintainer attribution, then generate authoritative metadata.
makepkg --printsrcinfo > .SRCINFO
makepkg --verifysource
namcap PKGBUILD
git add PKGBUILD .SRCINFO
git diff --cached --check
git diff --cached
git -c user.name='CHOSEN PUBLIC NAME' -c user.email='CHOSEN PUBLIC EMAIL' \
  commit -m 'Publish Zerus ADE binary package'
GIT_SSH_COMMAND='ssh -o BatchMode=yes' git push origin HEAD:master
```

Substitute the chosen source/development flavor in its own repository. If the
package has appeared since the namespace check, stop and use AUR ownership/contact
procedures; do not overwrite another maintainer's package. Review AUR's current
submission guidelines again on publication day.

## Validation record

On **2026-10-08**, the first complete
[main CI run](https://github.com/ufna/zerus/actions/runs/37704073894) passed every
job at commit `7cc3900`, including both CLI platforms, both desktop platforms,
Rust 1.85 and the installed Arch bundle check. Linux CLI validation covered 135
Rust unit tests, 202 terminal smoke checks and 398 Python cases across 33 modules;
native authenticated tests remain opt-in. Desktop coverage includes 30 Linux
and 32 macOS CTest suites. The corrected account fixture also passed ten
consecutive runs on each platform.

Production packaging with `BUILD_TESTING=OFF` and all three package flavors was
tested in a disposable Arch container. Migration helper tests cover idempotence,
conflict preservation, dangling links, missing targets and root refusal. Updated
dependency metadata and the final private release candidate are checked separately
after that initial green run. Public AUR installation, owner account/signing
choices and source visibility remain separate first-publication requirements.
