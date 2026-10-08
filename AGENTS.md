# Agent instructions

Zerus provides a desktop UI and the `hgs` CLI for persistent local and remote
agent sessions. The CLI lives in `src/`; the Qt GUI lives in `tray/src/`.
`design/` contains prototypes and original artwork, not runtime dependencies.

The repository language is English. Write documentation and other project-facing
text in English; conversations may use the user's preferred language.

## Work tracking and memory

Use the `beads` skill in `.agents/skills/beads/SKILL.md` and run `bd prime` at
session start and after compaction. Use `bd` for all durable tasks and blockers.
Use `bd remember --key <key> "<decision>"` for concise, shareable project memory.
Do not create separate TODO or MEMORY files. Read current source before carrying
forward a historical limitation.

The source of truth is the Dolt database inside `.beads/` (currently
`.beads/embeddeddolt/` with the embedded backend). Synchronize with `bd dolt push/pull` over
`refs/dolt/data`; `.beads/issues.jsonl` is a passive export, not a sync protocol.
New clones should run `bd init --non-interactive --prefix zerus`, then `bd prime`.
Never replace an existing database with a JSONL import during normal operation.

If the optional shared-memory helper is installed, recall the project using
`python3 "$HOME/.local/lib/hgdev-memory/memory.py" recall --client codex`.
Use only the returned project wing. Store short verified decisions there; never
upload credentials, private personal data, transcripts or full prompts.
Repository documentation owns specifications; Beads owns tasks and blockers.

## Compatibility and operation

Keep the `hgs` CLI, `HGS_*` variables, state/config paths and existing application
and service identifiers compatible. A repository rename must not reset accounts,
project catalogs, native agent processes or saved conversations.

Never restart or kill native agents, tmux servers or DeepSeek hosts as part of a
GUI update. Test tmux operations with an existing isolated socket directory.
Do not hot-reload a DeepSeek bridge that owns live agents: its disposal closes
them. Use the native API compatibility path for resident adapters and keep
unconfirmed permission setup blocked until an explicit resume verifies it.
Ordinary non-Git folders are supported; do not create Git repositories or worktrees
automatically. Destructive UI actions require explicit confirmation.

Use non-interactive filesystem commands and `ssh/scp -o BatchMode=yes` in automation.
Use synthetic fixture paths, reserved example domains and local/private configuration
for deployment values. Never commit login material, runtime data or personal audits.
All Beads data on the repository remote may eventually be public, including history.
Use a GitHub no-reply address for Git and Beads attribution if email privacy matters.

## Validation and handoff

Hosted CI is manual-only, including pull requests and pushes. Do not dispatch
GitHub Actions without an explicit owner request to run hosted checks. Validate
workflow edits locally. Default CI uses one Ubuntu job; full Linux integration,
Arch packaging and macOS are explicit opt-ins. Release candidates request
Linux/Arch coverage but leave macOS off by default. See `docs/ci-and-aur.md`.
Dependency proposals are monthly and grouped, with one ordinary PR per ecosystem.
Cargo proposals update only the lockfile within reviewed manifest constraints;
review manifest migrations explicitly and retain Rust 1.85 compatibility.
Keep security alerts and security updates enabled separately. Never auto-merge.
AUR packages are `zerus` (stable source), `zerus-git` (upstream main) and
`zerus-ade-bin` (stable binary), with x86_64 support. Release publication is a
separate owner-triggered workflow consuming a verified exact-commit candidate;
never rebuild or overwrite immutable assets on retries. Pure VCS version bumps
do not create AUR commits. Keep publication credentials confined to the release
environment and validate changes locally before requesting its manual launch.

Run checks appropriate to the change; see `CONTRIBUTING.md`. Update Beads status
and export with `bd export -o .beads/issues.jsonl` before staging tracker changes.
Inspect `git status` and the staged diff. Commit, push and synchronize only when
explicitly authorized; the default workflow is conservative.
Mirror substantive instruction changes in `AGENTS.md` and `CLAUDE.md`.
