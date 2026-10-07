# Contributing

Build requirements are documented in `README.md`. Keep fixtures synthetic and
machine connection settings local. `AGENTS.md` describes the Beads workflow.

## Local checks

```sh
cargo test --locked
cargo build --locked
HGS_TEST_BIN="$PWD/target/debug/hgs" bash tests/test_hgs.sh
python3 -m unittest discover -s tests -p 'test_mac_install.py' -v
cmake -S tray -B tray/build -DBUILD_TESTING=ON
cmake --build tray/build --parallel 4
QT_QPA_PLATFORM=offscreen ctest --test-dir tray/build --output-on-failure
python3 scripts/check-source-privacy.py
```

Native integration tests may need separately installed agent CLIs or a real desktop;
read each suite's setup before running it. Do not run tests against live agent state.
Some platform tests only exist on macOS or Linux. Record any unavailable checks.

Beads issues, history and memories must contain only information suitable for public
source collaboration. An ignored local database still becomes remote data when
`bd dolt push` is run. Review the exported issues and memory before synchronizing.

For release preparation, run Gitleaks on both the working tree and all Git history,
and review binary assets and dependency notices. The local privacy script catches
common accidental machine paths and keys; it is not a complete secret scanner.
