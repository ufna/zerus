# A team page that works without the lead's session (zerus-63rru.198, wave 3b)

Status: design note for review. Nothing is built until the coordinator answers.
Source: Teams UI audit round 2, item A1 (report SHA-256 db4134a8…).
Main read: 6b7181036. Every file:line below is at that commit. Since then, #2548 (merged in
28f41bc9b) added two lines to `store/teams.ts` after line 57, so that file's later lines are
2 higher on current main. No other file cited here changed.
Screenshots of today's dead end, in English and Russian:
`artifacts/test-results/teams-w3b/screens/` in this worktree.

## The problem

Every team view and write lives in the lead session's Team tab.
- `SessionDetail.tsx:226-250` shows the tab only on a lead row.
- `TeamTab.tsx:403-455` finds its team from the selected row.

A Teams row opens nothing else. It shows "The team's lead has no session yet." or "The
team's lead session is not on the session list." (`Teams.tsx:73-77`;
`dead-end-no-session(-ru).png`, `dead-end-not-listed(-ru).png`). Waiting's Open team and
Open task end the same way (`screens/Waiting.tsx:48-63`).

So a team whose lead has no listed session cannot be stopped, archived, configured, staffed
or decided. A person has no other route: the CLI refuses person writes (`src/teams/cli.rs:29-62`).

## What the code already allows

None of this needs the lead session; read and followed to the producing statement.

**Reads are keyed by team ID alone and run this computer's `hgs`.**
- `teams.openTeam(id)` takes only an ID (`store/teams.ts:633-638`). It follows `{list, waiting,
  team}` through `teams_follow` (`src-tauri/src/teams.rs:95-119`) and the core poller
  (`core/teams/model.rs:261-313, 520-527`).
- The poller reads:
  - `hgs team status --team T --bundle --json` (`core/teams/read.rs:107-177`; producer
    `src/teams/cli/reads.rs:33-84`);
  - `hgs team queue --team T --limit 200 [--after C] --json` (`reads.rs:87-128`).
- The core joins them into the `TeamView` the workspace shows (`board.rs:205-237`).
- Drawers, evidence, phases, observed timings, base and Tests and Builds also take the team
  ID (`teamDrawers.ts:30-33`, `src-tauri/src/teams.rs:163-179, 316-362`).
- Everything is bounded:
  - pages of 200 (`read.rs:18`, `store/records.rs:7`);
  - at most 256 members (`view/mod.rs:38`);
  - 16 stages per card (`board.rs:133-167`);
  - one drawer read at a time (`team_drawers.rs:9-21`);
  - list lookups of at most 5 pages (`teams.ts:264`);
  - every read capped at `READ_LIMIT` with a 15 s timeout (`read.rs:24, 446`).
- There is already a session-free precedent: `teams.revisionOf(id)` borrows a team follow for
  Waiting's decisions (`teams.ts:684-709`).

**Writes need no session at any hop.**
- `teamCall(view)` builds `{team, team_name, expected_revision, request_id, action}`
  (`ui/teams/actions.ts:11-13`).
- `teams.run` (`teams.ts:844-869`) passes it through `team_action`
  (`src-tauri/src/teams.rs:184-266`) to the native Person lane
  (`desktop_service/team_person_actions.rs:148-292`; decide: `team_person.rs:113-297`).
- The core `TeamCall` is `deny_unknown_fields` (`core/teams/action/types.rs:305-320`), so
  a session field cannot even be sent.
- The service reviews against the team row and its revision. It binds its own computer
  (`src/desktop/teams/person_actions/runtime.rs:225-296`, machine at `:275`) and writes as
  the person "local" (`src/teams/ops/authority.rs:6-120`).

**The lead session is used in the client only:**
1. finding the team ID: `navigationTeamFor` (`lead-navigation.ts:12-25`) and
   `teamOfSession` (`teams.ts:485-499`);
2. restarting the follow on every row change (`TeamTab.tsx:405-411`);
3. hiding the view when the lead changed (`TeamTab.tsx:419-427`);
4. **Lead chat**, which is the lead session's own composer and needs the selected lead row
   (seven conditions, `lead-chat.ts:33-39`; `TeamLeadChat.tsx:14-21`);
5. Catalog's Back, which remembers the selected row (`catalog-navigation.ts:17-22`).

`TeamWorkspace` itself takes a `view` and callbacks and needs no session
(`TeamTab.tsx:64-78`).

**One team at a time.** The store follows a single team (`teams.ts:332, 414`). The pages
are a `Switch` (`App.tsx:159-174`): leaving Sessions unmounts the lead tab and closes its
follow. A team page and the lead tab therefore never compete.

## The design

### 1. A team page

- A new page `team` (`store/ui.ts` `Page`), with a `teamPage` signal holding a team ID.
- `TeamPage` follows the ID with `teams.openTeam` on mount and `teams.closeTeam` on cleanup,
  like `followTeamScope`. It then renders:
  - a header: team name, status, home computer and **Back**;
  - the same `TeamWorkspace`, with an `act` built exactly as the tab's
    (`teamCall(view)` → `teams.run(call, "team-page:<id>")`) and the same props
    (`TeamTab.tsx:429-455`).
- The tab's wiring of `act`, `onSettingsSave`, the blocked and unresolved flags,
  `providerHome` and `onPage` moves into one shared component, `TeamBody`. The tab and the
  page then show the same thing.
- **Back** returns to where the page was opened (Teams, Waiting or the lead session), with
  the catalog's single-slot pattern (`catalog-navigation.ts:7-31`). There is no router, and
  none is added.
- **Lead chat on the page:** it shows "Open the lead's session to talk to it" with **Open
  lead session** when the lead's row is listed. Otherwise it gives the cause from
  `lead-chat.ts` (no session, archived, another computer…), as audit C4 asks.
- **Add task on the page:** `requestTeamTask` needs the task panel on the team's project
  (`teamTaskIntake.ts:10-16`), which follows the selected session (`taskContext.ts:9,25-27`).
  The page sets the tasks panel's project to the team's before it opens the form, or
  disables the button with that reason (slice 4).
- **Open task on the page:** `openTask` opens Sessions' Task View without switching page
  (`taskNavigation.ts:28`). On the page it switches to Sessions first (slice 4).

### 2. The Teams list

- **A row click opens the team page** (`Teams.tsx:73-77`), lead or no lead. The two
  dead-end notices go.
- **A row menu** ("…") uses the slot that already exists: `TeamsRow` actions
  (`TeamsScreenFrame.tsx:14-25`) and `TeamsScreenProps.rowActions` (`Teams.tsx:44-48`).
  Today only tests pass it; `App.tsx:172` would. It is built like `MemberActions`
  (`TeamActions.tsx:67-93`; `setMenu`, `MenuLayer`). Entries:
  - **Open** (the page);
  - **Open lead session** (when the lead's row is listed; disabled otherwise, with the reason
    as the item's note);
  - **Settings…** (opens the page with the Settings dialog, which needs the full `TeamView`,
    `TeamTab.tsx:221-222`);
  - **Pause** / **Resume**, **Stop…**, **Archive…**, by status, from `barActions`
    (`actions.ts:39-48`).
- **Lifecycle items** run from the row with the list row's ID, name and revision
  (`TeamSummary`, `teams.ts:22-31`). They go through the same `TeamConfirmDialog` and native
  confirmation. A stale revision is refused by the service and read again, as today.
  - Pausing a planning team needs the `TeamView` (`actions.ts:39-48`). For that status the
    item opens the page first.

### 3. Other ways in

| Entry | Today | With the page |
|---|---|---|
| Teams row | the lead tab, or a dead end | the page |
| Waiting: Open team / Open task (`screens/Waiting.tsx:48-63`) | the lead tab, or "…not on the session list." | the page; Open task also sets `setOpenTeamNode` (`openNode.ts`), which the workspace already honours (`TeamTab.tsx:130-152`) |
| Waiting: Open phase (`teams/Waiting.tsx:51`) | `openPhase`, then Open team | `openPhase`, then the page |
| Cards drawer "#N" (`ProposalCards.tsx:11-22`) | inside the open workspace | unchanged; works on the page too |
| Lead strip on a session card (`SessionList.tsx:371-375`) | the lead tab | unchanged (it is the lead's own card) |
| Catalog's Back (`catalog-navigation.ts`) | Teams, or the lead tab | also the page |
| Notifications | sessions only (`bridge.ts:926-940`) | unchanged |

### 4. What the lead session's tab becomes

- It stays, as the shortcut it is today, and shows the same `TeamBody`.
- It gains **Open team page** in its header.
- Lead chat keeps working only there, because it is that session's composer.
- One choice is the coordinator's (question 1): keep the full workspace in the tab, or
  reduce the tab to a summary with **Open team page**.

### 5. Another computer, offline, no service

- **Teams are this computer's.** The list reads the local store only (`reads.rs:176-188`;
  "Teams on this computer", `TeamsScreenFrame.tsx:8`). The poller tags everything with the
  local computer (`core/teams/poller.rs:51-79`). The service writes as its own computer
  (`runtime.rs:275`).
  - A team homed on another computer is not on this list; it is opened on that computer.
  - The page shows no remote teams and adds no remote lane.
- **Home is not this computer** (`TeamSummary.home` differs from `machineCards[""]`): this
  can happen when state moved between computers. The page shows the team read-only with
  "This team's home is another computer (%1). Open it there." and disables writes.
- **The desktop service is not connected:** reads still work (local `hgs`). Writes refuse
  with "Connect this window to its desktop service before changing a team."
  (`team_person_actions.rs:169-178`). The page shows that sentence once, as a banner above
  the workspace, rather than after each press.
- **Teams off or unavailable:** the page shows the same notice as the list (`Teams.tsx:91`),
  with wave 3a's link to Settings.
- **The team is gone or archived:** the follow ends with `ended` or `failed`
  (`model.rs:66-74`). The page says so and offers Back. An archived team shows read-only;
  wave 3a's "Show archived" decides whether it is listed.

## Living with wave 3a (zerus-63rru.197) and #2548

- **Wave 3a** (branch `fix/teams-ui-wave3a-dead-ends-20261119`, local in the w3a worktree)
  changes 33 files. Among them: `Teams.tsx` (Show archived, `For each={visible()}`),
  `screens/Waiting.tsx`, `CatalogPage.tsx`, `TeamTab.tsx` (an import near `:10` and a member
  action near `:359`), `TeamReports.tsx`, `TeamDrawers.tsx`, `ProposalCards.tsx`,
  `TeamActions.tsx`, `actions.ts`, `TeamConfirmDialog.tsx`, the i18n files and
  `docs/agent-teams.md`.
- **#2548** has merged (28f41bc9b). It changed `TaskPlacement.tsx`, `store/teams.ts` (+2),
  fixtures, the i18n files and docs, and none of the files this plan edits beyond the i18n
  files.
- **Plan:** wave 3b starts from main after wave 3a merges, and keeps its own hunks small:
  - **New files:** `TeamPage.tsx`, `TeamBody.tsx`, `team-navigation.ts`,
    `TeamRowMenu.tsx`.
  - **One-hunk edits:**
    - `Teams.tsx`: `open` only (`:73-77`), not the row loop wave 3a changes;
    - `TeamTab.tsx`: the wrapper at `:403-455` only, not wave 3a's hunks;
    - `screens/Waiting.tsx`: the open effect at `:48-63`;
    - `App.tsx`, `store/ui.ts`, `catalog-navigation.ts`: none touched by either.
  - **Shared with both:** the i18n files (union at merge) and `docs/agent-teams.md` (a new
    paragraph, not wave 3a's).
- If wave 3a has not merged when slice 1 is ready, I rebase on its branch instead and say so.

## Slices (smallest useful first), with test plans

Every slice has:
- **RED first on unmodified code.** Render tests use the real teams store, with
  `teams.apply` fed from producer fixtures: `screens/fixtures/teams-screen.json`
  (`teams_screen.rs`) and the lanes and status fixtures (`src/teams/cli/tests`). Only
  `invoke` is replaced.
- **At least five runtime mutants** at its production lines.
- **Screenshots in English and Russian** from the audit harness.
- tsc, the Teams vitest set, the i18n check, separators and the 500 kB entry bundle.

No Rust changes are expected.

1. **Team page, opened from the Teams list.** The page, `TeamBody` shared with the tab, and
   a row click opening the page.
   - Tests:
     - a team whose `lead_session` is null opens to its workspace;
     - the follow is `team:<id>` and closes on Back;
     - Pause, Settings and Add member build their calls from the view with no session in
       them (the `invoke` payloads);
     - Back returns to Teams;
     - the tab still shows the same body.
   - Mutants: the follow not closed, the call missing the revision, the row click still
     requiring the lead, Back to the wrong page, the source key shared with the tab.
2. **Row menu.** Open, Open lead session, Settings…, Pause/Resume, Stop…, Archive… by
   status, with reasons for disabled items.
   - Tests: entries per status from the fixture's teams; lifecycle payloads use the row's ID
     and revision; a planning team's Pause opens the page; the confirmation dialog is the
     existing one.
   - Mutants: the wrong status gate, a missing confirmation, a stale revision used, Open lead
     session enabled without a row.
3. **Other ways in.** Waiting's Open team, Open task and Open phase open the page; the tab
   gains Open team page; Catalog's Back can return to the page.
   - Tests: `Waiting.open.test.ts` cases for a team without a lead session; the
     `setOpenTeamNode` handoff; `CatalogReturn` with the page as origin.
4. **Page-only gaps.**
   - Lead chat's reason and its Open lead session link (audit C4).
   - Add task setting the tasks panel's project, or a disabled reason.
   - Open task switching to Sessions.
   - The home-computer and service-disconnected banners.
   - Tests: one per gap, plus the existing Lead chat tests on the tab.
5. **Optional, after use:** reduce the tab to a summary with Open team page (question 1).

## Questions for the coordinator

1. **The lead session's tab.** Keep the full workspace there (my recommendation for now: no
   regression, and Lead chat lives there), or make it a summary with **Open team page**?
2. **Row-menu lifecycle actions.** Run them from the row with the list's revision through
   the native confirmation (recommended), or always open the page first?
3. **A team whose home is another computer** (moved state). Show it read-only with the
   notice (recommended), or hide it?

## Evidence

- Screenshots, from the audit harness (a copy) with a synthetic scenario that adds a team
  with no lead session and one whose lead session is not listed (`harness-w3b-changes.txt`):
  - `teams-list(-ru).png`;
  - `dead-end-no-session(-ru).png`;
  - `dead-end-not-listed(-ru).png`.
- The window build is #2533's head plus its list fix. `Teams.tsx` is identical to main
  6b7181036.
- The read and write maps behind this note, with every file:line:
  `artifacts/test-results/teams-w3b/notes/`. I re-read the claims above against main.
