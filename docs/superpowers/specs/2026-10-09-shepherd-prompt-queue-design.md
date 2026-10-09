# Shepherd: per-session prompt queue — design

Date: 2026-10-09. Status: draft for review.

## Problem

While an agent works on one task, the user keeps having ideas for the next ones
and writes them in a separate notebook. Zerus should hold these prompts per
session, with images, and feed them to the agent one by one when it has really
finished, or when the user picks one. The queue behaves like a small task list
whose tasks are ready prompts.

## Decisions taken in discussion

- The queue lives on the agent host and is dispatched there, so it works while
  the desktop GUI is closed or the laptop sleeps, and every peer sees the same queue.
- The host recovery worker is generalized and renamed to **shepherd**
  (`hgs shepherd`); it runs recovery and the queue in one loop.
- "Finished" waits for background work the agent will be woken by. What blocks
  the queue is visible, and **Send now** overrides it.
- Priority uses three levels (Urgent, Normal, Later) with manual order inside
  each level. A per-prompt **Hold** flag keeps a prompt out of automatic dispatch.
- Lifecycle: Queued → Active (sent, the agent works on it) → Archive (the
  session has completely finished). Archived prompts can be returned to the queue
  as copies.
- UI follows the composer toolbar: a **Queue** chip with a popover, an **Add to
  queue** split button, and editing in the regular message field.

Defaults chosen without explicit discussion, open to change in review:

- UI name: chip **Queue**, popover title **Prompt queue**; "shepherd" appears
  in tooltips, documentation and service names.
- Enter keeps today's behavior while a turn works (the native agent queues the
  message). **Alt+Enter** and **Add to queue** put the prompt into the shepherd queue.

## Non-goals (v1)

Time-based scheduling, conditional or chained prompts, moving prompts between
sessions, queues for subagents, editing an Active prompt, mobile client UI,
image thumbnails in the queue list (v1 shows names and counts; Edit shows the
images), and changing what Enter does while a turn works.

## Part 1. Rename the recovery worker to shepherd

| Today | After | Compatibility |
|---|---|---|
| `hgs recovery worker` / `tick` | `hgs shepherd run` / `tick` | old commands remain aliases |
| `hgs-recovery.service` | `hgs-shepherd.service` | packages ship `hgs-recovery.service` as a symlink to the new unit, so existing `enable` links keep working as a systemd alias |
| `com.hgdev.hgs-recovery` | `com.hgdev.hgs-shepherd` | the installer boots out and removes the old LaunchAgent it created |
| `scripts/install-recovery-service.py` | `scripts/install-shepherd-service.py` | the installer disables and removes the old per-user unit only when its content matches what the old installer wrote |

- The single-instance lock stays at `recovery/worker.lock`, so an old and a new
  worker can never run together during an upgrade. State paths stay unchanged.
- `recovery/heartbeat.json` keeps its fields (the DeepSeek bridge reads
  `enabled` as the recovery policy) and gains `"shepherd": {"version":1,"queues":N}`.
- `tick()` always runs. The recovery part is skipped while the recovery policy is
  disabled; the queue part is skipped while no queue has work.
- `ensure_worker()` spawns `hgs shepherd run`. It is called from recovery writes
  as today and from queue writes that can make work (add, resume, release hold).
- Update `install.sh`, `mac-install.sh`, `packaging/linux/`, AUR packages,
  `scripts/check-package.py`, `packaging/linux/zerus-setup`, `docs/reference.md`
  and `docs/ci-and-aur.md`.

This is the first, separate change, merged before any queue code.

## Part 2. Host queue store and CLI

### Storage

`<state>/queue/<record-file-name>/`, mode 0700, next to `recovery/` under the
same state root, keyed exactly like recovery jobs (`legacy_record_path`).

- `queue.json` — the manifest, written atomically under the global state lock.
- `files/<sha256>.bin` — attachment bytes, mode 0600, immutable, shared by
  items with identical content and removed when no item references them.

```json
{
  "version": 1, "name": "orbit/api-refactor", "mode": "auto",
  "pause_reason": "", "updated_at": 0,
  "order": {"urgent": ["id"], "normal": ["id", "id"], "later": []},
  "active": ["id"], "archive": ["id"],
  "items": {"id": {
    "id": "uuid", "version": 3, "created_at": 0, "updated_at": 0,
    "text": "…", "priority": "normal", "hold": false,
    "attachments": [{"name": "shot.png", "mime": "image/png", "bytes": 1234,
                     "sha256": "…", "reference": "[Image #1]"}],
    "state": "queued", "origin": "", "request_id": "", "sent_at": 0,
    "acknowledged": false, "finished_at": 0, "outcome": ""
  }},
  "readiness": {"state": "waiting", "reason": "background_task",
                "detail": "npm run dev", "at": 0}
}
```

Item states: `queued`, `dispatching`, `active`, `uncertain`, `archived`.
The `active` list holds `dispatching`, `active` and `uncertain` items in send order.
Queue modes: `auto`, `paused`; pause reasons: `user`, `stopped`, `uncertain`,
`blocked`, `session_ended`.

Limits: text and attachments follow `hgs send` (64 KiB, 8 files, 10 MiB each,
20 MiB total); at most 200 queued items. The archive keeps the newest 100 items
and at most 200 MiB of attachment bytes; whole oldest archived items are dropped
first.

### CLI

`hgs [@host] queue SESSION <operation> --json`, JSON request on stdin and
JSON result on stdout, routed over SSH like `send`.

| Operation | Request | Effect |
|---|---|---|
| `get` | — | manifest without bytes, plus readiness and shepherd heartbeat age |
| `item` | `id` | one item with `data_base64` attachments (for Edit) |
| `add` | `request_id`, `text`, `attachments`, `priority`, `hold` | appends to the end of its level; idempotent by `request_id` |
| `update` | `id`, `version`, `text?`, `attachments?`, `priority?`, `hold?` | queued items only |
| `move` | `id`, `priority`, `before` (id or null) | reorders or changes the level |
| `delete` | `id`, `version` | queued or archived items |
| `restore` | `id` | copies an archived item to the end of its level |
| `mode` | `mode`, `reason?` | Auto on/off, Resume |
| `send-now` | `id` | dispatches immediately (see below) |
| `resolve` | `id`, `outcome` | for `uncertain`: `delivered` → Active, `return` → queued with Hold |

Item edits check the item `version`, not a queue-wide revision, so the worker
moving another item never conflicts with the user's edit. A stale version
returns "This prompt changed; refresh the queue".

`hgs ls --json` adds a small `queue` object to sessions with a non-empty queue:
`{"queued":4,"held":1,"active":1,"mode":"auto","state":"waiting","reason":"background_task"}`.
`hgs inspect` adds `prompt_queue` (the `get` result). A host without these fields
runs an older HGS; the GUI then hides queue actions for that machine and explains
"Update HGS on this machine to use the queue".

## Part 3. Dispatch in shepherd

### Readiness

A pure function `readiness(snapshot, processes, queue, now)` returns
`ready`, `waiting(reason, detail)` or `blocked(reason)`. The queue may dispatch
only when all of these hold:

1. The session is running, its agent process is alive, and it is not pausing.
2. Phase is `idle` after a main-agent Stop, with no active tools.
3. No subagent is working (`subagent_active_count == 0`).
4. No recovery job is active, and no provider error, approval, input or trust
   dialog is pending.
5. No `input_pending_at` without a newer hook event, and the native input queue
   is empty (`input_queue` from the terminal screen).
6. No background job that will wake the agent is running or unknown:
   Claude `run_in_background` tasks until their `task_notification`, open Codex
   `exec_command` sessions, running Kimi tasks and DeepSeek jobs. Plain OS
   descendants (`process_tree`) do not block.
7. A quiet period of 15 seconds has passed since the last hook event. This
   covers the handback turn Claude starts right after Stop.
8. The queue is in `auto` mode and has a queued item without Hold.

Waiting reasons shown to the user: `agent_working`, `background_task`,
`subagents`, `needs_attention`, `recovery`, `settling` (with the remaining
seconds), `all_on_hold`, `paused`, `session_not_running`.

Process collection is the expensive part, so shepherd runs it only for sessions
that pass checks 1–5 and have an eligible item, at most every 5 seconds per session.

### Dispatch and acknowledgement

The flow follows recovery:

1. Under the lock, the first eligible item (Urgent, then Normal, then Later; in
   list order) becomes `dispatching` with a new `request_id` and `sent_at`.
2. Outside the lock, `input::submit` sends its text and attachments with origin
   `queue:<item id>`, verifying the run and conversation identity observed by
   the tick. DeepSeek sessions use the extracted `dsh::send` function.
3. Under the lock, a submitted receipt makes the item `active`. A failure
   without a receipt returns it to `queued`; three such failures in a row pause
   the queue with `blocked` and the last error. Anything else makes the item
   `uncertain` and pauses the queue with `uncertain`; shepherd never resends.
4. A `UserPromptSubmit` or turn start after `sent_at` sets `acknowledged`.
   Without it within 30 seconds, the item becomes `uncertain`.
5. All Active items move to the archive when readiness checks 1–7 pass again
   after acknowledgement, with `outcome: done`.

A worker restart during `dispatching` makes the item `uncertain`, like recovery.
Only one dispatch per agent and account runs at a time, shared with recovery.

### Interruptions and other events

- Any interruption (phase `interrupted`), whether from Stop in Zerus or Escape
  in Terminal, pauses a non-empty queue with `stopped`: the user intervened and
  decides when to continue. Active items whose `sent_at` precedes it return to
  the top of their level with Hold. The GUI does not also restore such a prompt
  into the message field.
- New input typed by the user does not cancel the queue; the next dispatch
  simply waits for readiness again.
- Recovery takes precedence: the queue waits while a recovery job is active.
- Rename moves the queue with the session (`rename_job` gains a queue
  counterpart inside the same transaction).
- When the session is archived or terminated, the queue pauses with
  `session_ended` and keeps its items. A new session with the same name starts
  paused and the user resumes it explicitly.
- `send-now` runs the same submit in the CLI process under the item's
  `dispatching` claim, without readiness checks. The GUI asks for confirmation
  when the agent is working: the native agent will receive it as a queued message.

## Part 4. Desktop GUI

### Toolbar chip and popover

A new `ComposerToolbar::Slot::Queue` in the leading zone after Attachments.
The chip appears while the queue has queued, Active or uncertain items, whatever
its mode.

| Situation | Label (short) | Tone |
|---|---|---|
| waiting, agent working | Queue 4 (≡ 4) | Neutral |
| waiting for background task | Queue 4 · waiting for background task (≡ 4) | Neutral |
| settling | Sending in 12 s (12 s) | Success |
| Active | Active · 3 queued (▶ 3) | Success |
| paused or Auto off | Queue 4 · paused (‖ 4) | Quiet |
| uncertain or blocked | Check delivery (!) | Danger |
| all on hold | Queue 4 · all on hold (≡ 4) | Quiet |
| shepherd not running | Queue 4 · shepherd stopped (≡ 4) | Warning |

The popover (`PromptQueuePanel`) contains:

- a header with an **Auto** switch;
- a status line with the readiness reason and contextual actions (**Send next
  now**, **Resume**, and for uncertain items **Open Terminal**, **It was
  delivered**, **Return to queue**);
- groups Active, Urgent, Normal, Later, and a collapsed Archive.

Rows can be dragged inside a level to reorder and between levels to change
priority. The row menu offers Edit, Send now, Move to top, Hold or Release hold,
a priority choice and Delete…; archived rows offer Return to queue, Copy text and
Delete…. Deleting asks for confirmation. Keyboard: arrows move the selection,
Alt+Up/Down reorder, Enter edits, Delete deletes.

### Adding and editing

- **Add to queue** sits left of Send. Its arrow menu chooses Urgent, Normal
  (default) or Later and Hold; the chosen level persists for the session.
  Alt+Enter adds with the current choice. On success the composer clears and the
  chip flashes. On failure the draft stays, as with a failed send.
- **Edit** switches the composer to a separate local draft key
  `<session key>\nqueue:<item id>` filled from `queue item`. The session's own
  draft stays saved under its key and comes back automatically. A Warning chip
  "Editing queued prompt · Cancel" appears, and Send becomes **Save to queue**.
  Saving sends `update` with the item version; a stale version keeps the edit
  open with the error.

### Elsewhere

- Session rows show a small queue badge (`≡ 4`, `‖` when paused) from the `ls`
  summary.
- Activity marks user messages that came from the queue with a "from queue"
  tag. The input receipt records the origin, and the journal copies it onto the
  user event.
- `HgsClient::requestQueue(host, name, operation, payload)` wraps the CLI.
  Queue state for the selected session comes from `inspect`; other sessions use
  the `ls` summary.

## Testing

- Rust unit tests for the pure `readiness` function and the queue transitions:
  every blocking condition, quiet period, hold-only queues, interrupt, uncertain,
  three-strike blocked, rename, session end, archive caps.
- Rust store tests: add idempotency, versions, move between levels, attachment
  deduplication and cleanup, permissions.
- Integration tests with an isolated tmux socket and synthetic sessions, never
  the live server: dispatch into a fake agent, acknowledgement through
  hooks, a background task blocking dispatch, restart during dispatch.
- Packaging checks for `hgs-shepherd.service` and the `hgs-recovery.service`
  alias; installer migration tests with a fixture home.
- Qt tests: Add to queue and Alt+Enter, the edit draft key round trip, popover
  rendering for each readiness state, drag between levels, unsupported hosts.
  The toolbar preview gains queue chip states.

## Delivery order

1. Rename to shepherd (Part 1).
2. Queue store and CLI without dispatch (Part 2).
3. Readiness, dispatch and lifecycle in shepherd (Part 3), terminal agents first,
   then DeepSeek.
4. GUI chip, popover, add and edit (Part 4).
5. Session badge, Activity tag, documentation in `docs/reference.md`.
