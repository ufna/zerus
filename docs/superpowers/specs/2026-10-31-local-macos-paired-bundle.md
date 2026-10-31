# Local macOS paired bundle

Status: amended proposal; re-pinned desktop and webui agreement required before code.
Engineering task: `zerus-5wmh.66.3.25.2`, under G1 (`zerus-5wmh.66.3.25`).

## Problem and boundary

The accepted passive Mac installation selector (PR #803, temporary-path test
correction #806) derives both expected identities from its own verified, sealed,
genuinely signed app containing `Contents/MacOS/hgs-tray` and
`Contents/MacOS/hgs`. It rejects an external CLI, ad hoc signatures, different
teams and changed generations. Source installs currently produce a single-code
`hgs-tray.app`, reject a nested Mach-O CLI in `sign-local-macos.py`, and pass
the separate user CLI to the GUI. Therefore they cannot supply that origin.

This proposal supplies the installation artifact. It does not activate the
optional native-peer feature, create a native caller, admit a desktop writer,
start a desktop host, or establish current peer/role/configuration/challenge
authority. Running-self positive qualification remains the separately reviewed
`zerus-5wmh.66.3.25.1.2`. Desktop owns that gate and all native admission.

## Result and compatibility

For the Tauri choice, the maintained local installer prepares this pair:

| Location | Purpose |
| --- | --- |
| `hgs-tray.app/Contents/MacOS/hgs-tray` | Existing switch GUI executable |
| `hgs-tray.app/Contents/MacOS/hgs` | Private regular copy of the matching built CLI, signed before the app |
| User CLI prefix `bin/hgs` | Existing independent PATH installation for agents and shells |

The app remains `com.hgdev.hgs-tray`; raw switch builds remain
`com.hgdev.hgs-tray.local`. Keep all CLI commands, `HGS_*` variables, state and
configuration locations, service IDs, LaunchAgent label and Interactive process
type. Do not replace the PATH binary with a link into a swappable bundle.
Existing agents and services continue using the ordinary CLI installation.
Qt installation and public hardened/notarized packaging retain their policies.

The CLI's Unix `executable_path()` and `runner_executable()` honor
`HGS_EXECUTABLE`; otherwise they retain the invoked executable in durable
recipes, pane environments, supervisors and registration previews. Merely
switching the GUI to the bundled CLI would therefore record a path inside the
app that the updater swaps. Every production child invocation through the
bundled CLI must explicitly carry the installer-verified stable PATH CLI in
that child's `HGS_EXECUTABLE`. This applies to launch, fork, resume, watch,
detached helpers and ordinary service registration, including indirect calls.
Do not set a process-global discovery variable to achieve it. Slice 3 must
inventory every invocation path and prove that a session launched, forked or
resumed through the paired GUI never records an app-internal executable in its
recipe, pane environment or runner. Missing or unverified stable selection
refuses the operation rather than using the app path as a durable fallback.
Existing explicit CLI selections retain their ordinary behavior. Any future
native service registration using a paired executable is a separate G1-owned
decision; this installer grants no such registration or startup authority.

The source CLI comes from the matching root Cargo build in the same checkout,
not arbitrary PATH discovery. The installer builds the CLI with Cargo's locked
dependencies and retains its artifact hash with the GUI build/source identity.
The CLI currently reports a package version only; do not invent an embedded
CLI source-commit guarantee. Clean preview/Dev qualification pins the checkout
before and after both builds, the GUI's embedded commit and both artifact
hashes. Existing exact-main `ZERUS_DEV_BUILD_NUMBER` and branch
`ZERUS_LOCAL_PREVIEW_LABEL` rules still apply; the pair does not invent a channel
or alter version strings. Build both binaries before preparing their private
staged copy. No signing operation modifies the build-tree CLI or PATH copy.

Before any app replacement or LaunchAgent write, the paired installer requires
the stable PATH CLI to be a regular independent executable whose bytes have
the same SHA-256 as the build-tree artifact it will copy and sign. A mismatch
refuses with one actionable line: run `./install.sh` (or `mac-install.sh`) from
this checkout first. The remote maintained installer already runs root
`install.sh` first. This prevents a GUI-only update from silently leaving
agents, hooks and services on a different CLI. A later independent CLI update
or bundle rollback may still create skew; it must be observable as described
below and must not silently change durable executable paths.

## Preparation and signing

Add a headless preparation entry point shared by `tray/install.sh` and scratch
qualification. Inputs are the built app, built CLI, previous installed app
(read only), and a fresh output inside a caller-owned scratch prefix. Refuse
overlapping input/output or an existing output, symlinked code, special files,
unexpected nested code, and an invalid app executable/layout before signing.
The output keeps the `hgs-tray.app` basename. Failed preparation removes only
its owned output and never publishes a candidate or changes an installed app.

Extend `tray/sign-local-macos.py` with an explicit paired mode. Its existing
single-executable mode remains the default for Qt and old direct callers.
Paired mode requires exactly the two named regular Mach-O executables and
continues refusing all other nested code and all links. It uses one existing
compatible identity selected by the current installed-app continuity flow and
optional `HGS_MACOS_SIGN_IDENTITY`; there is no ad hoc fallback.

Sign the CLI explicitly first, retaining the `hgs` signing identifier used by
public paired bundles; then sign the enclosing app. Use the current local
non-hardened, no-network-timestamp options. Do not deep-sign discovered code.
Verify each code object strictly and verify the complete app with nested strict
checks. Require genuine Apple certificate signatures, nonempty derived team
identifiers equal in both objects, stable designated requirements, and the
expected executable identifiers. Never embed a team ID or accept a requirement
from peer input. Preserve mutual app designated-requirement compatibility with
the installed app. If the previous app contains a signed paired CLI, check its
CLI continuity too; a previous unpaired app needs only existing app continuity.

Record the CLI's build-tree SHA-256 before signing and its signed SHA-256 after
signing, in `Contents/Resources/zerus-local-pair.json`, before signing the app.
The outer signature seals that descriptive pair record. The build-tree hash
identifies the untouched input artifact, including any toolchain ad hoc
signature; it does not assert that the input had no signature. Validate the
record against the staged leaf before publishing the prepared output. No
record or hash replaces the runtime selector's independent signature checks.

Apple's [TN2206](https://developer.apple.com/library/archive/technotes/tn2206/_index.html)
describes signing nested code before its enclosing bundle and verifying with
`--deep --strict`. Existing app continuity follows
[TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).
These packaging checks are preparation evidence; the passive runtime selector
still independently derives and checks its own pair and mapped process.

The signer never imports a key, unlocks a keychain, changes key ACLs, resets TCC,
or uses public signing credentials. A locked key or signing prompt stops the
headless campaign. Any interactive key-access step needs a coordinator-arranged
owner window. An SSH login and a private HOME do not prove keychain isolation.

## CLI selection

For a maintained paired Tauri install, omit the generated LaunchAgent's `--hgs`
pair and let the GUI prefer its own sibling bundled CLI before PATH discovery.
Keep the label, executable path, Interactive type, logging and lifecycle intact.
The existing Qt default already selects its bundled CLI when present and then
falls back to the user's CLI install directories even with launchd's bare PATH
(`tray/src/main.cpp`, `AppConfig.cpp`). Thus rollback to an old unpaired Qt app
still works with that registration, without pointing to a missing bundled CLI.
An older unpaired Tauri rollback likewise uses `core::default_hgs()`'s PATH,
user CLI and Homebrew fallback chain.
Keep the current explicit external argument for ordinary Qt installs.
Preserve user-authored explicit `--hgs`
and `ZERUS_CLIENT_HGS` selection for existing CLI behavior and fixtures.
An explicit external override never establishes native authority: that future
authority must come from the independent verified own-bundle selector and exact
direct peer, without substituting an executable path for trust.

The no-`--hgs` registration is allowed only if the resolved stable `HGS_BIN`
equals the first existing candidate in the shared rollback fallback chain:
the managed registration's verified launchd PATH, then the user's `.local/bin`,
`/opt/homebrew/bin` and `/usr/local/bin`. Do not use the installer's interactive
PATH as launchd evidence. An unknown launchd PATH or a selected CLI outside
that chain refuses paired installation before any app replacement or plist
write. This protects both Qt and unpaired Tauri rollback from silently opening
with an empty fleet or selecting a different CLI.

Keep ordinary unbundled/Linux/Windows discovery unchanged. Do not make core
library users such as bare `hgs` infer native trust from a sibling file. Any
shared discovery helper takes a descriptive path only, returns no authority,
and needs independent native/security review before wiring. Current service
registration is unchanged; launching a paired service for native admission is
desktop's later integration work, not an installer side effect.

Keep sibling preference inside the GUI process's selected-path state. Do not
export `ZERUS_CLIENT_HGS` for that implicit choice: terminals and other GUI
children inherit environment variables. Explicit user-authored overrides keep
their current precedence and meaning.

## Install, update and rollback

`mac-install.sh` remains a maintained remote entry point. The client branch of
`tray/install.sh` builds/prepares the pair before any application replacement,
LaunchAgent write or live update. Keep source transfer and clean-checkout Dev
restrictions, and forward existing identity/version inputs as literal values.
The CLI remains installed by root `install.sh`. Mobile connector updates are
outside this task and remain explicitly disabled during qualification.

The existing `update-installed.py --check`, draft save, native-process retention,
single rollback slot, switch guards and numbered Dev guard remain mandatory.
The sealed app and CLI move together through the existing bundle swap. Do not
copy either executable into an installed bundle after signing. A rollback
restores that bundle's CLI along with its GUI. An older unpaired Qt rollback
continues to find the independent PATH CLI through its existing fallback; the
paired registration contains no CLI path that becomes invalid. Review this
exact direction before wiring.

Slice 3 also extends the existing build provenance with the bundled CLI's
build-tree and signed SHA-256 beside the GUI source identity, and exposes the
same descriptive pair through `--build-info`. Coordination can then compare
the running build and `build-provenance.json` before replacing it. The updater
summary names which CLI the new or restored app will run on both swap and
rollback, including fallback for an older unpaired app. Missing pair metadata
on an old app is reported as unavailable, never fabricated.

Settings > About must identify the CLI the app runs: bundled or an explicit
path. If the stable PATH CLI used by sessions differs, show both selections
and versions rather than suggesting they are one CLI. Exact copy and placement
need webui review against the existing About screen in slice 3, with catalog
strings and Russian translations and no dot separators. This is an acceptance
criterion, not an approved UI implementation in the design-only slice.

Scratch preparation is a separate stage, not `HGS_TRAY_RESTART=0`: that flag can
still replace an installation and touch registration. The scratch entry point
never calls launchctl, LaunchServices, the updater's live path, TCC, a GUI, or
any agent/service installer. It supports a private output prefix without
consulting the owner's installed app unless explicitly supplied read only.
No shared GUI, services, native agents, tmux server or mobile paths are touched
by this stream. Only the coordinator rolls out an owner installation.

## Reviewed slices and evidence

1. Agree and merge this design only. Pin desktop's G1 boundary agreement and
   webui's installer/rollback UX agreement using scoped `hgs wait` replies.
2. Add the explicit paired signer and headless staging primitive, with synthetic
   Linux controls and opt-in native Mac tests. Leave live installers unchanged.
   Pin security/native review and independently qualify real signatures through
   the maintained signer, in a scratch prefix, only with a confirmed owner
   key-access window if needed.
3. Wire the Tauri local installer and bundled default selection, with precise
   build/source evidence and old Qt rollback compatibility. Pin installer,
   native/security and UI review, then qualify private updater/LaunchAgent
   fixtures and a built-app smoke in an isolated display/bus context where
   applicable. No Mac interactive app launch is part of headless qualification.

Each slice gets raw logs, exact executed counts (skips identified), a SHA-256
manifest, a pinned review and a merge checkpoint/report. Model tests prove only
their controls; a skipped native suite never qualifies signing or running-self.
Required negative controls include missing/non-Mach-O CLI, third nested code,
links, wrong or empty team, ad hoc/unsupported certificate, signing failure,
app/CLI continuity failure, source mismatch, and tampering after sealing.
Slice 3 additionally requires wrong PATH/build hash refusal before mutation,
unknown or out-of-chain fallback refusal, paired-launch/fork/resume stable
durable-path controls, paired-to-unpaired Qt and Tauri rollback, About selection
and skew presentation, sealed pair metadata and updater provenance/summary
checks. Test the per-child environment across each production invocation path;
an indirect helper must not bypass the stable recipe invariant.
Native signer tests use two distinct synthetic app/CLI images and changed
rebuilds; require strict whole-pair validation and refusal after tampering.
The real running-self selector, current connections and resource lifetimes
remain desktop's separate gate even after those signer tests pass.

All launched test children receive an environment constructed from scratch,
private HOME/XDG/TMPDIR/TMUX_TMPDIR and private display/bus when required.
Signing receives only the existing maintained identity selector; never key
material. Retain failures and distinguish no-key/no-window blockers from test
passes. Request a Mac scratch resource slot and any necessary owner window
through the coordinator before native execution. No owner rollout is requested
by this design agreement.

## Headless mechanics qualification amendment

Proposed 2026-11-01 following the coordinator's scratch-signing ruling; obtain
fresh G1 and security agreement before implementing this test fixture.

The first headless campaign reached no signing operation: the fresh HOME had
no searchable signing identity. The coordinator verified that the real login
keychain is unavailable for SSH interaction and forbids unlocking it. Genuine
Apple-chain qualification therefore waits for the owner GUI session through
the maintained installer rollout, with `zerus-5wmh.66.3.25.1.2`. Do not read the
owner installation or retry its keychain from the headless campaign.

For the isolated preparation slice only, allow a separately labelled native
mechanics fixture with a throwaway self-signed code-signing identity. Keep
production `genuine_signature`, genuine-team checks and the runtime selector
unchanged. The fixture must first demonstrate that the production verifier
rejects its real self-signed signature. Its later mechanics exercise explicitly
models the Apple/team identity boundary in test code while retaining real
codesign operations, a requirement pinned to its own certificate, fixed CLI
identity, app/CLI continuity, resource sealing, hashes and tamper refusal.
This is not genuine Apple evidence or a successful production preparation.
The actual genuine test remains a separate opt-in case and unqualified gate.

Create one keychain with a unique non-login name inside the private HOME using
public Security APIs and explicit references only. A random password exists
only in process memory: no password in subprocess argv, logs, environment or
files. Generate the synthetic key/certificate in memory and import them only
into that explicit keychain. Any private-key access setup applies solely to
this throwaway key. Never add a certificate to real trust settings, use default
keychain references, or change a real search list. Apple's
[StorageManager implementation](https://github.com/apple-oss-distributions/Security/blob/main/OSX/libsecurity_keychain/lib/StorageManager.cpp)
distinguishes private keychain creation from login/System search-list updates;
verify the chosen API path and actual private preference effects before
claiming isolation. Do not substitute the hosted-runner keychain shell script,
which passes passwords in argv and changes search lists.

Only test code supplies the synthetic identity SHA-1 through the existing
identity-selection input, scopes identity discovery to the fixture, and adds
`codesign --keychain` for the private keychain. Do not add a production switch
that bypasses Apple verification, spoof a real team, or reinterpret a private
test certificate as trusted installation policy. Keep private raw evidence
separate from public counts/hashes. Mark every native result as mechanics with
a modelled identity boundary; a green fixture cannot close the genuine gate.

Require a fresh pinned fixture review before execution, fresh child
environments and the shared Mac lock. Refuse any UI/key interaction and report
it. Record the public certificate, private keychain and scratch artifact paths
and hashes privately; remove only those owned artifacts and prove cleanup.
The password and private key material never enter evidence. The final source
and mechanics evidence require fresh G1/webui acceptance before merging the
preparation slice. The installer slice, genuine identity qualification and all
native currentness/role/effect gates remain open.

The agreed fixture conditions also require these concrete guards:

- Private HOME is not proof of per-user keychain isolation. Capture the same
  UID's read-only `security list-keychains -d user|common|dynamic`,
  `security default-keychain -d user` and `security login-keychain` references
  before creation, after creation, after signing and after deletion. Compare
  responses without repairing state; a change stops before further signing.
  Record references with the real home prefix redacted to `~`. These metadata
  reads authorize no owner item, identity or certificate queries.
- Exclude `/login.keychain` anywhere in the whole absolute fixture path, and
  exclude the System path. The import access object permits only codesign and
  the fixture process. No allow-all ACL or password-bearing partition command.
- Run scoped production `find-identity -v -p codesigning` first. If it filters
  the self-signed identity, record that separate negative. Test-only discovery
  may then expose exactly the helper-proven private identity after an actual
  scoped unfiltered matching-identity check; fingerprint, CN or cardinality
  mismatches stop. No fabricated tool result or owner-keychain fallback.
- The next prepare keeps the real Apple/team verifier unchanged and must fail
  specifically on its actual Apple-anchor command. Only afterwards may a
  test-module validator replace that boundary with real strict all-architecture
  verification against the fixture leaf certificate and exact expected ID,
  non-ad-hoc signature, present CDHash and stable designated requirement.
- Supply a synthetic team marker only when native self-signed metadata has no
  team, and only after the actual own-certificate check. All other production
  mechanics and the final whole-app check run unchanged. The codesign wrapper
  only appends the private keychain argument; it asserts that the rest of argv
  is identical. Restore test patches in finally and prove both tray scripts
  remain byte-identical to the source-approved `88cf4efa`.
- Delete through the explicit private reference. Source expectations about
  private creation do not prove deletion isolation: compare actual list/default
  references and private preference files afterwards too. Scope drift stops
  signing, but still delete the known helper-created keychain when its exact
  device/inode matches, then record the references again and report failure.
  A replaced or unconfirmed path stays for review. Never use list/default/trust
  setters to repair state. Exclude `/Library/Keychains/` anywhere in fixture
  paths, use markers that cannot have an Apple team ID shape, and require an
  actual non-GUI remote security session. An interaction/partition refusal
  stops without a signing retry; this fixture adds no partition remedy.
