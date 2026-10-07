# Contributing

Build requirements are documented in `docs/installation.md`. Keep fixtures synthetic and
machine connection settings local. `AGENTS.md` describes the Beads workflow.

## Local checks

```sh
python3 scripts/ci/check-source.py
bash scripts/ci/cli.sh
bash scripts/ci/gui.sh
```

The CLI script clears inherited `HGS_*` session/account context, runs Rust unit
tests and terminal smoke tests, then runs each Python integration module in a
fresh process and builds a release binary. The desktop script builds with
`BUILD_TESTING=ON` and runs the complete Qt suite headlessly. Test logs live in
ignored `artifacts/test-results/`. Set `ZERUS_BUILD_JOBS` to limit desktop build
parallelism. Run `cargo +1.85.0 test --locked` for the minimum supported Rust.

On Arch, `bash scripts/ci/arch.sh` also builds and validates the pacman package
as an unprivileged user. It substitutes a local Git source only in its disposable
build directory; it never installs the package or enables services.
See [CI and Arch publication](docs/ci-and-aur.md) for the workflow matrix,
candidate release commands and publication requirements.

Native integration tests may need separately installed agent CLIs or a real desktop;
read each suite's setup before running it. Do not run tests against live agent state.
Some platform tests only exist on macOS or Linux. Record any unavailable checks.

Beads issues, history and memories must contain only information suitable for public
source collaboration. An ignored local database still becomes remote data when
`bd dolt push` is run. Review the exported issues and memory before synchronizing.

For release preparation, run Gitleaks on both the working tree and all Git history,
and review binary assets and dependency notices. The local privacy script catches
common accidental machine paths and keys; it is not a complete secret scanner.
