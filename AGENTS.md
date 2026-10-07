# Agent instructions

Zerus provides a desktop UI and the `hgs` CLI for persistent local and remote
agent sessions. The CLI lives in `src/`; the Qt GUI lives in `tray/src/`.
`design/` contains prototypes and original artwork, not runtime dependencies.

## Work tracking and memory

Use the `beads` skill in `.agents/skills/beads/SKILL.md` and run `bd prime` at
session start and after compaction. Use `bd` for all durable tasks and blockers.
Use `bd remember --key <key> "<decision>"` for concise, shareable project memory.
Do not create separate TODO or MEMORY files. Read current source before carrying
forward a historical limitation.

The source of truth is `.beads/dolt/`. Synchronize with `bd dolt push/pull` over
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
Ordinary non-Git folders are supported; do not create Git repositories or worktrees
automatically. Destructive UI actions require explicit confirmation.

Use non-interactive filesystem commands and `ssh/scp -o BatchMode=yes` in automation.
Use synthetic fixture paths, reserved example domains and local/private configuration
for deployment values. Never commit login material, runtime data or personal audits.
All Beads data on the repository remote may eventually be public, including history.
Use a GitHub no-reply address for Git and Beads attribution if email privacy matters.

## Validation and handoff

Run checks appropriate to the change; see `CONTRIBUTING.md`. Update Beads status
and export with `bd export -o .beads/issues.jsonl` before staging tracker changes.
Inspect `git status` and the staged diff. Commit, push and synchronize only when
explicitly authorized; the default workflow is conservative.
Mirror substantive instruction changes in `AGENTS.md` and `CLAUDE.md`.
