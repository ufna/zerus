# Contributing

Build requirements are documented in `docs/installation.md`. Keep fixtures synthetic and
machine connection settings local. `AGENTS.md` describes the Beads workflow.

## Local checks

```sh
python3 scripts/ci/check-source.py
python3 -m unittest discover -s tests -p 'test_*publication.py'
bash scripts/ci/cli.sh
bash scripts/ci/gui.sh
```

The CLI script clears inherited `HGS_*` session/account context, runs Rust unit
tests and terminal smoke tests, then runs the Python integration test classes in
parallel fresh processes and builds a release binary. Classes larger than twelve
tests run as several jobs unless they share a class fixture, slowest first. The
desktop script builds with `BUILD_TESTING=ON` and runs the complete Qt suite
headlessly in parallel; `test_sessionswindow` runs as ten shards of its test
functions. `ZERUS_TEST_JOBS` sets both test parallelisms (defaults: at most 16
Python and 8 Qt jobs, never more than the CPU count; `1` runs them one after
another). Test logs and Python job durations live in ignored
`artifacts/test-results/`. Set `ZERUS_BUILD_JOBS` to limit desktop build
parallelism. Run `cargo +1.85.0 test --locked` for the minimum supported Rust.

On Arch, `bash scripts/ci/arch.sh` also builds and validates the pacman package
as an unprivileged user. It substitutes a local Git source only in its disposable
build directory; it never installs the package or enables services.
Hosted CI runs full Linux CLI, Arch desktop/package and macOS checks automatically
on pushes to main and PRs targeting main. Manual runs default to full coverage;
explicit suite inputs can narrow a manual/reusable run. Validate workflow edits
locally before pushing. Daily nightlies publish a verified binary to GitHub and
AUR only after every check passes; stable releases remain manually published.
See [CI and Arch publication](docs/ci-and-aur.md) for triggers and publication
requirements.

Native integration tests may need separately installed agent CLIs or a real desktop;
read each suite's setup before running it. Do not run tests against live agent state.
Some platform tests only exist on macOS or Linux. Record any unavailable checks.

On a KDE Plasma 6 Wayland desktop, the optional native window-layer check uses
its own temporary settings and synthetic windows, verifies KWin's actual state,
and leaves unrelated windows and desktop rules alone:

```sh
QT_QPA_PLATFORM=wayland ZERUS_TEST_KWIN=1 \
  tray/build/tests/test_kwinwindowlayer nativeKWinChangesOnlyOurWindowAndRestores
QT_QPA_PLATFORM=wayland \
  tray/build/tests/test_sessionswindow windowLayerControlsKeepFocusAndLayout
```

The ordinary `kwinwindowlayer` test runs on a private D-Bus session with a fake
compositor and never controls the user's desktop.

For the mobile relay and connector, install `services/mobile` in a Python 3.11+
virtual environment and run `python -m unittest discover -s services/mobile/tests -v`.
These tests use synthetic subprocesses and isolated databases, without live native
agents. For Android, run `./gradlew testDebugUnitTest lintDebug assembleDebug` from
`mobile/android` with JDK 17 or 21 and Android SDK 36. Follow
[mobile setup](docs/mobile-deployment.md) for device and notification checks.

Beads issues, history and memories must contain only information suitable for public
source collaboration. An ignored local database still becomes remote data when
`bd dolt push` is run. Review the exported issues and memory before synchronizing.

For release preparation, run Gitleaks on both the working tree and all Git history,
and review binary assets and dependency notices. The local privacy script catches
common accidental machine paths and keys; it is not a complete secret scanner.
