# Genuine local Mac pair qualification through the rollout agent

Status: proposed pre-code agreement for G1, webui and the coordinator.
Scope: preparation slice `zerus-5wmh.66.3.25.2.1`, qualification child `.1.2`,
PR #819. This replaces the private-keychain fixture route. Its stopped evidence
remains a failed record, and its create/delete results do not qualify signing.
The proposed private-unlock amendment was never implemented or executed.

The coordinator selected the owner's existing Developer ID identity in the
owner's GUI Terminal rollout-agent session. The candidate must obtain full
successful signer proof before merging. No app, CLI or service is installed,
read from the owner's installation, registered or started by this test.
No keychain is created, imported, unlocked, configured or modified.

## Fixed external hook and source custody

The coordinator implements the dispatch in the external `rollout-lib.sh`, before
any ordinary rollout, install or keychain preparation. The repository supplies
only the fixed test `tests/run_macos_paired_genuine.py` and its controls.

The complete request is one line:

```text
<full-40-hex-commit><TAB>local-macos-pair-genuine<LF>
```

No commands, environment values, paths or additional arguments come from the
request. The hook's private configuration allowlists exactly one candidate SHA,
its repository, the integration base used for the source review, the fixed script
path and the published SHA-256 hashes of the script and reviewed source files.
For this pre-merge qualification only, the candidate may be the unmerged PR #819
head approved by both reviewers. This resolves the bootstrap dependency without
merging an unqualified signer. No other PR or commit inherits the exception.

Before queueing, refresh the candidate from `integrate/design-polish`, then pin
the integration base and candidate for both source reviews. The hook verifies
that base is an ancestor of the allowlisted candidate, the checkout is clean at
that exact candidate, the repository matches, and every configured source hash
matches. Later integration pushes cannot substitute another source into the
queued run; an updated candidate requires new approvals and a new allowlist.

The coordinator owns a separate clean checkout under
`~/zerus-dev/macbundle-genuine-checkout`. The fixed proof location is
`~/zerus-dev/macbundle-genuine-proof/<candidate-sha>/`, owned and mode 0700.
An existing or replaced output refuses; no previous result is overwritten.
A consumed-request marker is committed before dispatch. A crash or unknown
result remains consumed, and is never replayed. The allowlist entry is removed
right after the run, on either outcome.

The hook runs only this fixed invocation, with values derived from its own
allowlist and fixed location:

```text
/usr/bin/python3 -I -B tests/run_macos_paired_genuine.py --expected-sha <sha> --output-dir <fixed-proof-dir>
```

It takes the shared heavy-job lock with an exact owner receipt, waits a bounded
five minutes for another owner, and never removes a foreign lock. Its EXIT trap
releases its own lock first. No ordinary agent, tmux server, DeepSeek host,
owner GUI, LaunchAgent or service is stopped. The hook deadline is 600 seconds.

## Environments, existing identity and interaction

Orchestration, compiles, probes and verification tools use an environment built
from scratch with private HOME, XDG, TMPDIR, TMUX_TMPDIR and HGS directories
inside the proof directory. The script verifies its own GUI security-session
attributes through the public `SessionGetInfo` API before key access. No app is
launched and no display, bus, mobile connector or owner configuration is used.

There is one explicit HOME exception. The maintained signer subprocess
`tray/sign-local-macos.py`, and its security/codesign children, get the real
passwd HOME in the owner's GUI agent context. The coordinator authorized this
so the existing identity/search context works without any keychain change.
The child environment is otherwise fresh, uses isolated private directories,
and runs Python with `-I -B`. It receives only the existing optional
`HGS_MACOS_SIGN_IDENTITY` from the trusted agent configuration, or uses the
signer's ordinary automatic selection. The request cannot select an identity.
The genuine result must use Developer ID Application; a different signing
class fails without a fallback or retry.

The preparation API runs with private HOME. A test-only delegation at its
`signer.sign` call launches the unchanged maintained signer CLI with the exact
scratch app, scratch previous app and `--paired`. That is the only subprocess
with the real-HOME exception. The delegation returns the sealed pair record
from the verified output; it does not model a signature, identity, team,
requirement or verifier. All other production preparation paths run unchanged.
The test restores the delegation and checks source and module identity in
finally. Both production scripts remain byte-identical to `88cf4efa`.

The existing key must already be usable without interaction in that agent
session. No password is requested, read or supplied. A key-access, locked-key,
interaction or partition refusal stops the run. No unlock, access change,
partition command or alternate identity is attempted. The coordinator's owner
window must observe and attest whether a keychain prompt appeared; any prompt
is aborted without responding and makes the run fail. Missing or unknown
prompt observation cannot qualify a result. A process-local no-UI flag is not
claimed to propagate to a separate codesign process.

## Genuine scratch campaign

All scratch inputs and outputs are inside the fresh proof prefix. The first
previous-app input is a nonexistent scratch path; the second is the first
successfully prepared scratch pair. No installed app or CLI is inspected.

1. Record the exact commit, source hashes, clean checkout and GUI-session
   evidence. Create and bind the owned scratch root's device/inode. Record a
   bounded private-HOME inventory before scratch creation.
2. Compile four small, different Mach-O images: one app image and one CLI for
   each of two generations. These are synthetic executable inputs to the real
   preparation primitive, not a full application or a claim about a CLI source
   commit. Record every raw input hash and mode. Never execute these images.
3. Prepare generation one through the maintained preparation API and delegated
   signer. Record genuine Apple-anchor strict all-architecture verification,
   exact app/CLI identifiers, Developer ID class, non-ad-hoc signatures, same
   nonempty team, different CDHashes and stable designated requirements.
4. Prepare generation two against the first scratch pair. Prove actual mutual
   designated-requirement continuity independently for app and CLI. Recheck the
   whole app seal and both sealed CLI hashes against raw/signed bytes. Both
   input apps and CLIs must remain byte-identical. Keep all raw and output file
   hashes, pair records and actual verification results in private evidence.
5. Alter only the second scratch CLI and require the actual strict deep
   all-architecture verification to refuse it. A successful verification or a
   different unclassified failure fails the campaign. No verification boundary
   is patched or modelled anywhere in this test.
6. Restore module attributes and prove source hashes are unchanged. Remove the
   exact owned scratch root after its processes are reaped. Prove both signed
   bundles, raw images, sources and temporary files are absent. Record the
   private-HOME inventory after removal. Bounded inventory refusal or unexplained
   residue records an unknown outcome, never a clean cleanup result.

Inventories refuse links, special files, changing inputs and exceeded bounds:
128 entries, 1 MiB per file and 4 MiB total. Keep only relative names, types,
modes and hashes in the inventory. Never remove unconfirmed residue or repair
owner state. Bound each child command at 60 seconds, each signer subprocess at
180 seconds and the script at 480 seconds, reserving 60 seconds for cleanup.
A timeout supervisor may terminate only a newly created, recorded test process
group and must reap it before cleanup. Unknown process ownership or cleanup
stays failed and retained for coordinator review. No owner agent is terminated.

## Results and admission

The fixed proof directory retains `raw.log`, `result.json` and `SHA256SUMS`.
The result records exact source/base hashes, tool/case counts, both generations,
continuity and tamper outcomes, source restoration, exact cleanup and unknowns.
The external hook adds its one-run receipt, owner-window/prompt observation,
lock release and exit status. The coordinator supplies that envelope with the
hashed test artifacts; SSH is used only to read this fixed private result.
Raw identity names, team IDs and private paths remain in private evidence.
PR text and Beads contain counts and hashes only.

Any failure ends this queued run. There is no automatic retry or alternate
identity. The coordinator resolves a failed or unknown outcome before proposing
another action. The private-keychain fixture helpers are removed from the
candidate implementation; their failed raw records and historical source pins
remain available for audit.

Both reviewers must agree this design and hook before implementation. After
implementation, both review the exact source pin and published source hashes.
Only then does the coordinator implement/verify the external hook, arrange the
owner window and queue one run. Both inspect the exact final native evidence
before PR #819 may merge into `integrate/design-polish`. Main remains owned by
the snapshot stream.

A successful result qualifies genuine signatures and preparation on these
scratch images. The maintained installer, a real paired application build and
running-self `.25.1.2` remain separate; currentness, role, lifetime and native
effect gates are not granted by this signer test.
