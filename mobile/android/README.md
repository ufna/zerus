# Zerus Android

Native Kotlin and Jetpack Compose companion for the Zerus gateway protocol in
[the mobile architecture](../../docs/mobile-architecture.md). Android 8.0 or later
is supported. The app works without Google services.

Pair an HTTPS gateway with a short-lived invitation created by its administrator.
You can pair several gateways and browse logical projects and sessions across
their machines. Projects use workspace-scoped stable IDs; empty projects stay
visible, and uncatalogued sessions remain accessible by their folder or local
project label. An
optional `zerus://pair?server=...&code=...` link prefills the pairing dialog and
requires the user to review and confirm it. New connections suggest `https://relay.zerus.dev`; self-hosted HTTPS gateways
remain editable. No credential is compiled into the app. “Try demo” is an explicitly labelled, read-only preview.

The top-bar plus creates a new session. Workspace pairing lives in Machines;
empty Sessions and Projects views link there. Machine labels use the same badge
in session cards and the creation dialog. “Name on this phone” saves an encrypted
private label for the exact workspace and machine UUID. Reset restores the latest
reported name, including after an offline restart; it changes no hostname, SSH
configuration, project membership, session or delivery identity.
Machine label colors use the desktop's seven-color palette and tonal styling.
The encrypted phone-local override is scoped to the same workspace and machine
UUID and works offline. Automatic reset restores a UUID-derived color that stays
stable when the label is renamed.

Machines shows each reported gateway with its direct peer computers visibly
indented beneath it. A peer's “Through” caption uses the gateway's private phone
label when available. These computers belong to the same workspace connection;
each keeps its own name, color, status and controls, and session and project
filters still select individual computers. Each machine's action menu opens
Rename machine and Label color. Cached routes show “Last known” until
the catalog refreshes. The tree describes the selected one-hop route, not a
permanent master computer.

The retired official relay origin is a narrow transport alias for
`https://relay.zerus.dev`. Existing encrypted connection URLs remain stored as
original identity; requests, receipts, push setup and displayed endpoints use the
canonical transport. Other origins, paths and nondefault ports do not migrate.
Drafts, encrypted history and pending request UUIDs keep their original keys.

Messages and answers carry the exact computer, native session, run, conversation,
and request identity. Questions also carry their native content hash. An
interrupted delivery stays saved as uncertain. Receipt checks read the original
request; they never send a replacement automatically. Archive browsing uses an
explicit immutable archive UUID and verifies the returned identity. Archived
conversations have no message, answer, or interrupt composer. Scoped restore,
rename, fork, and forget require fresh native capability and action evidence;
destructive actions require a confirmation naming their target.

The message composer clears after the text and immutable attachment snapshots
are atomically saved in the private outgoing queue. It stays editable while the
previous message is sending. Each bubble shows Sending, Sent, Not sent, or
Delivery unknown; unknown delivery blocks another send to the same conversation
until an explicit receipt check or review. Sent means the computer accepted the
input, not that the model read or answered it. New text is never cleared by an
older receipt. Native user delivery rows reconcile bubbles by exact request UUID.
Without such a row, reconciliation requires an unambiguous exact text/time match
after a successful native receipt.

A message waiting in the agent's native queue appears as one compact expandable
card. Expand it to read and select the full text. Send now remains a manual
action, available only when the current agent and queued message allow it.

Attach files through the system document picker. The app copies and encrypts
selected bytes privately rather than sending document URIs or trusting mutable
files. The supported gateway limits are eight files, 10 MiB per file, 20 MiB total,
and 64 KiB of message text. The picker remains bound to its original composition
if navigation or typing changes; interrupted selection can be canceled in Saved
drafts. Files remain recoverable with unsent and uncertain messages.

An empty conversation immediately shows “Loading conversation…” while its
messages load. Background loading status and spinners appear only after five
seconds; manual refresh shows progress immediately and reuses any matching
in-flight read. Navigation cancels
ownership of earlier reads so stale results cannot replace a new conversation.
Matching history reads continue in a bounded shared queue and are reused on
reopening. Exact-target encrypted history is cached for seven days, up to 20
conversations and 20 MiB. Cached messages appear immediately when already
prepared in memory; cached history permits no action until a fresh inspection
verifies the conversation. History cache failures do not prevent live reads.

Draft edits update the editor immediately and save through an ordered background
writer with a 200 ms debounce. Navigation and backgrounding request a flush.
Edits awaiting a successful write are still in memory and can be lost on an
abrupt process exit. Accepted outgoing and attachment transactions finish before
transport or file cleanup, even if the screen is disposed; failed outgoing persistence sends
nothing. Successful outgoing records remain durable but are omitted from Saved
drafts.

Gateway tokens, drafts, outgoing messages, attachments, and push endpoints are encrypted with an Android Keystore
AES-GCM key. Android backup is disabled. Drafts remain available after disconnect
or session disappearance. Disconnect revokes this phone's gateway credential
before removing it locally; an already-revoked credential can also be removed.
An unavailable gateway leaves the connection intact.

## Sessions and projects

Sessions is the initial view. Its compact filters use the desktop glyphs and
native semantics: All sessions, Needs attention, Working, Saved sessions, and
Archive. Working requires a reachable computer and a current running agent;
input, approval, non-recovering errors, and blocked recovery require attention.
Optional native questions and explicit child requests also count as attention.
Routine child failures and legacy hook-profile observations do not. Saved
sessions are paused or stopped; Archive contains immutable archived histories.

The computer filter supports several machines across workspaces. Counts and
cards use the same computer, project, and search scope. Results form collapsible
project groups; empty groups are hidden in Sessions. Filter, query, collapsed
groups, and list position survive opening a conversation and background polling.
Provider badges match the desktop identity motifs.

Projects shows the canonical logical catalog, including empty projects, and its
saved folders grouped by machine. Overview cards show each folder's name and
selectable full wrapping path under one machine header, limited to two folders
on each of the first two machines with counts for remaining folders and machines.
Details show every folder using the same rows. Ordinary folders and local aliases
remain visible in Sessions.
Opening a project shows its folder details; View sessions opens a scoped Sessions
view. Back returns to project details and preserves the normal Sessions query
and filter. Archives are assigned by their UUID, so an archived and live session
with the same name can belong to different projects.

## Conversation history and native controls

The conversation keeps a bounded hot window rather than imposing a total chat
length limit. Nodes advertising canonical history support can fetch older and
newer pages and restore an opaque saved reading anchor. Native page order and
exact aliases distinguish repeated messages, even with identical timestamps.
Initial indexing keeps the recent inspected conversation visible; incomplete
sources and shortened message bodies are disclosed. Search covers the loaded
window. A confirmed own message stays visible when it falls out of the native
inspection tail.

Encrypted page storage retains up to 128 pages and 64 MiB for seven days, separate
from the recent-conversation cache. Cached retained pages can be browsed offline;
network reads require the corresponding native capability. Read positions and
markers survive page eviction. Automatic read progress acknowledges incoming replies only when they are
actually visible in the foreground with no modal covering the conversation.
Fresh complete canonical counters are exact; partial or stale evidence is shown
as a lower bound or unknown. First visits show the recent page without marking
it read merely because it opened. Mark read and Review later are local choices
and never answer a native question. Read all applies to the current conversation
or the Sessions scope on this phone, respecting the current project, machines
and search. It captures each conversation before changing local markers:
fresh complete heads mark the captured replies,
partial evidence marks only loaded replies, and unknown, unavailable or offline
rows without loaded evidence are skipped. Its result reports marked conversations, partial marks and skipped
rows. Reminders without usable reply evidence remain. New replies arriving
after the captured evidence remain unread; reading anchors and drafts stay put.
Bounded background metadata probes can prepare canonical heads for live sessions
with reported reply evidence, without opening their conversations or fetching
full history. Previously unvisited sessions with unknown evidence are not
silently marked read.

Context details show native size, limits, and cache evidence without inferring
warmth from cache hits. Compact and continue waits for the exact native request
to complete, then sends only its unchanged captured draft; editing, navigation,
uncertainty, or cancellation prevents the send. Clear requires a named
confirmation and preserves unsent text and files. Confirmed rename and resume
can move an ordinary editing draft only to their verified same-conversation
result. Saved drafts offers explicit recovery if that destination was unavailable.

Native controls are enabled only when both gateway and computer advertise them
and a fresh inspection permits the action. These include session lifecycle,
queue Send now, model and effort changes, process output and Stop requested,
scoped recovery controls, and exact child history. Scheduled model settings have
one durable authorized apply attempt when native readiness is observed, including
when another session is open. Uncertain actions expose their original receipt;
they are never retried automatically.

Terminal uses a separate saved input buffer and a fresh exact native binding.
Snapshots poll adaptively while visible; input sends literal text or fixed keys,
with confirmation for Ctrl+C and Ctrl+D. Submitted means native input handoff,
not command completion. New session uses the selected computer's native provider,
account catalog and an explicitly chosen folder; it never creates a Git repository
or worktree. A launch is confirmed only by its own native request identity.

## Build and install

Install JDK 17 or 21 and Android SDK platform 36 / build tools 35.0.0. Set
`JAVA_HOME` and `ANDROID_HOME` for those tools, then run:

```sh
cd mobile/android
./gradlew assembleDebug testDebugUnitTest lintDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

The checked-in Gradle wrapper uses Gradle 8.13 with an official distribution
checksum. Dependencies come from Google Maven and Maven Central. The debug APK
uses the local Android debug signing key; public distribution requires an
owner-controlled release key. Generated APKs and local configuration are ignored.

For device performance validation, `./gradlew assemblePilot lintPilot` produces
`app/build/outputs/apk/pilot/app-pilot.apk`. This build enables R8, resource
shrinking, and release performance while using the same development signing key
for pilot upgrades. Distributable pilot and release builds require a private
signing Properties file selected by `-PzerusSigningProperties`,
`ZERUS_ANDROID_SIGNING_PROPERTIES`, or the default
`~/.config/hgs/mobile/dev-signing/signing.properties`. Its fields are `storeFile`,
`storePassword`, `keyAlias`, and `keyPassword`; relative key paths resolve beside
that file. Keep the file, key and backup outside the repository. Existing dev
installations must keep their exact signing key. Ordinary debug builds use the
local test key independently. Pilot is not a store production release.

## Updates

About and updates is available from the main toolbar, including before pairing.
Anonymous checks use `https://zerus.dev/updates/v1/android/dev.json` at startup
and through WorkManager every six hours when a network is available. Android
may defer background work. The indicator remains available without notification
permission. Check, Download and Install are separate actions; no update silently
repoints or removes a paired workspace. The suggested relay for new pairing is
`https://relay.zerus.dev`, and custom HTTPS relays remain editable.

Downloads are bounded, cancellable and verified by exact size and SHA-256. Before
installation, the APK must match the advertised package, version, Android minimum
and the current installed signer. Saved verified downloads work offline.
Installation saves private drafts first and waits for active deliveries or
commands. Android's install-source permission and confirmation remain required;
the updater explicitly requests user action through
[PackageInstaller](https://developer.android.com/reference/android/content/pm/PackageInstaller.SessionParams#setRequireUserAction(int)).
An installer session is recorded before commit and never recommitted after
interruption. A lost confirmation screen requires explicit cancellation before
another attempt. Success is shown only after the installed package version is
observed. Update metadata, APK files and recovery state are app-private and
excluded from Android backup.

## App icons

Launcher icons reuse the selected Swarm blades from
[the canonical symbolic SVG](../../tray/resources/icons/hgs-zerus-symbolic.svg),
with the original curves and three rotations. The adaptive foreground is mint
`#67E8CB` on a full-bleed ink `#101517` background. Both layers use 108 dp; the
mark is centered at (54, 54), has an outer radius below 33 dp and bounds about
61 × 59 dp. Android supplies the launcher mask and shadow. Android 13 and later
receive the same geometry as a white monochrome layer for themed icons. The
round icon uses the same adaptive resource.

Notification icons use transparent white Swarm geometry in a 24 dp viewport,
with a roughly 20 dp outer diameter. Android 12 and later use the adaptive mark
on an ink splash background. The existing in-app brand image is unchanged.
Sizing and layer choices follow Android's
[adaptive icon design guidance](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive)
and [launcher asset documentation](https://developer.android.com/studio/write/create-app-icons).

## Notifications

“Keep a live connection” starts an explicit foreground remote messaging service
with a visible Stop action. It listens to the gateway event stream and follows
additional paired gateways. Android battery saving or network loss can delay
this connection; it is not a Doze wake-up guarantee.

“Set up UnifiedPush” discovers installed distributors and lets the user choose
one. A distributor such as ntfy or NextPush must already be installed and
configured. Registration status and gateway capabilities appear under Computers.
Push content acts only as a wake-up hint: the app immediately posts a generic
alert and schedules an expedited authenticated event fetch. Notifications contain
no conversation text. Once fetched, tapping an alert opens its computer/session.

Firebase is an optional build, excluded from the default APK. Create an Android
Firebase project for `app.zerus.mobile`, place its private local configuration in
`app/google-services.json`, and run:

```sh
./gradlew -PzerusFirebase=true assembleDebug testDebugUnitTest lintDebug
```

The gateway must advertise and configure FCM separately. The app checks that
capability before registering its token. Firebase automatic initialization is
disabled until the user chooses setup. Only data messages should be sent by the
gateway, allowing the app to show a private alert and fetch authenticated events.
No Firebase project credentials are included in this repository.

## Local validation

Unit tests cover exact destination identity, interrupted-delivery recovery,
HTTPS validation, one mutation attempt on a dropped connection, read-only receipt
polling, bounded responses, native user/assistant message merging, attention phases,
archive filtering, and verified composer readiness. Additional tests cover stable
project scoping and empty groups, delayed progress and stale operation ownership,
atomic composer/outgoing transitions, legacy draft migration, first-conversation
creation, ambiguous event reconciliation, immutable encrypted file copies,
tamper/cancellation handling, attachment presentation, and large bounded streamed
JSON envelopes. Android lint checks the app.
Device/emulator checks are still needed for push distributor setup and delivery,
Keystore persistence, notification permissions, and native conversation controls.

Pairing QR codes use CameraX 1.6.2 and ZXing core 3.5.4 without Google Play services. In Machines, open Pair a workspace and choose Scan QR to request camera access. Scanning only prefills the editable HTTPS gateway and invitation code; Pair remains an explicit action. The shared bounded parser also validates pairing deep links. Camera images and invitation codes are not saved or logged; leaving the scanner stops its camera. Manual entry remains available when access is denied or a camera is unavailable.

CameraX is Apache 2.0 and includes libyuv under BSD-3-Clause; ZXing is Apache 2.0. Full license texts and source attribution are available in Third-party notices. Pins follow the [official CameraX releases](https://developer.android.com/jetpack/androidx/releases/camera) and [ZXing 3.5.4 release](https://github.com/zxing/zxing/releases/tag/zxing-3.5.4). CameraX requires compile SDK 36 and AGP 8.9.1 or newer, covered by this build.

Accounts is a read-only viewer grouped by workspace and provider-reported account identity, matching desktop accounts. Each card contains its machine labels, using the same names and colors as Sessions. Provider account IDs and organization scope own identity; email-only reports join an ID only when unambiguous. Signed-out and unidentified profiles stay machine-local. Compact cards show account type and aligned period, remaining reset time and usage columns, with recurring-period and hourglass glyphs; tap a card for the full identity, limits, per-machine profiles and update times. Usage selects one best-quality report and never adds quotas across machines. It shows reported account identity, plan, usage windows and wallet balances when the connector provides them. Unknown limits stay unknown; ended windows need a provider refresh. Account snapshots use a separate encrypted private cache, so offline values remain available with their original provider update time and stale styling; offline and authentication problems stay visible. A connector advertising `accounts_snapshot` is required; Refresh reads its latest snapshot and does not force a provider refresh.

## Project and account selection

New session presents Project, Folder, Agent and Account as compact selectors.
Projects use the selected computer's fresh canonical swarm/project IDs, and
only its local folders are offered. The native browser verifies a chosen folder
again before creation. A folder explicitly chosen through Browse remains selected
when changing projects in the same dialog; a listed project folder follows its
project. Choosing an outside folder explicitly adds it to the selected project,
as the dialog states. No Git repository or worktree is created.

Accounts remain distinct native profiles. The actual reported default is
preselected, and an offered concrete account is always sent by its exact ID.
Identical labels include profile IDs; known cached sign-in status is shown
without treating credential-file presence as proof of sign-in. No synthetic
“Use native default” choice is added to a known catalog.

Project assignment requires the connector's `launch_project` feature and the
native `hgs swarm assign-launch --json` ABI. Older computers can still create
sessions, with an explicit project-assignment-unavailable notice. Creation is
confirmed by its launch UUID before the native helper checks the exact run,
session and current canonical project. Assignment is a separate metadata outcome;
it never retries session creation. A created session whose assignment failed or
is unknown remains a completed, nonblocking action, with a durable Drafts notice
offering Open created session and Mark reviewed.

The native helper records a private write-ahead intent in
`HGS_CONFIG_DIR/swarm/launch-assignments.json` before saving project metadata.
An interrupted intent is permanently uncertain and never reapplied. Recorded
outcomes survive later manual project moves, and the shared catalog keeps its
existing schema for older desktop binaries. The sidecar is bounded to 10,000
permanent receipts; at capacity new assignments fail before changing metadata.
Receipts never enter swarm exports or peer synchronization.
