# Zerus Next release acceptance and Qt removal

Owner: `zerus-5wmh.66.8.1` (G6). Basis: the
[retirement inventory](superpowers/specs/2026-10-29-qt-retirement-inventory.md),
especially its responsibility tables, migration contract and G1–G7 gates.
Execution order for this assignment: **Linux on arch, macOS, Windows on va-kids**.
Windows is reachable; the inventory's original unreachable-PC note is historical.
**All native acceptance below is pending.** A merged PR or a private fixture pass
does not check a box. Qt deletion stays a draft until the coordinator accepts the
results for all three platforms; do not deploy the deletion branch for these tests.

For each platform, record the exact Next and last-Qt release/source, installed
`--build-info`, package/artifact identity and installer route in the acceptance
PR comment. Fill every row with PASS or a linked blocking bead and the actual
steps/results. Keep logs bounded and redact account/message/device data. Preserve
Qt settings, drafts, outboxes and connector journals; never restart agents, tmux
or unrelated services. Explicit confirmations remain required for destructive UI.

## Linux — arch

Use the maintained AUR/package route and `tray/install.sh`; use a disposable
profile/prefix for clean-install checks, then the coordinator's maintained owner
upgrade route for live desktop checks. Do not install a test binary by hand.

| Pending check | How to verify | Coverage / owner |
|---|---|---|
| [ ] Clean install | Install the exact Next package in a clean profile; open About and compare GUI/CLI identities. Inspect package contents/dependencies for Qt/KF6/WebEngine absence; confirm accounts/projects are initially empty. | G5 `.66.7.1`, [#867](https://github.com/ufna/zerus/pull/867); linux-shell-u |
| [ ] Upgrade with agents running | From the last Qt release, keep a known terminal/native session and an unsent draft open; record exact session/run/conversation identities. Upgrade through the maintained installer/updater, reopen that same session and verify identity, jobs, terminal and draft survived without a restart. | [#886](https://github.com/ufna/zerus/pull/886), G2 `.66.4`, G6 `.66.8` |
| [ ] One-time import | Compare selected Qt settings, collapsed rows, folder split, cleanup choices, window geometry, drafts and read marks. Change a Next preference, restart twice and roll back/forward: no reseed, source writes or replay. Malformed/partial families remain recoverable. | G3 `.66.5.2.1`, `.66.5.3`, [#870](https://github.com/ufna/zerus/pull/870) |
| [ ] Service registration/readiness | Inspect the generated user unit and `hgs desktop status --json`: one owned service, independent background readiness, graphical-session environment imported. Close Next; catalog/details, presence and durable read delivery continue. Custom units refuse overwrite. | `.66.3.27`, [#886](https://github.com/ufna/zerus/pull/886), [#898](https://github.com/ufna/zerus/pull/898), `.66.3.31` |
| [ ] Notifications/clicks | With Next closed, produce one scoped attention event; get one Freedesktop alert and badge, click into the exact run, then test stale/withdrawn click refusal. Lock/idle does not invent activity. | `.66.3.30.1`, `.66.3.30.3`, [#894](https://github.com/ufna/zerus/pull/894); linux-shell-u/desktop-u |
| [ ] Tray/lifecycle | Check panel icon, primary click, menu, background start and second-instance routing; closing windows keeps service duties, Quit flushes drafts and does not stop agents. | G4 `.66.6`, `.66.3.31` |
| [ ] Terminal | Open an existing session and a folder terminal with spaces/non-ASCII paths; check exact argv, resize, paste and reattach after client exit. Never create a worktree by opening a dialog. | inventory “Terminal and process lifetime”; G7 `.66.2`, native terminal contracts |
| [ ] Voice/video | Explicitly choose microphone/camera; record, preview, cancel and transcribe a short synthetic message. Hide/Escape stops capture, private draft survives exit, and an uncertain delivery is not resent. | G2 `.66.4.5`, inventory “Voice, video and speech providers” |
| [ ] UI/durable parity | Exercise each inventory responsibility with its target owner: bounded Activity/archive/search/tasks pages, organization writes, creation/recovery, read marks and manual questions. Approval opt-in starts off; generic/agent callers cannot enable it. | G1–G4/G7; [#884](https://github.com/ufna/zerus/pull/884), `.66.3.32`, `.66.3.35`, `.66.3.38` |
| [ ] Qt rollback | Before removal, use the maintained previous-version rollback. Require settled desktop revoke/drain before Qt starts; missing/unknown revoke refuses. Verify the exact agents, drafts and original settings still exist; then return through the maintained Next route. | `.66.3.31`, `tray/update-installed.py`; desktop-u |

## macOS

Use the signed/notarized Next bundle/cask and the maintained local installer for
owner evaluation. Source selection before retirement is `HGS_MAC_GUI=client`
through `mac-install.sh`/`tray/install.sh`. Keep the existing privacy identity,
signature, installed path and one verified rollback copy.

| Pending check | How to verify | Coverage / owner |
|---|---|---|
| [ ] Clean install | Install the exact signed/notarized `Zerus.app` in a clean profile. Check bundle/CLI identity, signature/notary and package checks; no Qt frameworks/plugins. Cask install itself must not quit apps or start services. | G5 `.66.7.2`, [#871](https://github.com/ufna/zerus/pull/871); macbundle-u |
| [ ] Upgrade with agents running | Upgrade last Qt via the maintained updater; verify exact running session/run/conversation, terminal/jobs and flushed drafts survive. Keep `com.hgdev.hgs-tray` and `~/Applications/hgs-tray.app`; no native-agent/tmux restart. | `.12.6`, [#871](https://github.com/ufna/zerus/pull/871), `tray/update-installed.py` |
| [ ] One-time import | Check exported Qt defaults, layout/window bounds, drafts and read/attention marks; Next values win. Restart and rollback/forward without reseeding handled families; preserve original plist/export/store bytes. | G3 `.66.5.2.1`, `.66.5.3`, [#870](https://github.com/ufna/zerus/pull/870) |
| [ ] Service registration/readiness | Through explicit `zerus-setup`, inspect the Aqua `com.hgdev.hgs-desktop` LaunchAgent and actual `desktop status`; close Next and verify background duties continue. Foreign/custom definitions are refused. | `.66.3.27.2`, [#882](https://github.com/ufna/zerus/pull/882); macbundle-u/desktop-u |
| [ ] Notifications/clicks | With Next closed, post one service-owned alert through the signed host; check badge, exact-session click, stale click refusal and lock/idle behavior. Existing notification permission remains attached to the public app identity. | `.66.3.30.2`, `.66.3.30.3`, [#891](https://github.com/ufna/zerus/pull/891) is extraction only |
| [ ] Menu bar/lifecycle | Check icon/menu, reopen, Dock/menu-bar policy, second instance, Services/Hide/Show All and draft-safe Quit; agents and service remain independent. | G4 `.66.6`, `.66.3.31`, inventory native shell/menu rows |
| [ ] Terminal | Open exact session/folder in the configured Terminal path, with quotes/spaces/non-ASCII; accept only the expected Apple Events consent. Check resize/paste and session survival after UI exit. | inventory Mac terminal rows; native terminal contracts |
| [ ] Voice/video | Exercise real microphone/camera consent, short preview/transcription, cancel/hide and exit recovery. Check public privacy identity; never import an old device ID or replay uncertain delivery. | G2 `.66.4.5`, inventory media/privacy rows |
| [ ] UI/durable parity | Apply the same complete inventory responsibility walk as Linux, including bounded details/tasks, archived reads, creation recovery and default-off approvals with authenticated frontend-only enablement. | G1–G4/G7, `.66.3.31/.32/.35/.38` |
| [ ] Qt rollback | Use the retained complete Qt copy and original maintained updater before deletion; desktop revoke must settle before Qt starts. Verify settings/drafts/agents, then switch back to Next without a second import. | `.66.3.31`, updater rollback and defaults-snapshot checks |

## Windows — va-kids

Use **CI-built Windows artifacts from the exact tested commit**, never Arch
binaries. Stage in a unique private folder under kinder's profile. Interactive
checks are owner-authorized; close opened windows and remove test registrations
unless testing the final install. Do not touch unrelated desktop/apps/settings.

| Pending check | How to verify | Coverage / owner |
|---|---|---|
| [ ] Clean install | Test portable ZIP and per-user installer. Verify `zerus-windows-x86_64/hgs-tray.exe`, adjacent `hgs.exe`, connector/ConPTY and actual WebView2 readiness; no Qt DLLs/plugins. | G5 `.66.7.3/.4`, [#875](https://github.com/ufna/zerus/pull/875); fleet-c |
| [ ] Upgrade with agents running | Upgrade the last Qt install in its owned folder; verify exact native session/run/conversation and ConPTY jobs, saved drafts and accounts survive. Stale Qt payload cleanup must stay inside that installer-owned folder. | `.66.7.3`, `.66.6.15`; fleet-c/fixes-u |
| [ ] One-time import | Check actual legacy registry/INI preferences, folder mapping, geometry, drafts and attention seed. Restart and rollback/forward: no source writes, reimport or automatic policy inheritance. | G3 `.66.5.2.1`, `.66.5.3/.10`, [#870](https://github.com/ufna/zerus/pull/870) |
| [ ] Service registration/readiness | Check opt-in registration in the interactive user session, never LocalSystem/Session 0; verify named-pipe identity and independent background readiness with UI closed. Remove temporary logon entries. | `.66.3.27.3`, `.66.3.31`, [#874](https://github.com/ufna/zerus/pull/874); fixes-u/desktop-u |
| [ ] Notifications/clicks | In the real desktop, receive one service-owned toast/badge, click to exact session, reject stale clicks and check lock/idle state without manipulating unrelated apps. | `.66.3.30`, `.66.6.15`; fixes-u |
| [ ] Tray/lifecycle | Check tray/menu, background/second-instance routing, WebView2 window, draft-safe exit and opt-in login start; remove temporary startup values and shortcuts. | [#874](https://github.com/ufna/zerus/pull/874), `.66.6.15` |
| [ ] Terminal | Check PowerShell/Windows Terminal/ConPTY paths with spaces/non-ASCII, exact session selection, resize/paste and live-session continuity. Exercise supported operations only; unsupported remote paths must refuse clearly. | `.52`, `.66.6.15`, Windows native contracts; fixes-u |
| [ ] Voice/video | Verify actual supported capture/preview/transcription/cancel paths and privacy consent; unsupported paths refuse visibly. An unavailable release-required inventory capability blocks acceptance rather than counting as a skip. | G2 `.66.4.5`, inventory voice/media rows |
| [ ] UI/durable parity | Run the complete inventory responsibility walk, especially tasks, folder mapping and bounded details; approvals remain default-off and generic/agent toggles refuse. UI-only mode has no competing GUI Fleet. | G1–G4/G7, `.66.3.31/.32/.35/.37/.38` |
| [ ] Qt rollback | Before deletion, restore last Qt using the maintained rollback/install route only after settled service revoke. Verify original state and live sessions, then return to Next. Remove test staging/registrations only after owned processes exit. | `.66.3.31`, `.66.7.3`; desktop-u/fleet-c |

## Deletion PR merge gate

The coordinator records per-platform acceptance and any exceptions against the
inventory; unmet G1–G5/G7 responsibilities remain blockers. The draft deletion PR
must be rebased on the final installer/consumer/notification/packaging PRs and pass
source/privacy, workflow/security, package and retained migration/updater tests.
Audit final artifacts for Qt/KF6/WebEngine requirements and dangling source paths.
Keep `hgs`, `HGS_*`, service/app IDs, installed paths, user data, migration readers,
CLI/connector/ConPTY and immutable published assets unchanged. Keep legacy update
feed fields and sealed-candidate retry templates while old clients require them.
Only then remove Qt build code, jobs, dependencies and the obsolete fallback
offer; the coordinator owns merge and rollout. This document authorizes no install,
service cutover, credential operation or deletion of user data.
